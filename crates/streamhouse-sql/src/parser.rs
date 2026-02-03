//! SQL parser for StreamHouse queries

use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, Select, SelectItem,
    SetExpr, Statement, TableFactor, Value as SqlValue,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::error::SqlError;
use crate::types::*;
use crate::Result;

/// Parse a SQL query string into a SqlQuery
pub fn parse_query(sql: &str) -> Result<SqlQuery> {
    let dialect = GenericDialect {};
    let sql_trimmed = sql.trim();

    // Handle special commands that sqlparser doesn't understand
    let sql_upper = sql_trimmed.to_uppercase();
    if sql_upper.starts_with("SHOW TOPICS") {
        return Ok(SqlQuery::ShowTopics);
    }
    if sql_upper.starts_with("DESCRIBE ") || sql_upper.starts_with("DESC ") {
        let topic = sql_trimmed
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| SqlError::ParseError("DESCRIBE requires a topic name".to_string()))?
            .trim_matches(';')
            .to_string();
        return Ok(SqlQuery::DescribeTopic(topic));
    }

    let ast = Parser::parse_sql(&dialect, sql_trimmed)
        .map_err(|e| SqlError::ParseError(e.to_string()))?;

    if ast.is_empty() {
        return Err(SqlError::ParseError("Empty query".to_string()));
    }

    let statement = &ast[0];

    match statement {
        Statement::Query(query) => parse_select_query(query),
        _ => Err(SqlError::UnsupportedOperation(format!(
            "Only SELECT queries are supported, got: {:?}",
            statement
        ))),
    }
}

fn parse_select_query(query: &sqlparser::ast::Query) -> Result<SqlQuery> {
    let select = match &*query.body {
        SetExpr::Select(select) => select,
        _ => {
            return Err(SqlError::UnsupportedOperation(
                "Only simple SELECT queries are supported".to_string(),
            ))
        }
    };

    // Check for COUNT(*) query
    if is_count_query(select) {
        return parse_count_query(select, query);
    }

    // Parse FROM clause to get topic name
    let topic = parse_from_clause(select)?;

    // Parse SELECT columns
    let columns = parse_select_columns(select)?;

    // Parse WHERE clause
    let filters = if let Some(selection) = &select.selection {
        parse_where_clause(selection)?
    } else {
        vec![]
    };

    // Parse LIMIT
    let limit = query
        .limit
        .as_ref()
        .and_then(|e| extract_literal_int(e).map(|n| n as usize));

    // Parse OFFSET
    let offset = query
        .offset
        .as_ref()
        .and_then(|o| extract_literal_int(&o.value).map(|n| n as usize));

    // Parse ORDER BY
    let order_by = if !query.order_by.is_empty() {
        let first = &query.order_by[0];
        if let Expr::Identifier(ident) = &first.expr {
            Some(OrderBy {
                column: ident.value.clone(),
                descending: first.asc == Some(false),
            })
        } else {
            None
        }
    } else {
        None
    };

    Ok(SqlQuery::Select(SelectQuery {
        topic,
        columns,
        filters,
        order_by,
        limit,
        offset,
    }))
}

fn is_count_query(select: &Select) -> bool {
    if select.projection.len() != 1 {
        return false;
    }
    match &select.projection[0] {
        SelectItem::UnnamedExpr(Expr::Function(func)) => {
            func.name.to_string().to_uppercase() == "COUNT"
        }
        SelectItem::ExprWithAlias { expr: Expr::Function(func), .. } => {
            func.name.to_string().to_uppercase() == "COUNT"
        }
        _ => false,
    }
}

fn parse_count_query(select: &Select, _query: &sqlparser::ast::Query) -> Result<SqlQuery> {
    let topic = parse_from_clause(select)?;
    let filters = if let Some(selection) = &select.selection {
        parse_where_clause(selection)?
    } else {
        vec![]
    };

    Ok(SqlQuery::Count(CountQuery { topic, filters }))
}

fn parse_from_clause(select: &Select) -> Result<String> {
    if select.from.is_empty() {
        return Err(SqlError::ParseError(
            "SELECT requires a FROM clause".to_string(),
        ));
    }

    let table = &select.from[0];
    match &table.relation {
        TableFactor::Table { name, .. } => Ok(name.to_string()),
        _ => Err(SqlError::UnsupportedOperation(
            "Only simple table references are supported".to_string(),
        )),
    }
}

