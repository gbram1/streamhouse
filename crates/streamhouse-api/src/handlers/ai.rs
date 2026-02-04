//! AI-powered query endpoints
//!
//! Natural language to SQL query generation using Claude API.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::AppState;

/// Natural language query request
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AskQueryRequest {
    /// Natural language question about your data
    #[schema(example = "What were the top 5 products by revenue today?")]
    pub question: String,
    /// Topics to query (optional - if not specified, all topics are considered)
    #[schema(example = json!(["orders", "products"]))]
    pub topics: Option<Vec<String>>,
    /// Whether to execute the generated SQL (default: true)
    #[serde(default = "default_execute")]
    pub execute: bool,
    /// Query timeout in milliseconds (default: 30000)
    pub timeout_ms: Option<u64>,
}

fn default_execute() -> bool {
    true
}

/// Natural language query response
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AskQueryResponse {
    /// The natural language question that was asked
    pub question: String,
    /// Generated SQL query
    pub sql: String,
    /// Explanation of what the query does
    pub explanation: String,
    /// Query results (if execute=true)
    pub results: Option<QueryResults>,
    /// Topics that were used in the query
    pub topics_used: Vec<String>,
    /// Confidence score (0-1) for the generated query
    pub confidence: f32,
    /// Suggestions for refining the query
    pub suggestions: Vec<String>,
}

/// Query execution results
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryResults {
    /// Column names
    pub columns: Vec<String>,
    /// Result rows
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Number of rows returned
    pub row_count: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Whether results were truncated
    pub truncated: bool,
}

/// Error response for AI queries
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AskQueryError {
    pub error: String,
    pub message: String,
    /// The generated SQL (if available, even on error)
    pub sql: Option<String>,
    /// Suggestions for fixing the query
    pub suggestions: Vec<String>,
}

/// Claude API request
#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
    system: String,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

/// Claude API response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ClaudeContent {
    text: String,
    #[serde(rename = "type")]
    content_type: String,
}

/// Parsed SQL generation response
#[derive(Debug, Deserialize)]
struct SqlGenerationResponse {
    sql: String,
    explanation: String,
    confidence: f32,
    topics_used: Vec<String>,
    suggestions: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/query/ask",
    request_body = AskQueryRequest,
    responses(
        (status = 200, description = "Query generated and executed successfully", body = AskQueryResponse),
        (status = 400, description = "Invalid request or cannot generate query", body = AskQueryError),
        (status = 503, description = "AI service unavailable", body = AskQueryError),
        (status = 500, description = "Internal server error", body = AskQueryError)
    ),
    tag = "ai"
)]
pub async fn ask_query(
    State(state): State<AppState>,
    Json(req): Json<AskQueryRequest>,
) -> Result<Json<AskQueryResponse>, (StatusCode, Json<AskQueryError>)> {
    // Get API key from environment
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(AskQueryError {
                error: "ai_not_configured".to_string(),
                message: "ANTHROPIC_API_KEY environment variable not set".to_string(),
                sql: None,
                suggestions: vec![
                    "Set ANTHROPIC_API_KEY environment variable".to_string(),
                    "Get an API key from https://console.anthropic.com".to_string(),
                ],
            }),
        )
    })?;

    // Fetch topic schemas for context
    let topics = match &req.topics {
        Some(t) => t.clone(),
        None => {
            // Get all topics
            state
                .metadata
                .list_topics()
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(AskQueryError {
                            error: "metadata_error".to_string(),
                            message: format!("Failed to list topics: {}", e),
                            sql: None,
                            suggestions: vec![],
                        }),
                    )
                })?
                .into_iter()
                .map(|t| t.name)
                .collect()
        }
    };

    // Build schema context for the prompt
    let schema_context = build_schema_context(&state, &topics).await?;

    // Generate SQL using Claude
    let generation_result = generate_sql(&api_key, &req.question, &schema_context).await?;

    // Optionally execute the query
    let results = if req.execute {
        Some(execute_generated_sql(&state, &generation_result.sql, req.timeout_ms).await?)
    } else {
        None
    };

    Ok(Json(AskQueryResponse {
        question: req.question,
        sql: generation_result.sql,
        explanation: generation_result.explanation,
        results,
        topics_used: generation_result.topics_used,
        confidence: generation_result.confidence,
        suggestions: generation_result.suggestions,
    }))
}

