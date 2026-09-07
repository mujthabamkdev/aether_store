use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Constant-time header comparison. Both equal-length slices; XOR-compare to
/// defeat timing oracles. Returns false on length mismatch.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Auth middleware. Reads `X-Aether-Key` header and compares to the configured
/// key. Missing/incorrect key → 401. The /api/chat endpoint is intentionally
/// not exempt — chat burns LLM quota and can mutate manifests via Weave.
pub async fn require_api_key(
    State(state): State<Arc<crate::AppState>>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    let provided = headers
        .get("x-aether-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided.is_empty() || !ct_eq(provided.as_bytes(), state.api_key.as_bytes()) {
        return Err((StatusCode::UNAUTHORIZED, "unauthorized"));
    }
    Ok(next.run(req).await)
}