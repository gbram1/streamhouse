//! SQL parser for StreamHouse queries

use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, Join, JoinConstraint,
    JoinOperator, Select, SelectItem, SetExpr, Statement, TableFactor, Value as SqlValue,
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

    // Handle SHOW MATERIALIZED VIEWS
    if sql_upper.starts_with("SHOW MATERIALIZED VIEWS") {
        return Ok(SqlQuery::ShowMaterializedViews);
    }

    // Handle DESCRIBE MATERIALIZED VIEW <name>
    if sql_upper.starts_with("DESCRIBE MATERIALIZED VIEW ") {
        let name = sql_trimmed
            .split_whitespace()
            .nth(3)
            .ok_or_else(|| {
                SqlError::ParseError("DESCRIBE MATERIALIZED VIEW requires a view name".to_string())
            })?
            .trim_matches(';')
            .to_string();
        return Ok(SqlQuery::DescribeMaterializedView(name));
    }

    // Handle regular DESCRIBE (must come after DESCRIBE MATERIALIZED VIEW)
    if sql_upper.starts_with("DESCRIBE ") || sql_upper.starts_with("DESC ") {
        let topic = sql_trimmed
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| SqlError::ParseError("DESCRIBE requires a topic name".to_string()))?
            .trim_matches(';')
            .trim_matches('"')
            .to_string();
        return Ok(SqlQuery::DescribeTopic(topic));
    }

    // Handle DROP MATERIALIZED VIEW <name>
    if sql_upper.starts_with("DROP MATERIALIZED VIEW ") {
        let name = sql_trimmed
            .split_whitespace()
            .nth(3)
            .ok_or_else(|| {
                SqlError::ParseError("DROP MATERIALIZED VIEW requires a view name".to_string())
            })?
            .trim_matches(';')
            .to_string();
        return Ok(SqlQuery::DropMaterializedView(name));
    }

    // Handle REFRESH MATERIALIZED VIEW <name>
    if sql_upper.starts_with("REFRESH MATERIALIZED VIEW ") {
        let name = sql_trimmed
            .split_whitespace()
            .nth(3)
            .ok_or_else(|| {
                SqlError::ParseError("REFRESH MATERIALIZED VIEW requires a view name".to_string())
            })?
            .trim_matches(';')
            .to_string();
        return Ok(SqlQuery::RefreshMaterializedView(name));
    }

    // Handle CREATE [OR REPLACE] MATERIALIZED VIEW <name> AS SELECT...
    if sql_upper.starts_with("CREATE MATERIALIZED VIEW ")
        || sql_upper.starts_with("CREATE OR REPLACE MATERIALIZED VIEW ")
    {
        return parse_create_materialized_view(sql_trimmed);
    }

    let ast = Parser::parse_sql(&dialect, sql_trimmed)
        .map_err(|e| SqlError::ParseError(e.to_string()))?;

    if ast.is_empty() {
        return Err(SqlError::ParseError("Empty query".to_string()));
    }

    let statement = &ast[0];

    match statement {
        Statement::Query(query) => parse_select_query(query, sql_trimmed),
        _ => Err(SqlError::UnsupportedOperation(format!(
            "Only SELECT queries are supported, got: {:?}",
            statement
        ))),
    }
}

fn parse_select_query(query: &sqlparser::ast::Query, original_sql: &str) -> Result<SqlQuery> {
    let select = match &*query.body {
        SetExpr::Select(select) => select,
        _ => {
            return Err(SqlError::UnsupportedOperation(
                "Only simple SELECT queries are supported".to_string(),
            ))
        }
    };

    // Check for JOIN query first (before window/count checks)
    if let Some(join_query) = try_parse_join_query(select, query)? {
        return Ok(SqlQuery::Join(join_query));
    }

    // Check for window aggregate query (GROUP BY with TUMBLE/HOP/SESSION)
    if let Some(window_query) = try_parse_window_query(select, query, original_sql)? {
        return Ok(SqlQuery::WindowAggregate(window_query));
    }

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

/// Try to parse as a JOIN query
fn try_parse_join_query(
    select: &Select,
    query: &sqlparser::ast::Query,
) -> Result<Option<JoinQuery>> {
    // Check if FROM clause has a JOIN
    if select.from.is_empty() {
        return Ok(None);
    }

    let table_with_joins = &select.from[0];
    if table_with_joins.joins.is_empty() {
        return Ok(None);
    }

    // Parse left table
    let left = parse_table_ref(&table_with_joins.relation)?;

    // Parse the first join (we only support one join for now)
    let join = &table_with_joins.joins[0];

    // Parse join type
    let join_type = match &join.join_operator {
        JoinOperator::Inner(_) => JoinType::Inner,
        JoinOperator::LeftOuter(_) => JoinType::Left,
        JoinOperator::RightOuter(_) => JoinType::Right,
        JoinOperator::FullOuter(_) => JoinType::Full,
        _ => {
            return Err(SqlError::UnsupportedOperation(
                "Only INNER, LEFT, RIGHT, and FULL joins are supported".to_string(),
            ))
        }
    };

    // Parse right table
    let right = parse_table_ref(&join.relation)?;

    // Parse ON clause
    let condition = parse_join_condition(join)?;

    // Parse SELECT columns for join
    let columns = parse_join_select_columns(select, &left, &right)?;

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

    // Parse ORDER BY
    let order_by = if !query.order_by.is_empty() {
        let first = &query.order_by[0];
        if let Expr::Identifier(ident) = &first.expr {
            Some(OrderBy {
                column: ident.value.clone(),
                descending: first.asc == Some(false),
            })
        } else if let Expr::CompoundIdentifier(parts) = &first.expr {
            // Handle qualified column like o.amount
            let col = parts
                .iter()
                .map(|p| p.value.as_str())
                .collect::<Vec<_>>()
                .join(".");
            Some(OrderBy {
                column: col,
                descending: first.asc == Some(false),
            })
        } else {
            None
        }
    } else {
        None
    };

    Ok(Some(JoinQuery {
        left,
        right,
        join_type,
        condition,
        columns,
        filters,
        order_by,
        limit,
        window_ms: Some(3600000), // Default 1 hour window
    }))
}

/// Parse a table reference (topic with optional alias)
fn parse_table_ref(table_factor: &TableFactor) -> Result<TableRef> {
    match table_factor {
        TableFactor::Table { name, alias, .. } => {
            let topic = name.to_string();
            let alias_str = alias.as_ref().map(|a| a.name.value.clone());
            Ok(TableRef {
                topic,
                alias: alias_str,
                is_table: false,
            })
        }
        // TABLE(topic) syntax for stream-table joins
        // sqlparser parses this as TableFunction { expr, alias }
        TableFactor::TableFunction { expr, alias } => {
            // Extract topic name from expression
            let topic = match expr {
                Expr::Identifier(ident) => ident.value.clone(),
                Expr::Value(SqlValue::SingleQuotedString(s)) => s.clone(),
                Expr::CompoundIdentifier(parts) => parts
                    .iter()
                    .map(|p| p.value.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
                _ => {
                    return Err(SqlError::ParseError(
                        "TABLE() argument must be a topic name".to_string(),
                    ))
                }
            };
            let alias_str = alias.as_ref().map(|a| a.name.value.clone());
            Ok(TableRef {
                topic,
                alias: alias_str,
                is_table: true, // Mark as table reference for O(1) lookups
            })
        }
        // Also handle LATERAL TABLE() or general function syntax
        TableFactor::Function {
            name, args, alias, ..
        } => {
            let func_name = name.to_string().to_uppercase();
            if func_name == "TABLE" {
                // Extract topic name from args
                if args.is_empty() {
                    return Err(SqlError::ParseError(
                        "TABLE() requires a topic name argument".to_string(),
                    ));
                }
                let topic = match &args[0] {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(ident))) => {
                        ident.value.clone()
                    }
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        SqlValue::SingleQuotedString(s),
                    ))) => s.clone(),
                    _ => {
                        return Err(SqlError::ParseError(
                            "TABLE() argument must be a topic name".to_string(),
                        ))
                    }
                };
                let alias_str = alias.as_ref().map(|a| a.name.value.clone());
                Ok(TableRef {
                    topic,
                    alias: alias_str,
                    is_table: true, // Mark as table reference for O(1) lookups
                })
            } else {
                Err(SqlError::UnsupportedOperation(format!(
                    "Unsupported table function: {}. Use TABLE(topic) for stream-table joins.",
                    func_name
                )))
            }
        }
        _ => Err(SqlError::UnsupportedOperation(
            "Unsupported table reference type".to_string(),
        )),
    }
}

/// Parse JOIN ON clause to extract join condition
fn parse_join_condition(join: &Join) -> Result<JoinCondition> {
    let constraint = match &join.join_operator {
        JoinOperator::Inner(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c) => c,
        _ => {
            return Err(SqlError::UnsupportedOperation(
                "Join must have ON clause".to_string(),
            ))
        }
    };

    match constraint {
        JoinConstraint::On(expr) => parse_join_on_expr(expr),
        JoinConstraint::Using(_) => Err(SqlError::UnsupportedOperation(
            "USING clause not supported, use ON instead".to_string(),
        )),
        JoinConstraint::Natural => Err(SqlError::UnsupportedOperation(
            "NATURAL join not supported".to_string(),
        )),
        JoinConstraint::None => Err(SqlError::UnsupportedOperation(
            "Join must have ON clause".to_string(),
        )),
    }
}

/// Parse ON expression (e.g., o.user_id = u.id)
fn parse_join_on_expr(expr: &Expr) -> Result<JoinCondition> {
    match expr {
        Expr::BinaryOp { left, op, right } if *op == BinaryOperator::Eq => {
            let left_ref = parse_column_ref(left)?;
            let right_ref = parse_column_ref(right)?;
            Ok(JoinCondition {
                left: left_ref,
                right: right_ref,
            })
        }
        _ => Err(SqlError::UnsupportedOperation(
            "Only equality conditions (=) are supported in ON clause".to_string(),
        )),
    }
}

/// Parse a column reference (e.g., o.user_id or json_extract(o.value, '$.user_id'))
fn parse_column_ref(expr: &Expr) -> Result<(String, String)> {
    match expr {
        // Handle: o.column_name
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let qualifier = parts[0].value.clone();
            let column = parts[1].value.clone();
            // Convert column name to path if it's a known field
            let path = match column.as_str() {
                "key" => "key".to_string(),
                "value" => "value".to_string(),
                "offset" => "offset".to_string(),
                "timestamp" => "timestamp".to_string(),
                "partition" => "partition".to_string(),
                _ => format!("$.{}", column), // Assume it's a JSON path
            };
            Ok((qualifier, path))
        }
        // Handle: json_extract(o.value, '$.field')
        Expr::Function(func) => {
            let func_name = func.name.to_string().to_uppercase();
            if func_name == "JSON_EXTRACT" {
                let args = &func.args;
                if args.len() != 2 {
                    return Err(SqlError::ParseError(
                        "json_extract requires 2 arguments".to_string(),
                    ));
                }

                // Get qualifier from first arg (e.g., o.value)
                let qualifier = match &args[0] {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::CompoundIdentifier(
                        parts,
                    ))) => {
                        if !parts.is_empty() {
                            parts[0].value.clone()
                        } else {
                            return Err(SqlError::ParseError(
                                "Expected qualified column in json_extract".to_string(),
                            ));
                        }
                    }
                    _ => {
                        return Err(SqlError::ParseError(
                            "Expected qualified column in json_extract".to_string(),
                        ))
                    }
                };

                // Get path from second arg
                let path = match &args[1] {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                        SqlValue::SingleQuotedString(s),
                    ))) => s.clone(),
                    _ => {
                        return Err(SqlError::ParseError(
                            "Second argument to json_extract must be a string path".to_string(),
                        ))
                    }
                };

                Ok((qualifier, path))
            } else {
                Err(SqlError::UnsupportedOperation(format!(
                    "Function {} not supported in join condition",
                    func_name
                )))
            }
        }
        _ => Err(SqlError::UnsupportedOperation(
            "Unsupported expression in join condition".to_string(),
        )),
    }
}