/// Build schema context from topics
async fn build_schema_context(
    state: &AppState,
    topics: &[String],
) -> Result<String, (StatusCode, Json<AskQueryError>)> {
    let mut context = String::new();
    context.push_str("Available tables (topics) and their schemas:\n\n");

    for topic_name in topics {
        if let Ok(Some(topic)) = state.metadata.get_topic(topic_name).await {
            context.push_str(&format!("TABLE: {}\n", topic.name));
            context.push_str(&format!("  Partitions: {}\n", topic.partition_count));

            // Standard columns available in all StreamHouse tables
            context.push_str("  Columns:\n");
            context.push_str("    - key (STRING): Message key\n");
            context.push_str("    - value (JSON): Message value (use json_extract for fields)\n");
            context.push_str("    - partition (INTEGER): Partition ID\n");
            context.push_str("    - offset (BIGINT): Message offset within partition\n");
            context.push_str("    - timestamp (TIMESTAMP): Message timestamp\n");
            context.push_str("    - headers (JSON): Message headers\n");
            context.push_str("\n");
        }
    }

    context.push_str("\nStreamHouse SQL Syntax Notes:\n");
    context.push_str("- Use json_extract(value, '$.field') to access JSON fields\n");
    context.push_str("- Use json_extract(value, '$.nested.field') for nested fields\n");
    context.push_str("- Timestamps can be filtered with: timestamp >= '2026-01-15T00:00:00Z'\n");
    context.push_str("- Offset ranges: offset >= 1000 AND offset < 2000\n");
    context.push_str("- Partition filtering: partition = 0\n");
    context.push_str("- Always include LIMIT clause (max 10000)\n");
    context.push_str("- Supported: SELECT, WHERE, GROUP BY, ORDER BY, LIMIT, COUNT, SUM, AVG, MIN, MAX\n");
    context.push_str("- SHOW TOPICS - lists all topics\n");
    context.push_str("- DESCRIBE topic_name - shows topic details\n");

    Ok(context)
}

/// Generate SQL using Claude API
async fn generate_sql(
    api_key: &str,
    question: &str,
    schema_context: &str,
) -> Result<SqlGenerationResponse, (StatusCode, Json<AskQueryError>)> {
    let client = reqwest::Client::new();

    let system_prompt = format!(
        r#"You are a SQL query generator for StreamHouse, an event streaming database.
Your job is to convert natural language questions into valid StreamHouse SQL queries.

{}

IMPORTANT RULES:
1. Only generate SELECT queries (no INSERT, UPDATE, DELETE)
2. Always include a LIMIT clause (max 10000)
3. Use json_extract() for JSON field access
4. Return ONLY valid JSON in your response
5. If the question is ambiguous, make reasonable assumptions and note them

Respond with a JSON object containing:
{{
  "sql": "the SQL query",
  "explanation": "what the query does in plain English",
  "confidence": 0.0-1.0,
  "topics_used": ["list", "of", "topics"],
  "suggestions": ["optional suggestions for query refinement"]
}}"#,
        schema_context
    );

    let request = ClaudeRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 1024,
        system: system_prompt,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: format!("Generate a SQL query for: {}", question),
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(AskQueryError {
                    error: "ai_request_failed".to_string(),
                    message: format!("Failed to connect to AI service: {}", e),
                    sql: None,
                    suggestions: vec!["Check your internet connection".to_string()],
                }),
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(AskQueryError {
                error: "ai_error".to_string(),
                message: format!("AI service returned error {}: {}", status, error_text),
                sql: None,
                suggestions: vec!["Check your API key".to_string()],
            }),
        ));
    }

    let claude_response: ClaudeResponse = response.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AskQueryError {
                error: "ai_parse_error".to_string(),
                message: format!("Failed to parse AI response: {}", e),
                sql: None,
                suggestions: vec![],
            }),
        )
    })?;

    // Extract the text content
    let text = claude_response
        .content
        .first()
        .map(|c| c.text.clone())
        .unwrap_or_default();

    // Parse the JSON response
    // Try to extract JSON from the response (it might be wrapped in markdown)
    let json_text = extract_json(&text);

    let result: SqlGenerationResponse = serde_json::from_str(&json_text).map_err(|e| {
        // If JSON parsing fails, try to extract SQL directly
        tracing::warn!("Failed to parse structured response, attempting fallback: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(AskQueryError {
                error: "parse_error".to_string(),
                message: format!("Failed to parse AI response as JSON: {}. Raw response: {}", e, text),
                sql: extract_sql_fallback(&text),
                suggestions: vec!["Try rephrasing your question".to_string()],
            }),
        )
    })?;

    Ok(result)
}

