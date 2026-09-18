use crate::mcp::{handle_mcp_request, JsonRpcRequest};
use axum::{
    extract::{Path as AxPath, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
    routing::{delete, get, post},
    Router,
};
use futures::stream::Stream;
use memmesh_core::{HandoffStatus, InterAgentHandoff, MemoryRecord, MemoryTier, Provenance};
use memmesh_storage::StorageEngine;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<StorageEngine>,
    pub auth_token: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchPayload {
    pub query: String,
    pub tier: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct StoreMemoryPayload {
    pub content: String,
    pub title: Option<String>,
    pub tier: Option<String>,
    pub tags: Option<Vec<String>>,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
pub struct HandoffPayload {
    pub from_agent: String,
    pub target_agent: String,
    pub project_scope: String,
    pub task_summary: String,
    pub next_steps: Option<Vec<String>>,
    pub associated_files: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct HandoffQuery {
    pub agent: Option<String>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/v1/memories", post(store_memory_handler))
        .route("/v1/memories/search", post(search_memory_handler))
        .route("/v1/memories/:id", get(get_memory_handler))
        .route("/v1/memories/:id", delete(delete_memory_handler))
        .route("/v1/handoffs", post(create_handoff_handler))
        .route("/v1/handoffs/pending", get(get_pending_handoffs_handler))
        .route("/v1/handoffs/:agent", get(get_pending_by_agent_handler))
        .route("/v1/handoffs/:id/consume", post(consume_handoff_handler))
        .route("/sse", get(sse_handler).post(mcp_message_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let (records, vectors) = state.storage.get_stats().unwrap_or((0, 0));
    Json(serde_json::json!({
        "status": "ok",
        "version": "0.1.0",
        "records": records,
        "vectors": vectors
    }))
}

async fn store_memory_handler(
    State(state): State<AppState>,
    Json(payload): Json<StoreMemoryPayload>,
) -> impl IntoResponse {
    let tier = match payload.tier.as_deref() {
        Some("working") => MemoryTier::Working,
        Some("procedural") => MemoryTier::Procedural,
        Some("episodic") => MemoryTier::Episodic,
        _ => MemoryTier::Semantic,
    };

    let record = MemoryRecord {
        id: Uuid::new_v4(),
        tier,
        scope: payload.scope.unwrap_or_else(|| "global".to_string()),
        title: payload.title.unwrap_or_else(|| "Memory Record".to_string()),
        content: payload.content,
        tags: payload.tags.unwrap_or_default(),
        provenance: Provenance {
            author_agent: "api".to_string(),
            machine_id: "daemon".to_string(),
            session_id: None,
            timestamp: chrono::Utc::now(),
        },
        importance: 0.5,
        ttl_seconds: None,
        vector: None,
    };

    match state.storage.store_memory(&record) {
        Ok(_) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "id": record.id, "status": "created" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn search_memory_handler(
    State(state): State<AppState>,
    Json(payload): Json<SearchPayload>,
) -> impl IntoResponse {
    let limit = payload.limit.unwrap_or(5);
    match state.storage.hybrid_search(
        &payload.query,
        None,
        payload.tier.as_deref(),
        limit,
    ) {
        Ok(results) => (StatusCode::OK, Json(serde_json::json!({ "results": results }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn get_memory_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<Uuid>,
) -> impl IntoResponse {
    match state.storage.get_memory(&id) {
        Ok(Some(record)) => (StatusCode::OK, Json(serde_json::to_value(record).unwrap())),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Not Found" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn delete_memory_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<Uuid>,
) -> impl IntoResponse {
    match state.storage.delete_memory(&id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "deleted": true }))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Not Found" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn create_handoff_handler(
    State(state): State<AppState>,
    Json(payload): Json<HandoffPayload>,
) -> impl IntoResponse {
    let handoff = InterAgentHandoff {
        handoff_id: Uuid::new_v4(),
        from_agent: payload.from_agent,
        target_agent: payload.target_agent,
        project_scope: payload.project_scope,
        status: HandoffStatus::Pending,
        task_summary: payload.task_summary,
        next_steps: payload.next_steps.unwrap_or_default(),
        associated_files: payload.associated_files.unwrap_or_default(),
        created_at: chrono::Utc::now(),
        lease_expires_at: None,
        retry_count: 0,
        consumed: false,
    };

    match state.storage.store_handoff(&handooff) {
        Ok(_) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "handoff_id": handoff.handoff_id })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn get_pending_handoffs_handler(
    State(state): State<AppState>,
    Query(query): Query<HandoffQuery>,
) -> impl IntoResponse {
    let agent = query.agent.unwrap_or_else(|| "all".to_string());
    match state.storage.get_pending_handoffs(&agent) {
        Ok(handoffs) => (StatusCode::OK, Json(serde_json::json!({ "handoffs": handoffs }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn get_pending_by_agent_handler(
    State(state): State<AppState>,
    AxPath(agent): AxPath<String>,
) -> impl IntoResponse {
    match state.storage.get_pending_handoffs(&agent) {
        Ok(handoffs) => (StatusCode::OK, Json(serde_json::json!({ "handoffs": handoffs }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn consume_handoff_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<Uuid>,
) -> impl IntoResponse {
    match state.storage.consume_handoff(&id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "consumed": true }))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Not Found" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn mcp_message_handler(
    State(state): State<AppState>,
    Json(req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let res = handle_mcp_request(req, state.storage);
    Json(res)
}

async fn sse_handler() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(Duration::from_secs(15)))
        .map(|_| Ok(Event::default().event("ping").data("keep-alive")));
    Sse::new(stream).keep_alive(KeepAlive::default())
}