/// Parse SELECT columns for JOIN query
fn parse_join_select_columns(
    select: &Select,
    left: &TableRef,
    right: &TableRef,
) -> Result<Vec<JoinSelectColumn>> {
    let mut columns = Vec::new();

    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => {
                columns.push(JoinSelectColumn::AllFrom(None));
            }
            SelectItem::QualifiedWildcard(name, _) => {
                let qualifier = name.to_string();
                columns.push(JoinSelectColumn::AllFrom(Some(qualifier)));
            }
            SelectItem::UnnamedExpr(expr) => {
                columns.push(parse_join_select_expr(expr, None, left, right)?);
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                columns.push(parse_join_select_expr(
                    expr,
                    Some(alias.value.clone()),
                    left,
                    right,
                )?);
            }
        }
    }

    Ok(columns)
}

/// Parse a single SELECT expression in a JOIN query
fn parse_join_select_expr(
    expr: &Expr,
    alias: Option<String>,
    _left: &TableRef,
    _right: &TableRef,
) -> Result<JoinSelectColumn> {
    match expr {
        // Handle: o.column_name
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let qualifier = parts[0].value.clone();
            let column = parts[1].value.clone();
            Ok(JoinSelectColumn::QualifiedColumn {
                qualifier,
                column,
                alias,
            })
        }
        // Handle: json_extract(o.value, '$.field')
        Expr::Function(func) => {
            let func_name = func.name.to_string().to_uppercase();
            if func_name == "JSON_EXTRACT" {
                let (qualifier, path) = parse_json_extract_for_join(func)?;
                Ok(JoinSelectColumn::QualifiedJsonExtract {
                    qualifier,
                    path,
                    alias,
                })
            } else {
                Err(SqlError::UnsupportedOperation(format!(
                    "Function {} not yet supported in JOIN select",
                    func_name
                )))
            }
        }
        // Handle unqualified column (will be resolved at runtime)
        Expr::Identifier(ident) => {
            Ok(JoinSelectColumn::QualifiedColumn {
                qualifier: String::new(), // Empty means "resolve at runtime"
                column: ident.value.clone(),
                alias,
            })
        }
        _ => Err(SqlError::UnsupportedOperation(format!(
            "Unsupported expression in JOIN select: {:?}",
            expr
        ))),
    }
}

/// Parse json_extract for JOIN (returns qualifier and path)
fn parse_json_extract_for_join(func: &Function) -> Result<(String, String)> {
    let args = &func.args;
    if args.len() != 2 {
        return Err(SqlError::ParseError(
            "json_extract requires 2 arguments".to_string(),
        ));
    }

    // Get qualifier from first arg (e.g., o.value)
    let qualifier = match &args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::CompoundIdentifier(parts))) => {
            if !parts.is_empty() {
                parts[0].value.clone()
            } else {
                return Err(SqlError::ParseError(
                    "Expected qualified column in json_extract".to_string(),
                ));
            }
        }
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(ident))) => {
            // Unqualified - use empty string
            ident.value.clone()
        }
        _ => {
            return Err(SqlError::ParseError(
                "Expected column in json_extract".to_string(),
            ))
        }
    };

    // Get path from second arg
    let path = match &args[1] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => s.clone(),
        _ => {
            return Err(SqlError::ParseError(
                "Second argument to json_extract must be a string path".to_string(),
            ))
        }
    };

    Ok((qualifier, path))
}

/// Try to parse as a window aggregate query
fn try_parse_window_query(
    select: &Select,
    query: &sqlparser::ast::Query,
    _original_sql: &str,
) -> Result<Option<WindowAggregateQuery>> {
    // Extract expressions from GROUP BY
    let group_exprs = match &select.group_by {
        sqlparser::ast::GroupByExpr::All => return Ok(None),
        sqlparser::ast::GroupByExpr::Expressions(exprs) => exprs,
    };

    // Check if GROUP BY contains expressions
    if group_exprs.is_empty() {
        return Ok(None);
    }

    // Look for TUMBLE, HOP, or SESSION in the GROUP BY
    let mut window_type: Option<WindowType> = None;
    let mut other_group_by: Vec<String> = Vec::new();

    for group_expr in group_exprs {
        if let Expr::Function(func) = group_expr {
            let func_name = func.name.to_string().to_uppercase();
            match func_name.as_str() {
                "TUMBLE" => {
                    window_type = Some(parse_tumble_window(func)?);
                }
                "HOP" => {
                    window_type = Some(parse_hop_window(func)?);
                }
                "SESSION" => {
                    window_type = Some(parse_session_window(func)?);
                }
                _ => {
                    // Regular GROUP BY expression — extract JSON path if json_extract
                    if func_name == "JSON_EXTRACT" {
                        if let Ok(path) = extract_path_from_json_extract(func) {
                            other_group_by.push(path);
                        } else {
                            other_group_by.push(func.name.to_string());
                        }
                    } else {
                        other_group_by.push(func.name.to_string());
                    }
                }
            }
        } else if let Expr::Identifier(ident) = group_expr {
            other_group_by.push(ident.value.clone());
        }
    }

    let window = match window_type {
        Some(w) => w,
        None => {
            // No window function in GROUP BY — check if SELECT contains aggregate functions.
            // If so, treat this as a non-windowed GROUP BY with a single all-encompassing window.
            let aggs = parse_window_aggregations(select)?;
            if aggs.is_empty() {
                return Ok(None); // No aggregates, not a GROUP BY aggregate query
            }
            // Use a very large tumble window (100 years) to put all records in one window
            WindowType::Tumble {
                size_ms: 100 * 365 * 24 * 60 * 60 * 1000,
            }
        }
    };

    // Parse the rest of the query
    let topic = parse_from_clause(select)?;

    // Parse aggregations from SELECT
    let aggregations = parse_window_aggregations(select)?;

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

    Ok(Some(WindowAggregateQuery {
        topic,
        window,
        aggregations,
        group_by: other_group_by,
        filters,
        limit,
    }))
}

/// Parse TUMBLE(timestamp, INTERVAL 'duration')
fn parse_tumble_window(func: &Function) -> Result<WindowType> {
    let args = &func.args;
    if args.len() < 2 {
        return Err(SqlError::ParseError(
            "TUMBLE requires at least 2 arguments: TUMBLE(timestamp_column, size)".to_string(),
        ));
    }

    // Parse window size (second argument)
    let size_ms = parse_interval_arg(&args[1])?;

    Ok(WindowType::Tumble { size_ms })
}

/// Parse HOP(timestamp, size, slide)
fn parse_hop_window(func: &Function) -> Result<WindowType> {
    let args = &func.args;
    if args.len() < 3 {
        return Err(SqlError::ParseError(
            "HOP requires 3 arguments: HOP(timestamp_column, size, slide)".to_string(),
        ));
    }

    let size_ms = parse_interval_arg(&args[1])?;
    let slide_ms = parse_interval_arg(&args[2])?;

    Ok(WindowType::Hop { size_ms, slide_ms })
}

/// Parse SESSION(timestamp, gap)
fn parse_session_window(func: &Function) -> Result<WindowType> {
    let args = &func.args;
    if args.len() < 2 {
        return Err(SqlError::ParseError(
            "SESSION requires 2 arguments: SESSION(timestamp_column, gap)".to_string(),
        ));
    }

    let gap_ms = parse_interval_arg(&args[1])?;

    Ok(WindowType::Session { gap_ms })
}

/// Parse an interval argument (e.g., INTERVAL '5 minutes' or just a number in ms)
fn parse_interval_arg(arg: &FunctionArg) -> Result<i64> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => parse_interval_expr(expr),
        _ => Err(SqlError::ParseError(
            "Invalid interval argument".to_string(),
        )),
    }
}

/// Parse interval expression
fn parse_interval_expr(expr: &Expr) -> Result<i64> {
    match expr {
        // Handle: INTERVAL '5 minutes'
        Expr::Interval(interval) => {
            let value_str = match &*interval.value {
                Expr::Value(SqlValue::SingleQuotedString(s)) => s.clone(),
                _ => {
                    return Err(SqlError::ParseError(
                        "Interval value must be a string".to_string(),
                    ))
                }
            };
            parse_interval_string(&value_str)
        }
        // Handle: plain number (milliseconds)
        Expr::Value(SqlValue::Number(n, _)) => n
            .parse::<i64>()
            .map_err(|_| SqlError::ParseError("Invalid interval number".to_string())),
        // Handle: string like '5 minutes'
        Expr::Value(SqlValue::SingleQuotedString(s)) => parse_interval_string(s),
        _ => Err(SqlError::ParseError(format!(
            "Unsupported interval expression: {:?}",
            expr
        ))),
    }
}

/// Parse interval string like "5 minutes", "1 hour", "30 seconds"
fn parse_interval_string(s: &str) -> Result<i64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return Err(SqlError::ParseError("Empty interval string".to_string()));
    }

    let value: i64 = parts[0]
        .parse()
        .map_err(|_| SqlError::ParseError(format!("Invalid interval value: {}", parts[0])))?;

    let unit = if parts.len() > 1 {
        parts[1].to_lowercase()
    } else {
        "milliseconds".to_string()
    };

    let multiplier = match unit.as_str() {
        "ms" | "millisecond" | "milliseconds" => 1,
        "s" | "sec" | "second" | "seconds" => 1000,
        "m" | "min" | "minute" | "minutes" => 60 * 1000,
        "h" | "hour" | "hours" => 60 * 60 * 1000,
        "d" | "day" | "days" => 24 * 60 * 60 * 1000,
        _ => {
            return Err(SqlError::ParseError(format!(
                "Unknown interval unit: {}",
                unit
            )))
        }
    };

    Ok(value * multiplier)
}

// ============================================================================
// Materialized View Parsing
// ============================================================================

/// Parse CREATE [OR REPLACE] MATERIALIZED VIEW statement
///
/// Syntax:
///   CREATE [OR REPLACE] MATERIALIZED VIEW <name>
///   [WITH (refresh_mode = 'continuous'|'manual'|'periodic', interval = '5 minutes')]
///   AS SELECT ... FROM <topic> GROUP BY TUMBLE/HOP/SESSION(...)
fn parse_create_materialized_view(sql: &str) -> Result<SqlQuery> {
    let sql_upper = sql.to_uppercase();

    // Check for OR REPLACE
    let or_replace = sql_upper.contains("OR REPLACE");

    // Extract view name
    let name_start = sql_upper.find("MATERIALIZED VIEW").unwrap() + 17;

    // Find the AS keyword
    let as_pos = sql_upper.find(" AS ").ok_or_else(|| {
        SqlError::ParseError("CREATE MATERIALIZED VIEW requires AS keyword".to_string())
    })?;

    let name_part = sql[name_start..as_pos].trim();

    // Parse optional WITH clause and extract name
    let (name, refresh_mode) = if let Some(with_pos) = name_part.to_uppercase().find(" WITH ") {
        let view_name = name_part[..with_pos].trim().to_string();
        let with_clause = &name_part[with_pos + 6..];
        let refresh = parse_refresh_mode(with_clause)?;
        (view_name, refresh)
    } else {
        (name_part.to_string(), RefreshMode::Continuous)
    };

    // Extract the SELECT query after AS
    let select_sql = &sql[as_pos + 4..];

    // Parse the underlying SELECT query
    let dialect = GenericDialect {};
    let ast = Parser::parse_sql(&dialect, select_sql)
        .map_err(|e| SqlError::ParseError(format!("Invalid SELECT in materialized view: {}", e)))?;

    if ast.is_empty() {
        return Err(SqlError::ParseError(
            "Empty SELECT query in materialized view".to_string(),
        ));
    }

    let statement = &ast[0];

    match statement {
        Statement::Query(query) => {
            let select = match &*query.body {
                SetExpr::Select(select) => select,
                _ => {
                    return Err(SqlError::UnsupportedOperation(
                        "Materialized views must use a simple SELECT query".to_string(),
                    ))
                }
            };

            // Parse FROM clause to get source topic
            let source_topic = parse_from_clause(select)?;

            // Parse window and aggregations (materialized views typically use window aggregations)
            let (window, aggregations, group_by) = parse_mv_window_clause(select)?;

            // Parse WHERE clause filters
            let filters = if let Some(selection) = &select.selection {
                parse_where_clause(selection)?
            } else {
                vec![]
            };

            Ok(SqlQuery::CreateMaterializedView(
                CreateMaterializedViewQuery {
                    name,
                    source_topic,
                    query_sql: select_sql.trim().to_string(),
                    window,
                    aggregations,
                    group_by,
                    filters,
                    refresh_mode,
                    or_replace,
                },
            ))
        }
        _ => Err(SqlError::UnsupportedOperation(
            "Materialized views must use a SELECT query".to_string(),
        )),
    }
}

