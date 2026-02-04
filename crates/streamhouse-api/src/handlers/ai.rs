//! AI-powered query endpoints
//!
//! Natural language to SQL query generation using Claude API.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use utoipa::ToSchema;

use crate::AppState;

/// In-memory query history store (per-session, not persisted)
/// In production, this would be stored in the metadata database
pub type QueryHistoryStore = Arc<RwLock<HashMap<String, QueryHistoryEntry>>>;

/// Create a new query history store
pub fn new_query_history_store() -> QueryHistoryStore {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Query history entry
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryEntry {
    /// Unique query ID
    pub id: String,
    /// Original question
    pub question: String,
    /// Generated SQL
    pub sql: String,
    /// Explanation
    pub explanation: String,
    /// Topics used
    pub topics_used: Vec<String>,
    /// Confidence score
    pub confidence: f32,
    /// Timestamp (epoch ms)
    pub created_at: u64,
    /// Previous query ID (if this was a refinement)
    pub parent_id: Option<String>,
    /// Refinement history (chain of refinements)
    pub refinement_count: u32,
}

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
    /// Whether to save this query to history (default: true)
    #[serde(default = "default_save_history")]
    pub save_history: bool,
}

fn default_save_history() -> bool {
    true
}

/// Query refinement request
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefineQueryRequest {
    /// The refinement instruction (e.g., "add a filter for status = 'completed'")
    #[schema(example = "Add a filter for orders over $100")]
    pub refinement: String,
    /// Whether to execute the refined SQL (default: true)
    #[serde(default = "default_execute")]
    pub execute: bool,
    /// Query timeout in milliseconds (default: 30000)
    pub timeout_ms: Option<u64>,
}

/// Cost estimation request
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EstimateCostRequest {
    /// Natural language question or SQL query
    pub query: String,
    /// Whether the query is SQL (false = natural language)
    #[serde(default)]
    pub is_sql: bool,
    /// Topics to consider (optional)
    pub topics: Option<Vec<String>>,
}

/// Cost estimation response
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimate {
    /// Generated SQL (if input was natural language)
    pub sql: Option<String>,
    /// Estimated rows to scan
    pub estimated_rows: u64,
    /// Estimated data size in bytes
    pub estimated_bytes: u64,
    /// Topics that will be queried
    pub topics: Vec<String>,
    /// Estimated execution time in milliseconds
    pub estimated_time_ms: u64,
    /// Cost tier (low, medium, high)
    pub cost_tier: String,
    /// Warnings (e.g., "full table scan", "no LIMIT clause")
    pub warnings: Vec<String>,
    /// Optimization suggestions
    pub suggestions: Vec<String>,
}

/// Query history list response
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryResponse {
    /// List of query history entries
    pub queries: Vec<QueryHistoryEntry>,
    /// Total count
    pub total: usize,
}

fn default_execute() -> bool {
    true
}

/// Natural language query response
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AskQueryResponse {
    /// Unique query ID (for history and refinement)
    pub query_id: String,
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
    /// Cost estimate for this query
    pub cost_estimate: Option<CostEstimate>,
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

/// App state extension for query history
pub struct AiState {
    pub history: QueryHistoryStore,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            history: new_query_history_store(),
        }
    }
}