/// Extract JSON from potentially markdown-wrapped response
fn extract_json(text: &str) -> String {
    // Try to find JSON block
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

/// Fallback SQL extraction from raw text
fn extract_sql_fallback(text: &str) -> Option<String> {
    // Look for SQL in code blocks
    if let Some(start) = text.find("```sql") {
        if let Some(end) = text[start + 6..].find("```") {
            return Some(text[start + 6..start + 6 + end].trim().to_string());
        }
    }
    // Look for SELECT statement
    if let Some(start) = text.to_uppercase().find("SELECT") {
        let remaining = &text[start..];
        if let Some(end) = remaining.find(';') {
            return Some(remaining[..end + 1].to_string());
        }
        // Take until end of line or text
        if let Some(end) = remaining.find('\n') {
            return Some(remaining[..end].to_string());
        }
        return Some(remaining.to_string());
    }
    None
}

/// Execute the generated SQL query
async fn execute_generated_sql(
    state: &AppState,
    sql: &str,
    timeout_ms: Option<u64>,
) -> Result<QueryResults, (StatusCode, Json<AskQueryError>)> {
    let executor = streamhouse_sql::SqlExecutor::new(
        state.metadata.clone(),
        state.segment_cache.clone(),
        state.object_store.clone(),
    );

    match executor.execute(sql, timeout_ms).await {
        Ok(result) => Ok(QueryResults {
            columns: result.columns.into_iter().map(|c| c.name).collect(),
            rows: result.rows,
            row_count: result.row_count,
            execution_time_ms: result.execution_time_ms,
            truncated: result.truncated,
        }),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(AskQueryError {
                error: "sql_execution_error".to_string(),
                message: format!("Generated SQL failed to execute: {}", e),
                sql: Some(sql.to_string()),
                suggestions: vec![
                    "The generated SQL may have syntax errors".to_string(),
                    "Try rephrasing your question".to_string(),
                    "Check that the topics exist".to_string(),
                ],
            }),
        )),
    }
}

/// Health check for AI service
#[utoipa::path(
    get,
    path = "/api/v1/ai/health",
    responses(
        (status = 200, description = "AI service is configured"),
        (status = 503, description = "AI service is not configured")
    ),
    tag = "ai"
)]
pub async fn ai_health() -> Result<Json<serde_json::Value>, StatusCode> {
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        Ok(Json(serde_json::json!({
            "status": "configured",
            "provider": "anthropic",
            "model": "claude-sonnet-4-20250514"
        })))
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json() {
        let text = r#"Here is the query:
```json
{"sql": "SELECT * FROM orders", "explanation": "Gets all orders", "confidence": 0.9, "topics_used": ["orders"], "suggestions": []}
```"#;
        let json = extract_json(text);
        assert!(json.starts_with('{'));
        assert!(json.ends_with('}'));
    }

    #[test]
    fn test_extract_sql_fallback() {
        let text = "Here is the SQL:\n```sql\nSELECT * FROM orders LIMIT 100;\n```";
        let sql = extract_sql_fallback(text);
        assert_eq!(sql, Some("SELECT * FROM orders LIMIT 100;".to_string()));
    }

    #[test]
    fn test_extract_sql_fallback_no_block() {
        let text = "The query is SELECT * FROM orders WHERE amount > 100 LIMIT 10;";
        let sql = extract_sql_fallback(text);
        assert!(sql.is_some());
        assert!(sql.unwrap().contains("SELECT"));
    }
}