/// Parse the refresh mode from WITH clause
/// Syntax: WITH (refresh_mode = 'continuous', interval = '5 minutes')
fn parse_refresh_mode(with_clause: &str) -> Result<RefreshMode> {
    let lower = with_clause.to_lowercase();
    let trimmed = lower.trim().trim_start_matches('(').trim_end_matches(')');

    // Simple parsing of key=value pairs
    let mut refresh_mode = "continuous";
    let mut interval_str: Option<&str> = None;

    for part in trimmed.split(',') {
        let kv: Vec<&str> = part.split('=').map(|s| s.trim()).collect();
        if kv.len() == 2 {
            let key = kv[0].trim_matches(|c| c == '\'' || c == '"');
            let value = kv[1].trim_matches(|c| c == '\'' || c == '"');

            match key {
                "refresh_mode" | "refresh" | "mode" => refresh_mode = value,
                "interval" => interval_str = Some(value),
                _ => {}
            }
        }
    }

    match refresh_mode {
        "continuous" | "stream" | "streaming" => Ok(RefreshMode::Continuous),
        "manual" | "on_demand" => Ok(RefreshMode::Manual),
        "periodic" | "scheduled" => {
            let interval_ms = if let Some(interval) = interval_str {
                parse_interval_string(interval)?
            } else {
                // Default to 5 minutes if not specified
                5 * 60 * 1000
            };
            Ok(RefreshMode::Periodic { interval_ms })
        }
        _ => Err(SqlError::ParseError(format!(
            "Unknown refresh mode: '{}'. Use 'continuous', 'manual', or 'periodic'",
            refresh_mode
        ))),
    }
}

/// Parse window specification and aggregations for materialized views
fn parse_mv_window_clause(
    select: &Select,
) -> Result<(Option<WindowType>, Vec<WindowAggregation>, Vec<String>)> {
    // Extract expressions from GROUP BY
    let group_exprs = match &select.group_by {
        sqlparser::ast::GroupByExpr::All => return Ok((None, vec![], vec![])),
        sqlparser::ast::GroupByExpr::Expressions(exprs) => exprs,
    };

    if group_exprs.is_empty() {
        // No GROUP BY - parse as simple aggregations
        let aggregations = parse_window_aggregations(select)?;
        return Ok((None, aggregations, vec![]));
    }

    // Look for TUMBLE, HOP, or SESSION in the GROUP BY
    let mut window_type: Option<WindowType> = None;
    let mut other_group_by: Vec<String> = Vec::new();

    for group_expr in group_exprs {
        if let Expr::Function(func) = group_expr {
            let func_name = func.name.to_string().to_uppercase();
            match func_name.as_str() {
                "TUMBLE" => {
                    window_type = Some(parse_tumble_window(func)?);
                }
                "HOP" => {
                    window_type = Some(parse_hop_window(func)?);
                }
                "SESSION" => {
                    window_type = Some(parse_session_window(func)?);
                }
                _ => {
                    other_group_by.push(func.name.to_string());
                }
            }
        } else if let Expr::Identifier(ident) = group_expr {
            other_group_by.push(ident.value.clone());
        }
    }

    // Parse aggregations from SELECT clause
    let aggregations = parse_window_aggregations(select)?;

    Ok((window_type, aggregations, other_group_by))
}

/// Parse aggregations from SELECT clause for window query
fn parse_window_aggregations(select: &Select) -> Result<Vec<WindowAggregation>> {
    let mut aggregations = Vec::new();

    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) => {
                if let Some(agg) = try_parse_aggregation(expr, None)? {
                    aggregations.push(agg);
                }
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                if let Some(agg) = try_parse_aggregation(expr, Some(alias.value.clone()))? {
                    aggregations.push(agg);
                }
            }
            _ => {}
        }
    }

    Ok(aggregations)
}

/// Try to parse an expression as an aggregation function
fn try_parse_aggregation(expr: &Expr, alias: Option<String>) -> Result<Option<WindowAggregation>> {
    if let Expr::Function(func) = expr {
        let func_name = func.name.to_string().to_uppercase();
        let args = &func.args;

        match func_name.as_str() {
            "COUNT" => {
                // Check for COUNT(DISTINCT ...)
                if func.distinct {
                    let column = extract_column_from_arg(&args[0])?;
                    return Ok(Some(WindowAggregation::CountDistinct { column, alias }));
                }
                return Ok(Some(WindowAggregation::Count { alias }));
            }
            "SUM" => {
                let path = extract_path_from_agg_arg(&args[0])?;
                return Ok(Some(WindowAggregation::Sum { path, alias }));
            }
            "AVG" | "MEAN" => {
                let path = extract_path_from_agg_arg(&args[0])?;
                return Ok(Some(WindowAggregation::Avg { path, alias }));
            }
            "MIN" => {
                let path = extract_path_from_agg_arg(&args[0])?;
                return Ok(Some(WindowAggregation::Min { path, alias }));
            }
            "MAX" => {
                let path = extract_path_from_agg_arg(&args[0])?;
                return Ok(Some(WindowAggregation::Max { path, alias }));
            }
            "FIRST" | "FIRST_VALUE" => {
                let path = extract_path_from_agg_arg(&args[0])?;
                return Ok(Some(WindowAggregation::First { path, alias }));
            }
            "LAST" | "LAST_VALUE" => {
                let path = extract_path_from_agg_arg(&args[0])?;
                return Ok(Some(WindowAggregation::Last { path, alias }));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Extract column name from aggregation argument
fn extract_column_from_arg(arg: &FunctionArg) -> Result<String> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(ident))) => {
            Ok(ident.value.clone())
        }
        FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => Ok("*".to_string()),
        _ => Err(SqlError::ParseError(
            "Expected column name in aggregation".to_string(),
        )),
    }
}

/// Extract JSON path from aggregation argument (handles json_extract or direct path)
fn extract_path_from_agg_arg(arg: &FunctionArg) -> Result<String> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(inner_func))) => {
            let inner_name = inner_func.name.to_string().to_uppercase();
            if inner_name == "JSON_EXTRACT" {
                extract_path_from_json_extract(inner_func)
            } else {
                Err(SqlError::ParseError(
                    "Expected json_extract in aggregation".to_string(),
                ))
            }
        }
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => Ok(s.clone()),
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(ident))) => {
            Ok(format!("$.{}", ident.value))
        }
        _ => Err(SqlError::ParseError(
            "Invalid aggregation argument".to_string(),
        )),
    }
}

fn is_count_query(select: &Select) -> bool {
    if select.projection.len() != 1 {
        return false;
    }
    match &select.projection[0] {
        SelectItem::UnnamedExpr(Expr::Function(func)) => {
            func.name.to_string().to_uppercase() == "COUNT"
        }
        SelectItem::ExprWithAlias {
            expr: Expr::Function(func),
            ..
        } => func.name.to_string().to_uppercase() == "COUNT",
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
        TableFactor::Table { name, .. } => {
            // Use the unquoted identifier value directly to avoid
            // sqlparser's Display impl preserving quotes (e.g. "my-topic" → my-topic)
            if name.0.len() == 1 {
                Ok(name.0[0].value.clone())
            } else {
                Ok(name.to_string())
            }
        }
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
            match func_name.as_str() {
                "JSON_EXTRACT" => parse_json_extract_func(func, alias),
                "ZSCORE" => parse_zscore_func(func, alias),
                "MOVING_AVG" => parse_moving_avg_func(func, alias),
                "STDDEV" | "STD" => parse_stddev_func(func, alias),
                "AVG" | "MEAN" => parse_avg_func(func, alias),
                "ANOMALY" => parse_anomaly_func(func, alias),
                "COSINE_SIMILARITY" => parse_cosine_similarity_func(func, alias),
                "EUCLIDEAN_DISTANCE" => parse_euclidean_distance_func(func, alias),
                "DOT_PRODUCT" => parse_dot_product_func(func, alias),
                "VECTOR_NORM" | "L2_NORM" => parse_vector_norm_func(func, alias),
                _ => Err(SqlError::UnsupportedOperation(format!(
                    "Unsupported function: {}",
                    func_name
                ))),
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
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(ident))) => ident.value.clone(),
        _ => {
            return Err(SqlError::ParseError(
                "First argument to json_extract must be a column name".to_string(),
            ))
        }
    };

    let path = match &args[1] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => s.clone(),
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

/// Parse zscore(json_extract(value, '$.path')) or zscore(value, '$.path')
fn parse_zscore_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let path = extract_path_from_nested_func_or_direct(func)?;
    Ok(SelectColumn::ZScore { path, alias })
}

/// Parse moving_avg(json_extract(value, '$.path'), window_size) or moving_avg(value, '$.path', window_size)
fn parse_moving_avg_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let args = &func.args;

    if args.is_empty() {
        return Err(SqlError::ParseError(
            "moving_avg requires at least 1 argument".to_string(),
        ));
    }

    // Try to extract path from first argument (could be json_extract or direct path)
    let (path, window_arg_idx) = match &args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(inner_func))) => {
            let inner_name = inner_func.name.to_string().to_uppercase();
            if inner_name == "JSON_EXTRACT" {
                let p = extract_path_from_json_extract(inner_func)?;
                (p, 1)
            } else {
                return Err(SqlError::ParseError(
                    "First argument to moving_avg must be json_extract or a column".to_string(),
                ));
            }
        }
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => {
            // Direct path: moving_avg('$.field', 10)
            (s.clone(), 1)
        }
        _ => {
            return Err(SqlError::ParseError(
                "Invalid first argument to moving_avg".to_string(),
            ));
        }
    };

    // Get window size
    let window_size = if args.len() > window_arg_idx {
        match &args[window_arg_idx] {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::Number(n, _)))) => {
                n.parse::<usize>().map_err(|_| {
                    SqlError::ParseError("Window size must be a positive integer".to_string())
                })?
            }
            _ => 10, // Default window size
        }
    } else {
        10 // Default window size
    };

    Ok(SelectColumn::MovingAvg {
        path,
        window_size,
        alias,
    })
}

/// Parse stddev(json_extract(value, '$.path')) or stddev(value, '$.path')
fn parse_stddev_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let path = extract_path_from_nested_func_or_direct(func)?;
    Ok(SelectColumn::Stddev { path, alias })
}

/// Parse avg(json_extract(value, '$.path')) or avg(value, '$.path')
fn parse_avg_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let path = extract_path_from_nested_func_or_direct(func)?;
    Ok(SelectColumn::Avg { path, alias })
}

/// Parse anomaly(json_extract(value, '$.path'), threshold)
fn parse_anomaly_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let args = &func.args;

    if args.is_empty() {
        return Err(SqlError::ParseError(
            "anomaly requires at least 1 argument".to_string(),
        ));
    }

    // Try to extract path from first argument
    let (path, threshold_arg_idx) = match &args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(inner_func))) => {
            let inner_name = inner_func.name.to_string().to_uppercase();
            if inner_name == "JSON_EXTRACT" {
                let p = extract_path_from_json_extract(inner_func)?;
                (p, 1)
            } else {
                return Err(SqlError::ParseError(
                    "First argument to anomaly must be json_extract or a path".to_string(),
                ));
            }
        }
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => (s.clone(), 1),
        _ => {
            return Err(SqlError::ParseError(
                "Invalid first argument to anomaly".to_string(),
            ));
        }
    };

    // Get threshold (default 2.0 = ~95% confidence)
    let threshold = if args.len() > threshold_arg_idx {
        match &args[threshold_arg_idx] {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::Number(n, _)))) => n
                .parse::<f64>()
                .map_err(|_| SqlError::ParseError("Threshold must be a number".to_string()))?,
            _ => 2.0,
        }
    } else {
        2.0
    };

    Ok(SelectColumn::Anomaly {
        path,
        threshold,
        alias,
    })
}