// Global query history (in production, use database)
lazy_static::lazy_static! {
    static ref QUERY_HISTORY: QueryHistoryStore = new_query_history_store();
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
    let generation_result = generate_sql(&api_key, &req.question, &schema_context, None).await?;

    // Calculate cost estimate
    let cost_estimate = estimate_query_cost(&state, &generation_result.sql, &generation_result.topics_used).await.ok();

    // Optionally execute the query
    let results = if req.execute {
        Some(execute_generated_sql(&state, &generation_result.sql, req.timeout_ms).await?)
    } else {
        None
    };

    // Generate query ID
    let query_id = uuid::Uuid::new_v4().to_string();

    // Save to history if requested
    if req.save_history {
        let entry = QueryHistoryEntry {
            id: query_id.clone(),
            question: req.question.clone(),
            sql: generation_result.sql.clone(),
            explanation: generation_result.explanation.clone(),
            topics_used: generation_result.topics_used.clone(),
            confidence: generation_result.confidence,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            parent_id: None,
            refinement_count: 0,
        };

        let mut history = QUERY_HISTORY.write().await;
        // Keep history size bounded (max 1000 entries)
        if history.len() >= 1000 {
            // Remove oldest entry
            if let Some(oldest_key) = history
                .iter()
                .min_by_key(|(_, e)| e.created_at)
                .map(|(k, _)| k.clone())
            {
                history.remove(&oldest_key);
            }
        }
        history.insert(query_id.clone(), entry);
    }

    Ok(Json(AskQueryResponse {
        query_id,
        question: req.question,
        sql: generation_result.sql,
        explanation: generation_result.explanation,
        results,
        topics_used: generation_result.topics_used,
        confidence: generation_result.confidence,
        suggestions: generation_result.suggestions,
        cost_estimate,
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
    original_query: Option<&QueryHistoryEntry>,
) -> Result<SqlGenerationResponse, (StatusCode, Json<AskQueryError>)> {
    let client = reqwest::Client::new();

    let refinement_context = if let Some(original) = original_query {
        format!(
            r#"

REFINEMENT CONTEXT:
You are refining a previous query. Here is the original:
- Original question: {}
- Original SQL: {}
- Original explanation: {}

The user wants to modify this query with the following instruction:"#,
            original.question, original.sql, original.explanation
        )
    } else {
        String::new()
    };

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
{}
Respond with a JSON object containing:
{{
  "sql": "the SQL query",
  "explanation": "what the query does in plain English",
  "confidence": 0.0-1.0,
  "topics_used": ["list", "of", "topics"],
  "suggestions": ["optional suggestions for query refinement"]
}}"#,
        schema_context, refinement_context
    );

    let user_message = if original_query.is_some() {
        format!("Refine the query with: {}", question)
    } else {
        format!("Generate a SQL query for: {}", question)
    };

    let request = ClaudeRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 1024,
        system: system_prompt,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: user_message,
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

/// Get query history
#[utoipa::path(
    get,
    path = "/api/v1/query/history",
    params(
        ("limit" = Option<usize>, Query, description = "Max number of queries to return"),
        ("offset" = Option<usize>, Query, description = "Number of queries to skip"),
    ),
    responses(
        (status = 200, description = "Query history", body = QueryHistoryResponse),
    ),
    tag = "ai"
)]
pub async fn get_query_history(
    axum::extract::Query(params): axum::extract::Query<HistoryQueryParams>,
) -> Json<QueryHistoryResponse> {
    let history = QUERY_HISTORY.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let mut queries: Vec<QueryHistoryEntry> = history.values().cloned().collect();
    queries.sort_by(|a, b| b.created_at.cmp(&a.created_at)); // Newest first

    let total = queries.len();
    let queries: Vec<QueryHistoryEntry> = queries.into_iter().skip(offset).take(limit).collect();

    Json(QueryHistoryResponse { queries, total })
}

#[derive(Debug, Deserialize)]
pub struct HistoryQueryParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Get a specific query from history
#[utoipa::path(
    get,
    path = "/api/v1/query/history/{id}",
    params(
        ("id" = String, Path, description = "Query ID"),
    ),
    responses(
        (status = 200, description = "Query history entry", body = QueryHistoryEntry),
        (status = 404, description = "Query not found"),
    ),
    tag = "ai"
)]
pub async fn get_query_by_id(
    Path(id): Path<String>,
) -> Result<Json<QueryHistoryEntry>, StatusCode> {
    let history = QUERY_HISTORY.read().await;
    history
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Refine a previous query
#[utoipa::path(
    post,
    path = "/api/v1/query/history/{id}/refine",
    params(
        ("id" = String, Path, description = "Query ID to refine"),
    ),
    request_body = RefineQueryRequest,
    responses(
        (status = 200, description = "Query refined successfully", body = AskQueryResponse),
        (status = 404, description = "Original query not found", body = AskQueryError),
        (status = 400, description = "Invalid refinement", body = AskQueryError),
        (status = 503, description = "AI service unavailable", body = AskQueryError),
    ),
    tag = "ai"
)]
pub async fn refine_query(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RefineQueryRequest>,
) -> Result<Json<AskQueryResponse>, (StatusCode, Json<AskQueryError>)> {
    // Get the original query
    let original = {
        let history = QUERY_HISTORY.read().await;
        history.get(&id).cloned()
    };

    let original = original.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(AskQueryError {
                error: "query_not_found".to_string(),
                message: format!("Query with ID '{}' not found in history", id),
                sql: None,
                suggestions: vec!["Check the query ID".to_string()],
            }),
        )
    })?;

    // Get API key
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(AskQueryError {
                error: "ai_not_configured".to_string(),
                message: "ANTHROPIC_API_KEY environment variable not set".to_string(),
                sql: None,
                suggestions: vec![
                    "Set ANTHROPIC_API_KEY environment variable".to_string(),
                ],
            }),
        )
    })?;

    // Build context with original query
    let schema_context = build_schema_context(&state, &original.topics_used).await?;

    // Generate refined SQL
    let generation_result = generate_sql(
        &api_key,
        &req.refinement,
        &schema_context,
        Some(&original),
    )
    .await?;

    // Calculate cost estimate
    let cost_estimate = estimate_query_cost(&state, &generation_result.sql, &generation_result.topics_used).await.ok();

    // Optionally execute
    let results = if req.execute {
        Some(execute_generated_sql(&state, &generation_result.sql, req.timeout_ms).await?)
    } else {
        None
    };

    // Generate new query ID and save to history
    let query_id = uuid::Uuid::new_v4().to_string();
    let entry = QueryHistoryEntry {
        id: query_id.clone(),
        question: format!("{} (refined: {})", original.question, req.refinement),
        sql: generation_result.sql.clone(),
        explanation: generation_result.explanation.clone(),
        topics_used: generation_result.topics_used.clone(),
        confidence: generation_result.confidence,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        parent_id: Some(id),
        refinement_count: original.refinement_count + 1,
    };

    {
        let mut history = QUERY_HISTORY.write().await;
        history.insert(query_id.clone(), entry);
    }

    Ok(Json(AskQueryResponse {
        query_id,
        question: original.question,
        sql: generation_result.sql,
        explanation: generation_result.explanation,
        results,
        topics_used: generation_result.topics_used,
        confidence: generation_result.confidence,
        suggestions: generation_result.suggestions,
        cost_estimate,
    }))
}