fn parse_select_columns(select: &Select) -> Result<Vec<SelectColumn>> {
    let mut columns = vec![];

    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => {
                columns.push(SelectColumn::All);
            }
            SelectItem::UnnamedExpr(expr) => {
                columns.push(parse_select_expr(expr, None)?);
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                columns.push(parse_select_expr(expr, Some(alias.value.clone()))?);
            }
            _ => {
                return Err(SqlError::UnsupportedOperation(format!(
                    "Unsupported select item: {:?}",
                    item
                )))
            }
        }
    }

    Ok(columns)
}

fn parse_select_expr(expr: &Expr, alias: Option<String>) -> Result<SelectColumn> {
    match expr {
        Expr::Identifier(ident) => Ok(SelectColumn::Column(ident.value.clone())),
        Expr::Function(func) => {
            let func_name = func.name.to_string().to_uppercase();
            if func_name == "JSON_EXTRACT" {
                parse_json_extract_func(func, alias)
            } else {
                Err(SqlError::UnsupportedOperation(format!(
                    "Unsupported function: {}",
                    func_name
                )))
            }
        }
        _ => Err(SqlError::UnsupportedOperation(format!(
            "Unsupported expression: {:?}",
            expr
        ))),
    }
}

fn parse_json_extract_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let args = &func.args;
    if args.len() != 2 {
        return Err(SqlError::ParseError(
            "json_extract requires 2 arguments".to_string(),
        ));
    }

    let column = match &args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(ident))) => {
            ident.value.clone()
        }
        _ => {
            return Err(SqlError::ParseError(
                "First argument to json_extract must be a column name".to_string(),
            ))
        }
    };

    let path = match &args[1] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(s)))) => {
            s.clone()
        }
        _ => {
            return Err(SqlError::ParseError(
                "Second argument to json_extract must be a string path".to_string(),
            ))
        }
    };

    Ok(SelectColumn::JsonExtract {
        column,
        path,
        alias,
    })
}

fn parse_where_clause(expr: &Expr) -> Result<Vec<Filter>> {
    let mut filters = vec![];
    collect_filters(expr, &mut filters)?;
    Ok(filters)
}

fn collect_filters(expr: &Expr, filters: &mut Vec<Filter>) -> Result<()> {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            match op {
                BinaryOperator::And => {
                    collect_filters(left, filters)?;
                    collect_filters(right, filters)?;
                }
                BinaryOperator::Eq => {
                    if let Some(filter) = parse_equality_filter(left, right)? {
                        filters.push(filter);
                    }
                }
                BinaryOperator::Gt => {
                    if let Some(filter) = parse_comparison_filter(left, right, true, false)? {
                        filters.push(filter);
                    }
                }
                BinaryOperator::GtEq => {
                    if let Some(filter) = parse_comparison_filter(left, right, true, true)? {
                        filters.push(filter);
                    }
                }
                BinaryOperator::Lt => {
                    if let Some(filter) = parse_comparison_filter(left, right, false, false)? {
                        filters.push(filter);
                    }
                }
                BinaryOperator::LtEq => {
                    if let Some(filter) = parse_comparison_filter(left, right, false, true)? {
                        filters.push(filter);
                    }
                }
                _ => {
                    return Err(SqlError::UnsupportedOperation(format!(
                        "Unsupported operator: {:?}",
                        op
                    )))
                }
            }
        }
        Expr::Nested(inner) => {
            collect_filters(inner, filters)?;
        }
        _ => {
            return Err(SqlError::UnsupportedOperation(format!(
                "Unsupported WHERE expression: {:?}",
                expr
            )))
        }
    }
    Ok(())
}

fn parse_equality_filter(left: &Expr, right: &Expr) -> Result<Option<Filter>> {
    // Handle: column = value
    if let Expr::Identifier(ident) = left {
        let column = ident.value.to_lowercase();
        match column.as_str() {
            "key" => {
                if let Some(s) = extract_string_value(right) {
                    return Ok(Some(Filter::KeyEquals(s)));
                }
            }
            "partition" => {
                if let Some(n) = extract_literal_int(right) {
                    return Ok(Some(Filter::PartitionEquals(n as u32)));
                }
            }
            "offset" => {
                if let Some(n) = extract_literal_int(right) {
                    return Ok(Some(Filter::OffsetEquals(n as u64)));
                }
            }
            _ => {}
        }
    }

    // Handle: json_extract(value, '$.path') = value
    if let Expr::Function(func) = left {
        let func_name = func.name.to_string().to_uppercase();
        if func_name == "JSON_EXTRACT" {
            if let Some((path, _)) = parse_json_extract_args(func)? {
                let value = expr_to_json_value(right)?;
                return Ok(Some(Filter::JsonEquals { path, value }));
            }
        }
    }

    Ok(None)
}