/// Extract JSON path from a nested json_extract call or direct path argument
fn extract_path_from_nested_func_or_direct(func: &Function) -> Result<String> {
    let args = &func.args;

    if args.is_empty() {
        return Err(SqlError::ParseError(
            "Function requires at least 1 argument".to_string(),
        ));
    }

    match &args[0] {
        // Nested: zscore(json_extract(value, '$.path'))
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(inner_func))) => {
            let inner_name = inner_func.name.to_string().to_uppercase();
            if inner_name == "JSON_EXTRACT" {
                extract_path_from_json_extract(inner_func)
            } else {
                Err(SqlError::ParseError(
                    "Nested function must be json_extract".to_string(),
                ))
            }
        }
        // Direct: zscore('$.path')
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => Ok(s.clone()),
        _ => Err(SqlError::ParseError(
            "Argument must be json_extract function or a string path".to_string(),
        )),
    }
}

/// Extract path from json_extract(value, '$.path') function
fn extract_path_from_json_extract(func: &Function) -> Result<String> {
    let args = &func.args;
    if args.len() != 2 {
        return Err(SqlError::ParseError(
            "json_extract requires 2 arguments".to_string(),
        ));
    }

    match &args[1] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => Ok(s.clone()),
        _ => Err(SqlError::ParseError(
            "Second argument to json_extract must be a string path".to_string(),
        )),
    }
}

/// Parse cosine_similarity(vector_path, query_vector) or cosine_similarity(json_extract(...), query_vector)
fn parse_cosine_similarity_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let (path, query_vector) = parse_vector_similarity_args(func)?;
    Ok(SelectColumn::CosineSimilarity {
        path,
        query_vector,
        alias,
    })
}

/// Parse euclidean_distance(vector_path, query_vector)
fn parse_euclidean_distance_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let (path, query_vector) = parse_vector_similarity_args(func)?;
    Ok(SelectColumn::EuclideanDistance {
        path,
        query_vector,
        alias,
    })
}

/// Parse dot_product(vector_path, query_vector)
fn parse_dot_product_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let (path, query_vector) = parse_vector_similarity_args(func)?;
    Ok(SelectColumn::DotProduct {
        path,
        query_vector,
        alias,
    })
}

/// Parse vector_norm(vector_path)
fn parse_vector_norm_func(func: &Function, alias: Option<String>) -> Result<SelectColumn> {
    let path = extract_path_from_nested_func_or_direct(func)?;
    Ok(SelectColumn::VectorNorm { path, alias })
}

/// Parse arguments for vector similarity functions (path, query_vector)
fn parse_vector_similarity_args(func: &Function) -> Result<(String, Vec<f64>)> {
    let args = &func.args;
    if args.len() < 2 {
        return Err(SqlError::ParseError(
            "Vector similarity functions require 2 arguments: (vector_path, query_vector)"
                .to_string(),
        ));
    }

    // First argument: vector path (json_extract or direct path)
    let path = match &args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(inner_func))) => {
            let inner_name = inner_func.name.to_string().to_uppercase();
            if inner_name == "JSON_EXTRACT" {
                extract_path_from_json_extract(inner_func)?
            } else {
                return Err(SqlError::ParseError(
                    "First argument must be json_extract or a path".to_string(),
                ));
            }
        }
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => s.clone(),
        _ => {
            return Err(SqlError::ParseError(
                "Invalid first argument for vector function".to_string(),
            ));
        }
    };

    // Second argument: query vector (array literal or ARRAY[...])
    let query_vector = parse_vector_arg(&args[1])?;

    Ok((path, query_vector))
}

/// Parse a vector argument (array of numbers)
fn parse_vector_arg(arg: &FunctionArg) -> Result<Vec<f64>> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => parse_vector_expr(expr),
        _ => Err(SqlError::ParseError("Invalid vector argument".to_string())),
    }
}

/// Parse vector expression (array literal)
fn parse_vector_expr(expr: &Expr) -> Result<Vec<f64>> {
    match expr {
        // Handle ARRAY[1.0, 2.0, 3.0]
        Expr::Array(sqlparser::ast::Array { elem, .. }) => {
            let mut vec = Vec::with_capacity(elem.len());
            for e in elem {
                match e {
                    Expr::Value(SqlValue::Number(n, _)) => {
                        let f: f64 = n.parse().map_err(|_| {
                            SqlError::ParseError(format!("Invalid number in vector: {}", n))
                        })?;
                        vec.push(f);
                    }
                    Expr::UnaryOp {
                        op: sqlparser::ast::UnaryOperator::Minus,
                        expr,
                    } => {
                        if let Expr::Value(SqlValue::Number(n, _)) = expr.as_ref() {
                            let f: f64 = n.parse().map_err(|_| {
                                SqlError::ParseError(format!("Invalid number in vector: {}", n))
                            })?;
                            vec.push(-f);
                        } else {
                            return Err(SqlError::ParseError("Invalid vector element".to_string()));
                        }
                    }
                    _ => {
                        return Err(SqlError::ParseError(
                            "Vector elements must be numbers".to_string(),
                        ));
                    }
                }
            }
            Ok(vec)
        }
        // Handle string representation: '[1.0, 2.0, 3.0]'
        Expr::Value(SqlValue::SingleQuotedString(s)) => parse_vector_string(s),
        _ => Err(SqlError::ParseError(format!(
            "Unsupported vector expression: {:?}",
            expr
        ))),
    }
}

/// Parse vector from string representation
fn parse_vector_string(s: &str) -> Result<Vec<f64>> {
    let s = s.trim();
    let s = s.trim_start_matches('[').trim_end_matches(']');

    let mut vec = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let f: f64 = part.parse().map_err(|_| {
            SqlError::ParseError(format!("Invalid number in vector string: {}", part))
        })?;
        vec.push(f);
    }

    if vec.is_empty() {
        return Err(SqlError::ParseError("Empty vector".to_string()));
    }

    Ok(vec)
}

fn parse_where_clause(expr: &Expr) -> Result<Vec<Filter>> {
    let mut filters = vec![];
    collect_filters(expr, &mut filters)?;
    Ok(filters)
}

fn collect_filters(expr: &Expr, filters: &mut Vec<Filter>) -> Result<()> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
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
        },
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
        match func_name.as_str() {
            "JSON_EXTRACT" => {
                if let Some((path, _)) = parse_json_extract_args(func)? {
                    let value = expr_to_json_value(right)?;
                    return Ok(Some(Filter::JsonEquals { path, value }));
                }
            }
            "ANOMALY" => {
                // Handle: anomaly(json_extract(value, '$.path'), threshold) = true
                let args = &func.args;
                if !args.is_empty() {
                    let (path, threshold_arg_idx) = match &args[0] {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Function(inner_func))) => {
                            let inner_name = inner_func.name.to_string().to_uppercase();
                            if inner_name == "JSON_EXTRACT" {
                                let p = extract_path_from_json_extract(inner_func)?;
                                (p, 1)
                            } else {
                                return Ok(None);
                            }
                        }
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                            SqlValue::SingleQuotedString(s),
                        ))) => (s.clone(), 1),
                        _ => return Ok(None),
                    };

                    let threshold = if args.len() > threshold_arg_idx {
                        match &args[threshold_arg_idx] {
                            FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
                                SqlValue::Number(n, _),
                            ))) => n.parse::<f64>().unwrap_or(2.0),
                            _ => 2.0,
                        }
                    } else {
                        2.0
                    };

                    // Check if comparing to true
                    if let Expr::Value(SqlValue::Boolean(true)) = right {
                        return Ok(Some(Filter::AnomalyThreshold { path, threshold }));
                    }
                }
            }
            _ => {}
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
        match func_name.as_str() {
            "JSON_EXTRACT" => {
                if let Some((path, _)) = parse_json_extract_args(func)? {
                    let value = expr_to_json_value(right)?;
                    return Ok(Some(if is_greater {
                        Filter::JsonGt { path, value }
                    } else {
                        Filter::JsonLt { path, value }
                    }));
                }
            }
            "ZSCORE" => {
                // Handle: zscore(json_extract(value, '$.path')) > threshold
                let path = extract_path_from_nested_func_or_direct(func)?;
                if let Some(threshold) = extract_literal_float(right) {
                    return Ok(Some(if is_greater {
                        Filter::ZScoreGt { path, threshold }
                    } else {
                        Filter::ZScoreLt { path, threshold }
                    }));
                }
            }
            _ => {}
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
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(ident))) => ident.value.clone(),
        _ => return Ok(None),
    };

    let path = match &args[1] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(SqlValue::SingleQuotedString(
            s,
        )))) => s.clone(),
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