/// Estimate query cost before execution
#[utoipa::path(
    post,
    path = "/api/v1/query/estimate",
    request_body = EstimateCostRequest,
    responses(
        (status = 200, description = "Cost estimate", body = CostEstimate),
        (status = 400, description = "Invalid query", body = AskQueryError),
        (status = 503, description = "AI service unavailable", body = AskQueryError),
    ),
    tag = "ai"
)]
pub async fn estimate_cost(
    State(state): State<AppState>,
    Json(req): Json<EstimateCostRequest>,
) -> Result<Json<CostEstimate>, (StatusCode, Json<AskQueryError>)> {
    let (sql, topics) = if req.is_sql {
        // Parse SQL to extract topics
        let topics = extract_topics_from_sql(&req.query);
        (req.query.clone(), topics)
    } else {
        // Generate SQL first
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(AskQueryError {
                    error: "ai_not_configured".to_string(),
                    message: "ANTHROPIC_API_KEY environment variable not set".to_string(),
                    sql: None,
                    suggestions: vec![],
                }),
            )
        })?;

        let topics = req.topics.unwrap_or_else(|| {
            // Default to all topics - would need async here
            vec![]
        });

        let schema_context = build_schema_context(&state, &topics).await?;
        let result = generate_sql(&api_key, &req.query, &schema_context, None).await?;
        (result.sql, result.topics_used)
    };

    let estimate = estimate_query_cost(&state, &sql, &topics).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(AskQueryError {
                error: "estimation_error".to_string(),
                message: format!("Failed to estimate cost: {}", e),
                sql: Some(sql.clone()),
                suggestions: vec![],
            }),
        )
    })?;

    Ok(Json(estimate))
}