fn parse_comparison_filter(
    left: &Expr,
    right: &Expr,
    is_greater: bool,
    is_equal: bool,
) -> Result<Option<Filter>> {
    // Handle: column > value, column >= value, column < value, column <= value
    if let Expr::Identifier(ident) = left {
        let column = ident.value.to_lowercase();
        match column.as_str() {
            "offset" => {
                if let Some(n) = extract_literal_int(right) {
                    return Ok(Some(if is_greater {
                        if is_equal {
                            Filter::OffsetGte(n as u64)
                        } else {
                            Filter::OffsetGte((n + 1) as u64) // > N means >= N+1
                        }
                    } else if is_equal {
                        Filter::OffsetLt((n + 1) as u64) // <= N means < N+1
                    } else {
                        Filter::OffsetLt(n as u64)
                    }));
                }
            }
            "timestamp" => {
                if let Some(ts) = extract_timestamp_value(right) {
                    return Ok(Some(if is_greater {
                        Filter::TimestampGte(ts)
                    } else {
                        Filter::TimestampLt(ts)
                    }));
                }
            }
            _ => {}
        }
    }

    // Handle: json_extract(value, '$.path') > value
    if let Expr::Function(func) = left {
        let func_name = func.name.to_string().to_uppercase();
        if func_name == "JSON_EXTRACT" {
            if let Some((path, _)) = parse_json_extract_args(func)? {
                let value = expr_to_json_value(right)?;
                return Ok(Some(if is_greater {
                    Filter::JsonGt { path, value }
                } else {
                    Filter::JsonLt { path, value }
                }));
            }
        }
    }

    Ok(None)
}

fn parse_json_extract_args(func: &Function) -> Result<Option<(String, String)>> {
    let args = &func.args;
    if args.len() != 2 {
        return Ok(None);
    }

    let column = match &args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(ident))) => {
            ident.value.clone()
        }
        _ => return Ok(None),
    };

    let path = match &args[1] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(s)))) => {
            s.clone()
        }
        _ => return Ok(None),
    };

    Ok(Some((path, column)))
}

fn extract_string_value(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(SqlValue::SingleQuotedString(s)) => Some(s.clone()),
        Expr::Value(SqlValue::DoubleQuotedString(s)) => Some(s.clone()),
        _ => None,
    }
}

fn extract_literal_int(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Value(SqlValue::Number(n, _)) => n.parse().ok(),
        _ => None,
    }
}

fn extract_timestamp_value(expr: &Expr) -> Option<i64> {
    let s = extract_string_value(expr)?;
    // Try to parse as ISO 8601
    chrono::DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.timestamp_millis())
        .ok()
}

fn expr_to_json_value(expr: &Expr) -> Result<serde_json::Value> {
    match expr {
        Expr::Value(SqlValue::Number(n, _)) => {
            if let Ok(i) = n.parse::<i64>() {
                Ok(serde_json::Value::Number(i.into()))
            } else if let Ok(f) = n.parse::<f64>() {
                Ok(serde_json::json!(f))
            } else {
                Err(SqlError::ParseError(format!("Invalid number: {}", n)))
            }
        }
        Expr::Value(SqlValue::SingleQuotedString(s)) => Ok(serde_json::Value::String(s.clone())),
        Expr::Value(SqlValue::Boolean(b)) => Ok(serde_json::Value::Bool(*b)),
        Expr::Value(SqlValue::Null) => Ok(serde_json::Value::Null),
        _ => Err(SqlError::ParseError(format!(
            "Cannot convert expression to JSON: {:?}",
            expr
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_select() {
        let query = parse_query("SELECT * FROM orders LIMIT 10").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.topic, "orders");
                assert!(matches!(q.columns[0], SelectColumn::All));
                assert_eq!(q.limit, Some(10));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_clause() {
        let query = parse_query("SELECT * FROM orders WHERE partition = 0 AND offset >= 100").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 2);
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_show_topics() {
        let query = parse_query("SHOW TOPICS").unwrap();
        assert!(matches!(query, SqlQuery::ShowTopics));
    }

    #[test]
    fn test_parse_describe() {
        let query = parse_query("DESCRIBE orders").unwrap();
        match query {
            SqlQuery::DescribeTopic(name) => assert_eq!(name, "orders"),
            _ => panic!("Expected DescribeTopic"),
        }
    }

    #[test]
    fn test_parse_count() {
        let query = parse_query("SELECT COUNT(*) FROM orders WHERE partition = 0").unwrap();
        match query {
            SqlQuery::Count(q) => {
                assert_eq!(q.topic, "orders");
                assert_eq!(q.filters.len(), 1);
            }
            _ => panic!("Expected Count query"),
        }
    }
}