fn extract_literal_float(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Value(SqlValue::Number(n, _)) => n.parse().ok(),
        // Handle unary minus for negative numbers
        Expr::UnaryOp {
            op: sqlparser::ast::UnaryOperator::Minus,
            expr,
        } => extract_literal_float(expr).map(|v| -v),
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
        let query =
            parse_query("SELECT * FROM orders WHERE partition = 0 AND offset >= 100").unwrap();
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

    // Anomaly Detection Function Tests

    #[test]
    fn test_parse_zscore() {
        let query =
            parse_query("SELECT zscore(json_extract(value, '$.price')) as z FROM orders LIMIT 100")
                .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.topic, "orders");
                assert_eq!(q.columns.len(), 1);
                match &q.columns[0] {
                    SelectColumn::ZScore { path, alias } => {
                        assert_eq!(path, "$.price");
                        assert_eq!(alias.as_deref(), Some("z"));
                    }
                    _ => panic!("Expected ZScore column"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_zscore_direct_path() {
        let query = parse_query("SELECT zscore('$.amount') FROM transactions").unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::ZScore { path, .. } => {
                    assert_eq!(path, "$.amount");
                }
                _ => panic!("Expected ZScore column"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_moving_avg() {
        let query =
            parse_query("SELECT moving_avg(json_extract(value, '$.price'), 10) as ma FROM orders")
                .unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::MovingAvg {
                    path,
                    window_size,
                    alias,
                } => {
                    assert_eq!(path, "$.price");
                    assert_eq!(*window_size, 10);
                    assert_eq!(alias.as_deref(), Some("ma"));
                }
                _ => panic!("Expected MovingAvg column"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_stddev() {
        let query =
            parse_query("SELECT stddev(json_extract(value, '$.latency')) FROM metrics").unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::Stddev { path, .. } => {
                    assert_eq!(path, "$.latency");
                }
                _ => panic!("Expected Stddev column"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_avg() {
        let query = parse_query("SELECT avg(json_extract(value, '$.cpu')) FROM metrics").unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::Avg { path, .. } => {
                    assert_eq!(path, "$.cpu");
                }
                _ => panic!("Expected Avg column"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_anomaly() {
        let query = parse_query(
            "SELECT anomaly(json_extract(value, '$.price'), 2.5) as is_anomaly FROM orders",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::Anomaly {
                    path,
                    threshold,
                    alias,
                } => {
                    assert_eq!(path, "$.price");
                    assert!((*threshold - 2.5).abs() < 0.001);
                    assert_eq!(alias.as_deref(), Some("is_anomaly"));
                }
                _ => panic!("Expected Anomaly column"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_anomaly_default_threshold() {
        let query =
            parse_query("SELECT anomaly(json_extract(value, '$.price')) FROM orders").unwrap();
        match query {
            SqlQuery::Select(q) => {
                match &q.columns[0] {
                    SelectColumn::Anomaly { threshold, .. } => {
                        assert!((*threshold - 2.0).abs() < 0.001); // Default is 2.0
                    }
                    _ => panic!("Expected Anomaly column"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_zscore_filter() {
        let query =
            parse_query("SELECT * FROM orders WHERE zscore(json_extract(value, '$.price')) > 2.0")
                .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::ZScoreGt { path, threshold } => {
                        assert_eq!(path, "$.price");
                        assert!((*threshold - 2.0).abs() < 0.001);
                    }
                    _ => panic!("Expected ZScoreGt filter"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_zscore_filter_lt() {
        let query =
            parse_query("SELECT * FROM orders WHERE zscore(json_extract(value, '$.price')) < -1.5")
                .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::ZScoreLt { path, threshold } => {
                        assert_eq!(path, "$.price");
                        assert!((*threshold - (-1.5)).abs() < 0.001);
                    }
                    _ => panic!("Expected ZScoreLt filter"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_combined_anomaly_columns() {
        let query = parse_query(
            "SELECT json_extract(value, '$.price') as price, \
             zscore(json_extract(value, '$.price')) as z, \
             anomaly(json_extract(value, '$.price'), 2.0) as is_outlier \
             FROM orders LIMIT 100",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.columns.len(), 3);
                assert!(matches!(&q.columns[0], SelectColumn::JsonExtract { .. }));
                assert!(matches!(&q.columns[1], SelectColumn::ZScore { .. }));
                assert!(matches!(&q.columns[2], SelectColumn::Anomaly { .. }));
            }
            _ => panic!("Expected Select query"),
        }
    }

    // Window Function Tests

    #[test]
    fn test_parse_tumble_window() {
        let query = parse_query(
            "SELECT COUNT(*) as cnt, SUM(json_extract(value, '$.amount')) as total \
             FROM orders \
             GROUP BY TUMBLE(timestamp, '5 minutes')",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert_eq!(q.topic, "orders");
                match &q.window {
                    WindowType::Tumble { size_ms } => {
                        assert_eq!(*size_ms, 5 * 60 * 1000); // 5 minutes in ms
                    }
                    _ => panic!("Expected Tumble window"),
                }
                assert_eq!(q.aggregations.len(), 2);
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    #[test]
    fn test_parse_hop_window() {
        let query = parse_query(
            "SELECT AVG(json_extract(value, '$.latency')) as avg_latency \
             FROM metrics \
             GROUP BY HOP(timestamp, '10 minutes', '5 minutes')",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert_eq!(q.topic, "metrics");
                match &q.window {
                    WindowType::Hop { size_ms, slide_ms } => {
                        assert_eq!(*size_ms, 10 * 60 * 1000); // 10 minutes
                        assert_eq!(*slide_ms, 5 * 60 * 1000); // 5 minutes
                    }
                    _ => panic!("Expected Hop window"),
                }
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    #[test]
    fn test_parse_session_window() {
        let query = parse_query(
            "SELECT COUNT(*) as events \
             FROM user_actions \
             GROUP BY SESSION(timestamp, '30 minutes'), key",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert_eq!(q.topic, "user_actions");
                match &q.window {
                    WindowType::Session { gap_ms } => {
                        assert_eq!(*gap_ms, 30 * 60 * 1000); // 30 minutes
                    }
                    _ => panic!("Expected Session window"),
                }
                assert_eq!(q.group_by, vec!["key"]);
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    #[test]
    fn test_parse_interval_variations() {
        // Test various interval formats
        assert_eq!(parse_interval_string("5 minutes").unwrap(), 5 * 60 * 1000);
        assert_eq!(parse_interval_string("1 hour").unwrap(), 60 * 60 * 1000);
        assert_eq!(parse_interval_string("30 seconds").unwrap(), 30 * 1000);
        assert_eq!(parse_interval_string("1 day").unwrap(), 24 * 60 * 60 * 1000);
        assert_eq!(parse_interval_string("100 ms").unwrap(), 100);
    }

    #[test]
    fn test_parse_window_with_filter() {
        let query = parse_query(
            "SELECT MIN(json_extract(value, '$.price')) as min_price, \
                    MAX(json_extract(value, '$.price')) as max_price \
             FROM orders \
             WHERE partition = 0 \
             GROUP BY TUMBLE(timestamp, '1 hour')",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert_eq!(q.filters.len(), 1);
                assert!(matches!(&q.filters[0], Filter::PartitionEquals(0)));
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    // Vector Function Tests

    #[test]
    fn test_parse_cosine_similarity() {
        let query = parse_query(
            "SELECT cosine_similarity(json_extract(value, '$.embedding'), '[0.1, 0.2, 0.3]') as sim \
             FROM documents LIMIT 10"
        ).unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.columns.len(), 1);
                match &q.columns[0] {
                    SelectColumn::CosineSimilarity {
                        path,
                        query_vector,
                        alias,
                    } => {
                        assert_eq!(path, "$.embedding");
                        assert_eq!(query_vector.len(), 3);
                        assert!((query_vector[0] - 0.1).abs() < 0.001);
                        assert_eq!(alias.as_deref(), Some("sim"));
                    }
                    _ => panic!("Expected CosineSimilarity column"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_euclidean_distance() {
        let query =
            parse_query("SELECT euclidean_distance('$.vector', '[1.0, 2.0]') as dist FROM points")
                .unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::EuclideanDistance {
                    path, query_vector, ..
                } => {
                    assert_eq!(path, "$.vector");
                    assert_eq!(query_vector, &vec![1.0, 2.0]);
                }
                _ => panic!("Expected EuclideanDistance column"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_dot_product() {
        let query = parse_query(
            "SELECT dot_product(json_extract(value, '$.features'), '[0.5, 0.5, 0.5]') FROM items",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert!(matches!(&q.columns[0], SelectColumn::DotProduct { .. }));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_vector_norm() {
        let query = parse_query(
            "SELECT vector_norm(json_extract(value, '$.embedding')) as magnitude FROM docs",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::VectorNorm { path, alias } => {
                    assert_eq!(path, "$.embedding");
                    assert_eq!(alias.as_deref(), Some("magnitude"));
                }
                _ => panic!("Expected VectorNorm column"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_vector_string_formats() {
        // Test various vector string formats
        assert_eq!(
            parse_vector_string("[1.0, 2.0, 3.0]").unwrap(),
            vec![1.0, 2.0, 3.0]
        );
        assert_eq!(parse_vector_string("1.5, 2.5").unwrap(), vec![1.5, 2.5]);
        assert_eq!(parse_vector_string("[0.1]").unwrap(), vec![0.1]);
    }

    #[test]
    fn test_parse_vector_search_query() {
        let query = parse_query(
            "SELECT key, \
                    json_extract(value, '$.title') as title, \
                    cosine_similarity(json_extract(value, '$.embedding'), '[0.1, 0.2, 0.3, 0.4]') as score \
             FROM documents \
             ORDER BY score DESC \
             LIMIT 10"
        ).unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.columns.len(), 3);
                assert_eq!(q.topic, "documents");
                assert!(q.order_by.is_some());
                assert_eq!(q.limit, Some(10));
            }
            _ => panic!("Expected Select query"),
        }
    }

    // JOIN Query Tests

    #[test]
    fn test_parse_inner_join() {
        let query = parse_query(
            "SELECT o.key, u.key \
             FROM orders o \
             INNER JOIN users u ON o.user_id = u.id \
             LIMIT 100",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.left.topic, "orders");
                assert_eq!(q.left.alias, Some("o".to_string()));
                assert_eq!(q.right.topic, "users");
                assert_eq!(q.right.alias, Some("u".to_string()));
                assert_eq!(q.join_type, JoinType::Inner);
                assert_eq!(q.limit, Some(100));
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_left_join() {
        let query = parse_query(
            "SELECT o.key, u.key \
             FROM orders o \
             LEFT JOIN users u ON o.user_id = u.id",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.join_type, JoinType::Left);
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_right_join() {
        let query = parse_query(
            "SELECT o.key, u.key \
             FROM orders o \
             RIGHT JOIN users u ON o.user_id = u.id",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.join_type, JoinType::Right);
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_full_join() {
        let query = parse_query(
            "SELECT o.key, u.key \
             FROM orders o \
             FULL OUTER JOIN users u ON o.user_id = u.id",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.join_type, JoinType::Full);
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_join_with_json_extract() {
        let query = parse_query(
            "SELECT o.key, json_extract(o.value, '$.amount') as amount, u.key as user_key \
             FROM orders o \
             JOIN users u ON json_extract(o.value, '$.user_id') = json_extract(u.value, '$.id') \
             LIMIT 50",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.left.topic, "orders");
                assert_eq!(q.right.topic, "users");
                assert_eq!(q.columns.len(), 3);
                // Check join condition uses JSON paths
                assert!(q.condition.left.1.starts_with("$."));
                assert!(q.condition.right.1.starts_with("$."));
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_join_on_key() {
        let query = parse_query(
            "SELECT o.key, o.value, u.value \
             FROM orders o \
             JOIN users u ON o.key = u.key",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                // When joining on .key, it should recognize it as the key field
                assert_eq!(q.condition.left.0, "o");
                assert_eq!(q.condition.right.0, "u");
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_join_with_wildcard() {
        let query = parse_query("SELECT * FROM orders o JOIN users u ON o.user_id = u.id").unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.columns.len(), 1);
                assert!(matches!(&q.columns[0], JoinSelectColumn::AllFrom(None)));
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_join_with_qualified_wildcard() {
        let query = parse_query("SELECT o.*, u.key FROM orders o JOIN users u ON o.user_id = u.id")
            .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.columns.len(), 2);
                assert!(
                    matches!(&q.columns[0], JoinSelectColumn::AllFrom(Some(qual)) if qual == "o")
                );
            }
            _ => panic!("Expected Join query"),
        }
    }

    // Stream-Table JOIN Tests (TABLE() syntax)

    #[test]
    fn test_parse_stream_table_join() {
        let query = parse_query(
            "SELECT o.key, u.key \
             FROM orders o \
             INNER JOIN TABLE(users) u ON o.user_id = u.key \
             LIMIT 100",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.left.topic, "orders");
                assert!(!q.left.is_table); // Stream
                assert_eq!(q.right.topic, "users");
                assert!(q.right.is_table); // Table
                assert_eq!(q.right.alias, Some("u".to_string()));
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_table_stream_join() {
        // TABLE() on left side
        let query = parse_query(
            "SELECT u.key, o.key \
             FROM TABLE(users) u \
             LEFT JOIN orders o ON u.key = o.user_id",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.left.topic, "users");
                assert!(q.left.is_table); // Table
                assert_eq!(q.right.topic, "orders");
                assert!(!q.right.is_table); // Stream
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_stream_table_join_with_json() {
        let query = parse_query(
            "SELECT o.key, json_extract(o.value, '$.amount') as amount, \
                    json_extract(u.value, '$.name') as user_name \
             FROM orders o \
             JOIN TABLE(users) u ON json_extract(o.value, '$.user_id') = u.key \
             LIMIT 50",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert!(!q.left.is_table);
                assert!(q.right.is_table);
                assert_eq!(q.columns.len(), 3);
                // Check join condition uses JSON path
                assert!(q.condition.left.1.starts_with("$."));
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_join_with_where_filter() {
        // Test JOIN with WHERE clause for predicate pushdown
        let query = parse_query(
            "SELECT o.key, u.key \
             FROM orders o \
             JOIN users u ON o.user_id = u.id \
             WHERE partition = 0",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                assert_eq!(q.left.topic, "orders");
                assert_eq!(q.right.topic, "users");
                assert_eq!(q.filters.len(), 1);
                assert!(matches!(&q.filters[0], Filter::PartitionEquals(0)));
            }
            _ => panic!("Expected Join query"),
        }
    }

    // Materialized View Tests

    #[test]
    fn test_parse_show_materialized_views() {
        let query = parse_query("SHOW MATERIALIZED VIEWS").unwrap();
        assert!(matches!(query, SqlQuery::ShowMaterializedViews));
    }

    #[test]
    fn test_parse_describe_materialized_view() {
        let query = parse_query("DESCRIBE MATERIALIZED VIEW hourly_sales").unwrap();
        match query {
            SqlQuery::DescribeMaterializedView(name) => {
                assert_eq!(name, "hourly_sales");
            }
            _ => panic!("Expected DescribeMaterializedView"),
        }
    }

    #[test]
    fn test_parse_drop_materialized_view() {
        let query = parse_query("DROP MATERIALIZED VIEW hourly_sales").unwrap();
        match query {
            SqlQuery::DropMaterializedView(name) => {
                assert_eq!(name, "hourly_sales");
            }
            _ => panic!("Expected DropMaterializedView"),
        }
    }

    #[test]
    fn test_parse_refresh_materialized_view() {
        let query = parse_query("REFRESH MATERIALIZED VIEW hourly_sales").unwrap();
        match query {
            SqlQuery::RefreshMaterializedView(name) => {
                assert_eq!(name, "hourly_sales");
            }
            _ => panic!("Expected RefreshMaterializedView"),
        }
    }

    #[test]
    fn test_parse_create_materialized_view_basic() {
        let query = parse_query(
            "CREATE MATERIALIZED VIEW hourly_sales AS \
             SELECT COUNT(*) as orders, SUM(json_extract(value, '$.amount')) as total \
             FROM orders \
             GROUP BY TUMBLE(timestamp, '1 hour')",
        )
        .unwrap();
        match query {
            SqlQuery::CreateMaterializedView(q) => {
                assert_eq!(q.name, "hourly_sales");
                assert_eq!(q.source_topic, "orders");
                assert!(!q.or_replace);
                assert!(matches!(q.refresh_mode, RefreshMode::Continuous));
                assert!(q.window.is_some());
                match &q.window {
                    Some(WindowType::Tumble { size_ms }) => {
                        assert_eq!(*size_ms, 60 * 60 * 1000); // 1 hour in ms
                    }
                    _ => panic!("Expected Tumble window"),
                }
            }
            _ => panic!("Expected CreateMaterializedView"),
        }
    }

    #[test]
    fn test_parse_create_or_replace_materialized_view() {
        let query = parse_query(
            "CREATE OR REPLACE MATERIALIZED VIEW daily_totals AS \
             SELECT COUNT(*) FROM orders GROUP BY TUMBLE(timestamp, '1 day')",
        )
        .unwrap();
        match query {
            SqlQuery::CreateMaterializedView(q) => {
                assert_eq!(q.name, "daily_totals");
                assert!(q.or_replace);
            }
            _ => panic!("Expected CreateMaterializedView"),
        }
    }

    #[test]
    fn test_parse_create_materialized_view_with_refresh_mode() {
        let query = parse_query(
            "CREATE MATERIALIZED VIEW hourly_sales \
             WITH (refresh_mode = 'periodic', interval = '5 minutes') \
             AS SELECT COUNT(*) FROM orders GROUP BY TUMBLE(timestamp, '1 hour')",
        )
        .unwrap();
        match query {
            SqlQuery::CreateMaterializedView(q) => {
                assert_eq!(q.name, "hourly_sales");
                match q.refresh_mode {
                    RefreshMode::Periodic { interval_ms } => {
                        assert_eq!(interval_ms, 5 * 60 * 1000);
                    }
                    _ => panic!("Expected Periodic refresh mode"),
                }
            }
            _ => panic!("Expected CreateMaterializedView"),
        }
    }

    #[test]
    fn test_parse_create_materialized_view_manual_refresh() {
        let query = parse_query(
            "CREATE MATERIALIZED VIEW snapshot \
             WITH (refresh_mode = 'manual') \
             AS SELECT COUNT(*) FROM orders GROUP BY TUMBLE(timestamp, '1 day')",
        )
        .unwrap();
        match query {
            SqlQuery::CreateMaterializedView(q) => {
                assert!(matches!(q.refresh_mode, RefreshMode::Manual));
            }
            _ => panic!("Expected CreateMaterializedView"),
        }
    }

    #[test]
    fn test_parse_create_materialized_view_with_hop_window() {
        let query = parse_query(
            "CREATE MATERIALIZED VIEW sliding_avg AS \
             SELECT AVG(json_extract(value, '$.price')) as avg_price \
             FROM orders \
             GROUP BY HOP(timestamp, '10 minutes', '1 minute')",
        )
        .unwrap();
        match query {
            SqlQuery::CreateMaterializedView(q) => {
                assert_eq!(q.name, "sliding_avg");
                match &q.window {
                    Some(WindowType::Hop { size_ms, slide_ms }) => {
                        assert_eq!(*size_ms, 10 * 60 * 1000);
                        assert_eq!(*slide_ms, 1 * 60 * 1000);
                    }
                    _ => panic!("Expected Hop window"),
                }
            }
            _ => panic!("Expected CreateMaterializedView"),
        }
    }

    #[test]
    fn test_parse_create_materialized_view_with_filter() {
        let query = parse_query(
            "CREATE MATERIALIZED VIEW filtered_sales AS \
             SELECT COUNT(*) FROM orders \
             WHERE partition = 0 \
             GROUP BY TUMBLE(timestamp, '1 hour')",
        )
        .unwrap();
        match query {
            SqlQuery::CreateMaterializedView(q) => {
                assert_eq!(q.filters.len(), 1);
                assert!(matches!(&q.filters[0], Filter::PartitionEquals(0)));
            }
            _ => panic!("Expected CreateMaterializedView"),
        }
    }

    // ========================================================================
    // SELECT query parsing - comprehensive coverage
    // ========================================================================

    #[test]
    fn test_parse_select_star_no_limit() {
        let query = parse_query("SELECT * FROM events").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.topic, "events");
                assert_eq!(q.columns.len(), 1);
                assert!(matches!(q.columns[0], SelectColumn::All));
                assert!(q.limit.is_none());
                assert!(q.offset.is_none());
                assert!(q.order_by.is_none());
                assert!(q.filters.is_empty());
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_specific_columns() {
        let query = parse_query("SELECT key, value, offset FROM orders").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.columns.len(), 3);
                match &q.columns[0] {
                    SelectColumn::Column(name) => assert_eq!(name, "key"),
                    _ => panic!("Expected Column"),
                }
                match &q.columns[1] {
                    SelectColumn::Column(name) => assert_eq!(name, "value"),
                    _ => panic!("Expected Column"),
                }
                match &q.columns[2] {
                    SelectColumn::Column(name) => assert_eq!(name, "offset"),
                    _ => panic!("Expected Column"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_single_column() {
        let query = parse_query("SELECT key FROM mytopic").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.topic, "mytopic");
                assert_eq!(q.columns.len(), 1);
                assert!(matches!(&q.columns[0], SelectColumn::Column(name) if name == "key"));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_with_semicolon() {
        let query = parse_query("SELECT * FROM orders LIMIT 5;").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.topic, "orders");
                assert_eq!(q.limit, Some(5));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_with_offset() {
        let query = parse_query("SELECT * FROM events LIMIT 10 OFFSET 20").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.limit, Some(10));
                assert_eq!(q.offset, Some(20));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_offset_without_limit() {
        // SQL standard allows OFFSET without LIMIT in some dialects
        let query = parse_query("SELECT * FROM events OFFSET 5").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert!(q.limit.is_none());
                assert_eq!(q.offset, Some(5));
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // ORDER BY parsing
    // ========================================================================

    #[test]
    fn test_parse_order_by_asc() {
        let query = parse_query("SELECT * FROM orders ORDER BY offset ASC").unwrap();
        match query {
            SqlQuery::Select(q) => {
                let ob = q.order_by.unwrap();
                assert_eq!(ob.column, "offset");
                assert!(!ob.descending);
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_order_by_desc() {
        let query = parse_query("SELECT * FROM orders ORDER BY timestamp DESC").unwrap();
        match query {
            SqlQuery::Select(q) => {
                let ob = q.order_by.unwrap();
                assert_eq!(ob.column, "timestamp");
                assert!(ob.descending);
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_order_by_default_direction() {
        // Default direction in SQL is ASC
        let query = parse_query("SELECT * FROM orders ORDER BY offset").unwrap();
        match query {
            SqlQuery::Select(q) => {
                let ob = q.order_by.unwrap();
                assert_eq!(ob.column, "offset");
                assert!(!ob.descending);
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_order_by_with_limit() {
        let query = parse_query("SELECT * FROM orders ORDER BY offset DESC LIMIT 50").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.limit, Some(50));
                let ob = q.order_by.unwrap();
                assert_eq!(ob.column, "offset");
                assert!(ob.descending);
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // WHERE clause - key equality
    // ========================================================================

    #[test]
    fn test_parse_where_key_equals() {
        let query = parse_query("SELECT * FROM orders WHERE key = 'customer-123'").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::KeyEquals(k) => assert_eq!(k, "customer-123"),
                    _ => panic!("Expected KeyEquals filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_key_equals_empty_string() {
        let query = parse_query("SELECT * FROM orders WHERE key = ''").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::KeyEquals(k) => assert_eq!(k, ""),
                    _ => panic!("Expected KeyEquals filter"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // WHERE clause - partition filter
    // ========================================================================

    #[test]
    fn test_parse_where_partition_equals() {
        let query = parse_query("SELECT * FROM orders WHERE partition = 3").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::PartitionEquals(p) => assert_eq!(*p, 3),
                    _ => panic!("Expected PartitionEquals filter"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_partition_zero() {
        let query = parse_query("SELECT * FROM events WHERE partition = 0").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                assert!(matches!(&q.filters[0], Filter::PartitionEquals(0)));
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // WHERE clause - offset filters
    // ========================================================================

    #[test]
    fn test_parse_where_offset_gte() {
        let query = parse_query("SELECT * FROM orders WHERE offset >= 1000").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::OffsetGte(o) => assert_eq!(*o, 1000),
                    _ => panic!("Expected OffsetGte filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_offset_lt() {
        let query = parse_query("SELECT * FROM orders WHERE offset < 2000").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::OffsetLt(o) => assert_eq!(*o, 2000),
                    _ => panic!("Expected OffsetLt filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_offset_equals() {
        let query = parse_query("SELECT * FROM orders WHERE offset = 42").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::OffsetEquals(o) => assert_eq!(*o, 42),
                    _ => panic!("Expected OffsetEquals filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_offset_range() {
        let query =
            parse_query("SELECT * FROM orders WHERE offset >= 100 AND offset < 200").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 2);
                let has_gte = q
                    .filters
                    .iter()
                    .any(|f| matches!(f, Filter::OffsetGte(100)));
                let has_lt = q.filters.iter().any(|f| matches!(f, Filter::OffsetLt(200)));
                assert!(has_gte, "Expected OffsetGte(100)");
                assert!(has_lt, "Expected OffsetLt(200)");
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_offset_gt_converts_to_gte() {
        // offset > 5 should become OffsetGte(6) internally
        let query = parse_query("SELECT * FROM orders WHERE offset > 5").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::OffsetGte(o) => assert_eq!(*o, 6),
                    _ => panic!("Expected OffsetGte filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_offset_lte_converts_to_lt() {
        // offset <= 10 should become OffsetLt(11) internally
        let query = parse_query("SELECT * FROM orders WHERE offset <= 10").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::OffsetLt(o) => assert_eq!(*o, 11),
                    _ => panic!("Expected OffsetLt filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // WHERE clause - timestamp filters
    // ========================================================================

    #[test]
    fn test_parse_where_timestamp_gte() {
        let query =
            parse_query("SELECT * FROM orders WHERE timestamp >= '2026-01-15T00:00:00Z'").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::TimestampGte(ts) => {
                        // 2026-01-15T00:00:00Z in milliseconds
                        let expected = chrono::DateTime::parse_from_rfc3339("2026-01-15T00:00:00Z")
                            .unwrap()
                            .timestamp_millis();
                        assert_eq!(*ts, expected);
                    }
                    _ => panic!("Expected TimestampGte filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_timestamp_lt() {
        let query =
            parse_query("SELECT * FROM orders WHERE timestamp < '2026-02-01T12:00:00Z'").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::TimestampLt(ts) => {
                        let expected = chrono::DateTime::parse_from_rfc3339("2026-02-01T12:00:00Z")
                            .unwrap()
                            .timestamp_millis();
                        assert_eq!(*ts, expected);
                    }
                    _ => panic!("Expected TimestampLt filter"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_timestamp_range() {
        let query = parse_query(
            "SELECT * FROM orders \
             WHERE timestamp >= '2026-01-01T00:00:00Z' AND timestamp < '2026-02-01T00:00:00Z'",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 2);
                let has_gte = q
                    .filters
                    .iter()
                    .any(|f| matches!(f, Filter::TimestampGte(_)));
                let has_lt = q
                    .filters
                    .iter()
                    .any(|f| matches!(f, Filter::TimestampLt(_)));
                assert!(has_gte, "Expected TimestampGte");
                assert!(has_lt, "Expected TimestampLt");
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // WHERE clause - JSON filters
    // ========================================================================

    #[test]
    fn test_parse_where_json_equals_string() {
        let query =
            parse_query("SELECT * FROM orders WHERE json_extract(value, '$.status') = 'shipped'")
                .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::JsonEquals { path, value } => {
                        assert_eq!(path, "$.status");
                        assert_eq!(value, &serde_json::Value::String("shipped".to_string()));
                    }
                    _ => panic!("Expected JsonEquals filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_json_equals_number() {
        let query = parse_query("SELECT * FROM orders WHERE json_extract(value, '$.quantity') = 5")
            .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::JsonEquals { path, value } => {
                        assert_eq!(path, "$.quantity");
                        assert_eq!(value, &serde_json::json!(5));
                    }
                    _ => panic!("Expected JsonEquals filter"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_json_gt() {
        let query = parse_query("SELECT * FROM orders WHERE json_extract(value, '$.amount') > 100")
            .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::JsonGt { path, value } => {
                        assert_eq!(path, "$.amount");
                        assert_eq!(value, &serde_json::json!(100));
                    }
                    _ => panic!("Expected JsonGt filter, got {:?}", q.filters[0]),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_json_lt() {
        let query =
            parse_query("SELECT * FROM orders WHERE json_extract(value, '$.price') < 50").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 1);
                match &q.filters[0] {
                    Filter::JsonLt { path, value } => {
                        assert_eq!(path, "$.price");
                        assert_eq!(value, &serde_json::json!(50));
                    }
                    _ => panic!("Expected JsonLt filter"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // WHERE clause - combined filters
    // ========================================================================

    #[test]
    fn test_parse_where_multiple_and_conditions() {
        let query = parse_query(
            "SELECT * FROM orders \
             WHERE partition = 0 \
             AND offset >= 1000 \
             AND offset < 2000 \
             AND key = 'user-42'",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 4);
                let has_partition = q
                    .filters
                    .iter()
                    .any(|f| matches!(f, Filter::PartitionEquals(0)));
                let has_offset_gte = q
                    .filters
                    .iter()
                    .any(|f| matches!(f, Filter::OffsetGte(1000)));
                let has_offset_lt = q
                    .filters
                    .iter()
                    .any(|f| matches!(f, Filter::OffsetLt(2000)));
                let has_key = q
                    .filters
                    .iter()
                    .any(|f| matches!(f, Filter::KeyEquals(k) if k == "user-42"));
                assert!(has_partition);
                assert!(has_offset_gte);
                assert!(has_offset_lt);
                assert!(has_key);
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_where_parenthesized_conditions() {
        let query =
            parse_query("SELECT * FROM orders WHERE (partition = 0) AND (offset >= 100)").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.filters.len(), 2);
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // SELECT json_extract
    // ========================================================================

    #[test]
    fn test_parse_select_json_extract_with_alias() {
        let query =
            parse_query("SELECT json_extract(value, '$.customer_id') as cid FROM orders").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.columns.len(), 1);
                match &q.columns[0] {
                    SelectColumn::JsonExtract {
                        column,
                        path,
                        alias,
                    } => {
                        assert_eq!(column, "value");
                        assert_eq!(path, "$.customer_id");
                        assert_eq!(alias.as_deref(), Some("cid"));
                    }
                    _ => panic!("Expected JsonExtract column"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_json_extract_without_alias() {
        let query = parse_query("SELECT json_extract(value, '$.name') FROM users").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.columns.len(), 1);
                match &q.columns[0] {
                    SelectColumn::JsonExtract {
                        column,
                        path,
                        alias,
                    } => {
                        assert_eq!(column, "value");
                        assert_eq!(path, "$.name");
                        assert!(alias.is_none());
                    }
                    _ => panic!("Expected JsonExtract column"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_mixed_columns_and_json_extract() {
        let query = parse_query(
            "SELECT key, offset, json_extract(value, '$.amount') as amount FROM orders LIMIT 50",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.columns.len(), 3);
                assert!(matches!(&q.columns[0], SelectColumn::Column(name) if name == "key"));
                assert!(matches!(&q.columns[1], SelectColumn::Column(name) if name == "offset"));
                assert!(matches!(&q.columns[2], SelectColumn::JsonExtract { .. }));
                assert_eq!(q.limit, Some(50));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_select_nested_json_path() {
        let query = parse_query(
            "SELECT json_extract(value, '$.order.items.0.price') as first_price FROM orders",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::JsonExtract { path, .. } => {
                    assert_eq!(path, "$.order.items.0.price");
                }
                _ => panic!("Expected JsonExtract"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // SHOW / DESCRIBE commands
    // ========================================================================

    #[test]
    fn test_parse_show_topics_case_insensitive() {
        assert!(matches!(
            parse_query("SHOW TOPICS").unwrap(),
            SqlQuery::ShowTopics
        ));
        assert!(matches!(
            parse_query("show topics").unwrap(),
            SqlQuery::ShowTopics
        ));
        assert!(matches!(
            parse_query("Show Topics").unwrap(),
            SqlQuery::ShowTopics
        ));
    }

    #[test]
    fn test_parse_show_topics_with_semicolon() {
        assert!(matches!(
            parse_query("SHOW TOPICS;").unwrap(),
            SqlQuery::ShowTopics
        ));
    }

    #[test]
    fn test_parse_describe_with_semicolon() {
        let query = parse_query("DESCRIBE orders;").unwrap();
        match query {
            SqlQuery::DescribeTopic(name) => assert_eq!(name, "orders"),
            _ => panic!("Expected DescribeTopic"),
        }
    }

    #[test]
    fn test_parse_desc_shorthand() {
        let query = parse_query("DESC orders").unwrap();
        match query {
            SqlQuery::DescribeTopic(name) => assert_eq!(name, "orders"),
            _ => panic!("Expected DescribeTopic"),
        }
    }

    #[test]
    fn test_parse_describe_case_insensitive() {
        let query = parse_query("describe events").unwrap();
        match query {
            SqlQuery::DescribeTopic(name) => assert_eq!(name, "events"),
            _ => panic!("Expected DescribeTopic"),
        }
    }

    #[test]
    fn test_parse_show_materialized_views_case_insensitive() {
        assert!(matches!(
            parse_query("SHOW MATERIALIZED VIEWS").unwrap(),
            SqlQuery::ShowMaterializedViews
        ));
        assert!(matches!(
            parse_query("show materialized views").unwrap(),
            SqlQuery::ShowMaterializedViews
        ));
    }

    // ========================================================================
    // COUNT(*) queries
    // ========================================================================

    #[test]
    fn test_parse_count_no_filter() {
        let query = parse_query("SELECT COUNT(*) FROM events").unwrap();
        match query {
            SqlQuery::Count(q) => {
                assert_eq!(q.topic, "events");
                assert!(q.filters.is_empty());
            }
            _ => panic!("Expected Count query"),
        }
    }

    #[test]
    fn test_parse_count_with_multiple_filters() {
        let query = parse_query("SELECT COUNT(*) FROM orders WHERE partition = 0 AND key = 'test'")
            .unwrap();
        match query {
            SqlQuery::Count(q) => {
                assert_eq!(q.topic, "orders");
                assert_eq!(q.filters.len(), 2);
            }
            _ => panic!("Expected Count query"),
        }
    }

    // ========================================================================
    // Error cases
    // ========================================================================

    #[test]
    fn test_parse_empty_query() {
        let result = parse_query("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_whitespace_only() {
        let result = parse_query("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_sql() {
        let result = parse_query("NOT A VALID SQL QUERY");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_insert_rejected() {
        let result = parse_query("INSERT INTO orders VALUES ('key', 'value')");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_update_rejected() {
        let result = parse_query("UPDATE orders SET value = 'new' WHERE key = 'k'");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_delete_rejected() {
        let result = parse_query("DELETE FROM orders WHERE key = 'k'");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_create_table_rejected() {
        let result = parse_query("CREATE TABLE orders (id INT)");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_drop_table_rejected() {
        let result = parse_query("DROP TABLE orders");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_select_without_from() {
        let result = parse_query("SELECT 1");
        // This may either error or produce an unexpected result depending on sqlparser
        // The important thing is it does not panic
        let _ = result;
    }

    #[test]
    fn test_parse_unsupported_operator_in_where() {
        let result = parse_query("SELECT * FROM orders WHERE key LIKE '%test%'");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unsupported_function() {
        let result = parse_query("SELECT UNKNOWN_FUNC(value) FROM orders");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_extract_wrong_arg_count() {
        let result = parse_query("SELECT json_extract(value) FROM orders");
        assert!(result.is_err());
    }

    // ========================================================================
    // Interval parsing
    // ========================================================================

    #[test]
    fn test_parse_interval_string_seconds() {
        assert_eq!(parse_interval_string("1 second").unwrap(), 1000);
        assert_eq!(parse_interval_string("30 seconds").unwrap(), 30_000);
        assert_eq!(parse_interval_string("1 sec").unwrap(), 1000);
        assert_eq!(parse_interval_string("5 s").unwrap(), 5000);
    }

    #[test]
    fn test_parse_interval_string_minutes() {
        assert_eq!(parse_interval_string("1 minute").unwrap(), 60_000);
        assert_eq!(parse_interval_string("5 minutes").unwrap(), 300_000);
        assert_eq!(parse_interval_string("10 min").unwrap(), 600_000);
        assert_eq!(parse_interval_string("1 m").unwrap(), 60_000);
    }

    #[test]
    fn test_parse_interval_string_hours() {
        assert_eq!(parse_interval_string("1 hour").unwrap(), 3_600_000);
        assert_eq!(parse_interval_string("2 hours").unwrap(), 7_200_000);
        assert_eq!(parse_interval_string("1 h").unwrap(), 3_600_000);
    }

    #[test]
    fn test_parse_interval_string_days() {
        assert_eq!(parse_interval_string("1 day").unwrap(), 86_400_000);
        assert_eq!(parse_interval_string("7 days").unwrap(), 604_800_000);
        assert_eq!(parse_interval_string("1 d").unwrap(), 86_400_000);
    }

    #[test]
    fn test_parse_interval_string_milliseconds() {
        assert_eq!(parse_interval_string("100 ms").unwrap(), 100);
        assert_eq!(parse_interval_string("500 milliseconds").unwrap(), 500);
        assert_eq!(parse_interval_string("1 millisecond").unwrap(), 1);
    }

    #[test]
    fn test_parse_interval_string_number_only() {
        // When no unit is given, treated as milliseconds
        assert_eq!(parse_interval_string("1000").unwrap(), 1000);
    }

    #[test]
    fn test_parse_interval_string_unknown_unit() {
        assert!(parse_interval_string("5 weeks").is_err());
        assert!(parse_interval_string("1 year").is_err());
    }

    #[test]
    fn test_parse_interval_string_empty() {
        assert!(parse_interval_string("").is_err());
    }

    #[test]
    fn test_parse_interval_string_invalid_number() {
        assert!(parse_interval_string("abc minutes").is_err());
    }

    // ========================================================================
    // Vector string parsing
    // ========================================================================

    #[test]
    fn test_parse_vector_string_basic() {
        let v = parse_vector_string("[1.0, 2.0, 3.0]").unwrap();
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_parse_vector_string_no_brackets() {
        let v = parse_vector_string("1.5, 2.5, 3.5").unwrap();
        assert_eq!(v, vec![1.5, 2.5, 3.5]);
    }

    #[test]
    fn test_parse_vector_string_single_element() {
        let v = parse_vector_string("[0.5]").unwrap();
        assert_eq!(v, vec![0.5]);
    }

    #[test]
    fn test_parse_vector_string_integers() {
        let v = parse_vector_string("[1, 2, 3]").unwrap();
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_parse_vector_string_empty_fails() {
        assert!(parse_vector_string("[]").is_err());
        assert!(parse_vector_string("").is_err());
    }

    #[test]
    fn test_parse_vector_string_invalid_element() {
        assert!(parse_vector_string("[1.0, abc, 3.0]").is_err());
    }

    #[test]
    fn test_parse_vector_string_whitespace_handling() {
        let v = parse_vector_string("  [  1.0 ,  2.0 ,  3.0  ]  ").unwrap();
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }

    // ========================================================================
    // Window aggregate queries - additional tests
    // ========================================================================

    #[test]
    fn test_parse_window_count_distinct() {
        let query = parse_query(
            "SELECT COUNT(DISTINCT key) as unique_keys \
             FROM orders \
             GROUP BY TUMBLE(timestamp, '1 hour')",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert_eq!(q.aggregations.len(), 1);
                match &q.aggregations[0] {
                    WindowAggregation::CountDistinct { column, alias } => {
                        assert_eq!(column, "key");
                        assert_eq!(alias.as_deref(), Some("unique_keys"));
                    }
                    _ => panic!("Expected CountDistinct"),
                }
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    #[test]
    fn test_parse_window_min_max() {
        let query = parse_query(
            "SELECT MIN(json_extract(value, '$.price')) as low, \
                    MAX(json_extract(value, '$.price')) as high \
             FROM orders \
             GROUP BY TUMBLE(timestamp, '5 minutes')",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert_eq!(q.aggregations.len(), 2);
                assert!(matches!(&q.aggregations[0], WindowAggregation::Min { .. }));
                assert!(matches!(&q.aggregations[1], WindowAggregation::Max { .. }));
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    #[test]
    fn test_parse_window_with_group_by_column() {
        let query = parse_query(
            "SELECT COUNT(*) as cnt \
             FROM orders \
             GROUP BY TUMBLE(timestamp, '1 hour'), key",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert!(matches!(&q.window, WindowType::Tumble { .. }));
                assert_eq!(q.group_by, vec!["key"]);
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    #[test]
    fn test_parse_window_with_limit() {
        let query = parse_query(
            "SELECT COUNT(*) FROM orders \
             GROUP BY TUMBLE(timestamp, '1 hour') \
             LIMIT 24",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert_eq!(q.limit, Some(24));
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    #[test]
    fn test_parse_tumble_window_missing_args() {
        // TUMBLE with insufficient arguments should error
        let result = parse_query("SELECT COUNT(*) FROM orders GROUP BY TUMBLE(timestamp)");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hop_window_missing_args() {
        // HOP needs 3 args
        let result =
            parse_query("SELECT COUNT(*) FROM orders GROUP BY HOP(timestamp, '5 minutes')");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_session_window_missing_args() {
        let result = parse_query("SELECT COUNT(*) FROM orders GROUP BY SESSION(timestamp)");
        assert!(result.is_err());
    }

    // ========================================================================
    // Materialized view commands - additional tests
    // ========================================================================

    #[test]
    fn test_parse_describe_materialized_view_with_semicolon() {
        let query = parse_query("DESCRIBE MATERIALIZED VIEW my_view;").unwrap();
        match query {
            SqlQuery::DescribeMaterializedView(name) => assert_eq!(name, "my_view"),
            _ => panic!("Expected DescribeMaterializedView"),
        }
    }

    #[test]
    fn test_parse_drop_materialized_view_with_semicolon() {
        let query = parse_query("DROP MATERIALIZED VIEW old_view;").unwrap();
        match query {
            SqlQuery::DropMaterializedView(name) => assert_eq!(name, "old_view"),
            _ => panic!("Expected DropMaterializedView"),
        }
    }

    #[test]
    fn test_parse_refresh_materialized_view_with_semicolon() {
        let query = parse_query("REFRESH MATERIALIZED VIEW stale_view;").unwrap();
        match query {
            SqlQuery::RefreshMaterializedView(name) => assert_eq!(name, "stale_view"),
            _ => panic!("Expected RefreshMaterializedView"),
        }
    }

    #[test]
    fn test_parse_create_materialized_view_session_window() {
        let query = parse_query(
            "CREATE MATERIALIZED VIEW user_sessions AS \
             SELECT COUNT(*) as events \
             FROM user_actions \
             GROUP BY SESSION(timestamp, '30 minutes'), key",
        )
        .unwrap();
        match query {
            SqlQuery::CreateMaterializedView(q) => {
                assert_eq!(q.name, "user_sessions");
                assert_eq!(q.source_topic, "user_actions");
                match &q.window {
                    Some(WindowType::Session { gap_ms }) => {
                        assert_eq!(*gap_ms, 30 * 60 * 1000);
                    }
                    _ => panic!("Expected Session window"),
                }
                assert_eq!(q.group_by, vec!["key"]);
            }
            _ => panic!("Expected CreateMaterializedView"),
        }
    }

    // ========================================================================
    // JOIN queries - additional edge cases
    // ========================================================================

    #[test]
    fn test_parse_join_with_order_by() {
        let query = parse_query(
            "SELECT o.key, u.key \
             FROM orders o \
             JOIN users u ON o.user_id = u.id \
             ORDER BY o.key DESC \
             LIMIT 50",
        )
        .unwrap();
        match query {
            SqlQuery::Join(q) => {
                let ob = q.order_by.unwrap();
                assert_eq!(ob.column, "o.key");
                assert!(ob.descending);
                assert_eq!(q.limit, Some(50));
            }
            _ => panic!("Expected Join query"),
        }
    }

    #[test]
    fn test_parse_join_default_window() {
        let query =
            parse_query("SELECT o.key FROM orders o JOIN users u ON o.key = u.key").unwrap();
        match query {
            SqlQuery::Join(q) => {
                // Default window should be 1 hour (3600000 ms)
                assert_eq!(q.window_ms, Some(3600000));
            }
            _ => panic!("Expected Join query"),
        }
    }

    // ========================================================================
    // Anomaly detection / statistics functions - additional tests
    // ========================================================================

    #[test]
    fn test_parse_moving_avg_default_window() {
        let query =
            parse_query("SELECT moving_avg(json_extract(value, '$.temp')) FROM sensors").unwrap();
        match query {
            SqlQuery::Select(q) => {
                match &q.columns[0] {
                    SelectColumn::MovingAvg { window_size, .. } => {
                        assert_eq!(*window_size, 10); // Default is 10
                    }
                    _ => panic!("Expected MovingAvg"),
                }
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_moving_avg_custom_window() {
        let query = parse_query(
            "SELECT moving_avg(json_extract(value, '$.temp'), 50) as ma50 FROM sensors",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::MovingAvg {
                    window_size, alias, ..
                } => {
                    assert_eq!(*window_size, 50);
                    assert_eq!(alias.as_deref(), Some("ma50"));
                }
                _ => panic!("Expected MovingAvg"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_zscore_filter_combined_with_select() {
        let query = parse_query(
            "SELECT key, json_extract(value, '$.price') as price, \
                    zscore(json_extract(value, '$.price')) as z \
             FROM orders \
             WHERE zscore(json_extract(value, '$.price')) > 3.0 \
             LIMIT 100",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.columns.len(), 3);
                assert_eq!(q.filters.len(), 1);
                assert!(
                    matches!(&q.filters[0], Filter::ZScoreGt { threshold, .. } if (*threshold - 3.0).abs() < 0.001)
                );
                assert_eq!(q.limit, Some(100));
            }
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // Vector functions - additional tests
    // ========================================================================

    #[test]
    fn test_parse_l2_norm_alias() {
        let query =
            parse_query("SELECT l2_norm(json_extract(value, '$.vec')) as norm FROM items").unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::VectorNorm { path, alias } => {
                    assert_eq!(path, "$.vec");
                    assert_eq!(alias.as_deref(), Some("norm"));
                }
                _ => panic!("Expected VectorNorm"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_cosine_similarity_direct_path() {
        let query =
            parse_query("SELECT cosine_similarity('$.embedding', '[0.5, 0.5]') as sim FROM docs")
                .unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::CosineSimilarity {
                    path, query_vector, ..
                } => {
                    assert_eq!(path, "$.embedding");
                    assert_eq!(query_vector.len(), 2);
                }
                _ => panic!("Expected CosineSimilarity"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_euclidean_distance_direct_path() {
        let query =
            parse_query("SELECT euclidean_distance('$.coords', '[0.0, 0.0]') as dist FROM points")
                .unwrap();
        match query {
            SqlQuery::Select(q) => match &q.columns[0] {
                SelectColumn::EuclideanDistance {
                    path, query_vector, ..
                } => {
                    assert_eq!(path, "$.coords");
                    assert_eq!(query_vector, &vec![0.0, 0.0]);
                }
                _ => panic!("Expected EuclideanDistance"),
            },
            _ => panic!("Expected Select query"),
        }
    }

    // ========================================================================
    // Comprehensive query parsing: full realistic queries
    // ========================================================================

    #[test]
    fn test_parse_full_select_query() {
        let query = parse_query(
            "SELECT key, offset, json_extract(value, '$.amount') as amount \
             FROM orders \
             WHERE partition = 0 AND offset >= 500 AND key = 'customer-1' \
             ORDER BY offset DESC \
             LIMIT 25",
        )
        .unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.topic, "orders");
                assert_eq!(q.columns.len(), 3);
                assert_eq!(q.filters.len(), 3);
                assert_eq!(q.limit, Some(25));
                let ob = q.order_by.unwrap();
                assert_eq!(ob.column, "offset");
                assert!(ob.descending);
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_window_aggregate_full_query() {
        let query = parse_query(
            "SELECT COUNT(*) as cnt, \
                    SUM(json_extract(value, '$.amount')) as total, \
                    AVG(json_extract(value, '$.amount')) as avg_amount, \
                    MIN(json_extract(value, '$.amount')) as min_amt, \
                    MAX(json_extract(value, '$.amount')) as max_amt \
             FROM orders \
             WHERE partition = 0 \
             GROUP BY TUMBLE(timestamp, '1 hour') \
             LIMIT 48",
        )
        .unwrap();
        match query {
            SqlQuery::WindowAggregate(q) => {
                assert_eq!(q.topic, "orders");
                assert_eq!(q.aggregations.len(), 5);
                assert_eq!(q.filters.len(), 1);
                assert_eq!(q.limit, Some(48));
                assert!(
                    matches!(&q.window, WindowType::Tumble { size_ms } if *size_ms == 3_600_000)
                );
            }
            _ => panic!("Expected WindowAggregate query"),
        }
    }

    // ========================================================================
    // Whitespace and formatting resilience
    // ========================================================================

    #[test]
    fn test_parse_leading_trailing_whitespace() {
        let query = parse_query("   SELECT * FROM orders LIMIT 10   ").unwrap();
        assert!(matches!(query, SqlQuery::Select(_)));
    }

    #[test]
    fn test_parse_multiline_query() {
        let query = parse_query("SELECT *\nFROM orders\nWHERE partition = 0\nLIMIT 10").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.topic, "orders");
                assert_eq!(q.filters.len(), 1);
                assert_eq!(q.limit, Some(10));
            }
            _ => panic!("Expected Select query"),
        }
    }

    #[test]
    fn test_parse_tab_separated_query() {
        let query = parse_query("SELECT\t*\tFROM\torders\tLIMIT\t5").unwrap();
        match query {
            SqlQuery::Select(q) => {
                assert_eq!(q.topic, "orders");
                assert_eq!(q.limit, Some(5));
            }
            _ => panic!("Expected Select query"),
        }
    }
}