/// Extract topic names from SQL query
fn extract_topics_from_sql(sql: &str) -> Vec<String> {
    let sql_upper = sql.to_uppercase();
    let mut topics = Vec::new();

    // Simple pattern: FROM <topic> or JOIN <topic>
    for keyword in ["FROM ", "JOIN "] {
        if let Some(pos) = sql_upper.find(keyword) {
            let after = &sql[pos + keyword.len()..];
            if let Some(topic) = after.split_whitespace().next() {
                let topic = topic.trim_matches(|c| c == ',' || c == ';' || c == ')');
                if !topic.is_empty() && topic.chars().next().unwrap().is_alphabetic() {
                    topics.push(topic.to_lowercase());
                }
            }
        }
    }

    topics.sort();
    topics.dedup();
    topics
}

/// Estimate query cost
async fn estimate_query_cost(
    state: &AppState,
    sql: &str,
    topics: &[String],
) -> Result<CostEstimate, String> {
    let mut estimated_rows: u64 = 0;
    let mut estimated_bytes: u64 = 0;
    let mut warnings = Vec::new();
    let mut suggestions = Vec::new();

    // Get topic stats
    for topic_name in topics {
        if let Ok(Some(topic)) = state.metadata.get_topic(topic_name).await {
            // Get message count from all partitions
            for partition_id in 0..topic.partition_count {
                if let Ok(segments) = state
                    .metadata
                    .get_segments(topic_name, partition_id)
                    .await
                {
                    let partition_rows: u64 = segments.iter().map(|s| s.record_count as u64).sum();
                    let partition_bytes: u64 = segments.iter().map(|s| s.size_bytes).sum();

                    estimated_rows += partition_rows;
                    estimated_bytes += partition_bytes;
                }
            }

            if estimated_rows > 100_000 {
                warnings.push(format!(
                    "Topic '{}' has {} rows - consider adding filters",
                    topic_name, estimated_rows
                ));
            }
        }
    }

    // Check for LIMIT clause
    let sql_upper = sql.to_uppercase();
    if !sql_upper.contains("LIMIT") {
        warnings.push("No LIMIT clause - query may return too many rows".to_string());
        suggestions.push("Add LIMIT clause to restrict results".to_string());
    } else {
        // Try to extract limit value
        if let Some(pos) = sql_upper.find("LIMIT") {
            let after = &sql[pos + 5..];
            if let Some(limit_str) = after.split_whitespace().next() {
                if let Ok(limit) = limit_str.trim_matches(|c| c == ';').parse::<u64>() {
                    estimated_rows = estimated_rows.min(limit);
                }
            }
        }
    }

    // Check for WHERE clause
    if !sql_upper.contains("WHERE") {
        warnings.push("No WHERE clause - full table scan".to_string());
        suggestions.push("Add filters to reduce scanned data".to_string());
    }

    // Check for partition filter
    if !sql_upper.contains("PARTITION") {
        suggestions.push("Consider filtering by partition for better performance".to_string());
    }

    // Estimate time based on rows (rough: 10K rows/sec)
    let estimated_time_ms = (estimated_rows / 10_000).max(10);

    // Determine cost tier
    let cost_tier = if estimated_rows < 10_000 {
        "low"
    } else if estimated_rows < 100_000 {
        "medium"
    } else {
        "high"
    }
    .to_string();

    Ok(CostEstimate {
        sql: None,
        estimated_rows,
        estimated_bytes,
        topics: topics.to_vec(),
        estimated_time_ms,
        cost_tier,
        warnings,
        suggestions,
    })
}

/// Delete a query from history
#[utoipa::path(
    delete,
    path = "/api/v1/query/history/{id}",
    params(
        ("id" = String, Path, description = "Query ID to delete"),
    ),
    responses(
        (status = 204, description = "Query deleted"),
        (status = 404, description = "Query not found"),
    ),
    tag = "ai"
)]
pub async fn delete_query(Path(id): Path<String>) -> StatusCode {
    let mut history = QUERY_HISTORY.write().await;
    if history.remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

/// Clear all query history
#[utoipa::path(
    delete,
    path = "/api/v1/query/history",
    responses(
        (status = 204, description = "History cleared"),
    ),
    tag = "ai"
)]
pub async fn clear_query_history() -> StatusCode {
    let mut history = QUERY_HISTORY.write().await;
    history.clear();
    StatusCode::NO_CONTENT
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
