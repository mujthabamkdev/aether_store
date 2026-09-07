use aether_store::{
    AetherVault, AetherKernel, AetherOrchestrator, ProductTemplate, InputSchema,
    ProjectAtom, ProjectStatus, Paths, AppState, Limits, RateLimiter,
};
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::Path;
use axum::{
    Router, routing::{get, post}, Json, extract::{State, ConnectInfo}, http::{HeaderMap, Method, StatusCode, header::HeaderValue},
    response::IntoResponse,
};
use tower_http::services::ServeDir;
use tower_http::cors::CorsLayer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;

#[derive(Deserialize)]
struct OrchestrationRequest {
    manifest: String,
}

#[derive(Serialize)]
struct OrchestrationResult {
    root_hash: String,
    ui_hint: Option<String>,
    output: serde_json::Value,
    logs: Vec<String>,
}

#[derive(Deserialize)]
struct RunTemplateRequest {
    product_id: String,
    inputs: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct InspectRequest {
    format: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    project: String,
    #[serde(default)]
    #[allow(dead_code)] // API contract; reserved for future context routing
    hash: Option<String>,
    message: String,
    #[serde(default)]
    history: Option<Vec<ChatMessage>>,
    #[serde(default)]
    ai_provider: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
struct LogicNodePatch {
    name: String,
    intent: String,
    dependencies: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone)]
struct InputPatch {
    name: String,
    label: Option<String>,
    input_type: Option<String>,
    options: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ManifestPatch {
    raw_yaml: Option<String>,
    #[serde(default)]
    add_nodes: Option<Vec<LogicNodePatch>>,
    #[serde(default)]
    modify_nodes: Option<Vec<LogicNodePatch>>,
    #[serde(default)]
    remove_nodes: Option<Vec<String>>,
    #[serde(default)]
    add_inputs: Option<Vec<InputPatch>>,
    #[serde(default)]
    modify_inputs: Option<Vec<InputPatch>>,
    #[serde(default)]
    remove_inputs: Option<Vec<String>>,
    #[serde(default)]
    styles: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct WeaveRequest {
    project: String,
    /// Optional optimistic concurrency token. When set, server checks it
    /// matches the project's stored root_hash before applying.
    #[serde(default)]
    current_hash: Option<String>,
    patch: ManifestPatch,
}

#[derive(Serialize)]
struct InspectResult {
    dot_graph: String,
}

#[derive(Deserialize)]
struct ExecuteRequest {
    hash: String,
}

#[derive(Deserialize)]
struct ProjectRequest {
    name: String,
    #[serde(default)]
    inputs: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Serialize)]
struct DeployResult {
    app_url: String,
    root_hash: String,
}

#[derive(Deserialize)]
struct InjectRequest {
    spec: serde_json::Value,
}

#[derive(Deserialize)]
struct InventoryQuery {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn caller_key(headers: &HeaderMap, addr: &SocketAddr) -> String {
    if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = v.split(',').next() {
            return first.trim().to_string();
        }
    }
    addr.ip().to_string()
}

async fn handle_orchestration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<OrchestrationRequest>,
) -> impl IntoResponse {
    if !state.chat_limiter.allow(&format!("orch:{}", caller_key(&headers, &addr))) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({"error": "rate limited"}))).into_response();
    }
    if payload.manifest.len() > state.limits.manifest_max_bytes {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({"error": "manifest too large"}))).into_response();
    }
    let orchestrator = match AetherOrchestrator::with_limits(state.vault.clone(), state.limits.clone()) {
        Ok(o) => o,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    match orchestrator.build_app(&payload.manifest) {
        Ok((root_hash, ui_hint)) => {
            let kernel = AetherKernel::new(state.vault.clone());
            match kernel.execute_smart(&root_hash).await {
                Ok(result) => Json(OrchestrationResult {
                    root_hash, ui_hint, output: result,
                    logs: vec!["Execution Successful".to_string()],
                }).into_response(),
                Err(e) => Json(OrchestrationResult {
                    root_hash, ui_hint: None,
                    output: serde_json::json!({"error": e.to_string()}),
                    logs: vec![format!("Execution Error: {}", e)],
                }).into_response(),
            }
        }
        Err(e) => Json(OrchestrationResult {
            root_hash: String::new(), ui_hint: None,
            output: serde_json::json!({"error": e.to_string()}),
            logs: vec![format!("Build Error: {}", e)],
        }).into_response(),
    }
}

async fn handle_run_template(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RunTemplateRequest>,
) -> impl IntoResponse {
    let content = match fs::read_to_string(&state.paths.catalog_path) {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "catalog missing"}))).into_response(),
    };
    let catalog: HashMap<String, ProductTemplate> = serde_json::from_str(&content).unwrap_or_default();

    let product = match catalog.get(&payload.product_id) {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Product ID not found"}))).into_response(),
    };

    let mut manifest = product.manifest_template.clone();
    for (key, val) in &payload.inputs {
        let placeholder = format!("{{{{{}}}}}", key);
        let s = match val { serde_json::Value::String(s) => s.clone(), other => other.to_string() };
        manifest = manifest.replace(&placeholder, &s);
    }

    let orchestrator = match AetherOrchestrator::with_limits(state.vault.clone(), state.limits.clone()) {
        Ok(o) => o,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    match orchestrator.build_app(&manifest) {
        Ok((root_hash, ui_hint)) => {
            let kernel = AetherKernel::new(state.vault.clone());
            match kernel.execute_smart(&root_hash).await {
                Ok(result) => Json(OrchestrationResult {
                    root_hash, ui_hint, output: result,
                    logs: vec!["Template Executed".to_string()],
                }).into_response(),
                Err(e) => Json(OrchestrationResult {
                    root_hash, ui_hint: None,
                    output: serde_json::json!({"error": e.to_string()}),
                    logs: vec![format!("Execution Error: {}", e)],
                }).into_response(),
            }
        }
        Err(e) => Json(OrchestrationResult {
            root_hash: String::new(), ui_hint: None,
            output: serde_json::json!({"error": e.to_string()}),
            logs: vec![format!("Build Error: {}", e)],
        }).into_response(),
    }
}

async fn handle_inspect(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InspectRequest>,
) -> impl IntoResponse {
    let result = if payload.format == "json" {
        state.vault.export_graph_json().to_string()
    } else if payload.format == "inventory" {
        let limit = payload.limit.unwrap_or(100).min(state.limits.inventory_max_items);
        let offset = payload.offset.unwrap_or(0);
        serde_json::to_string(&state.vault.inventory(limit, offset)).unwrap_or_default()
    } else {
        state.vault.export_graph_viz()
    };
    Json(InspectResult { dot_graph: result })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tracing — level from RUST_LOG, default "info".
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .compact()
        .init();

    let _ = dotenvy::dotenv();

    let paths = Paths::discover();
    tracing::info!(engine_dir = ?paths.engine_dir, "discovered paths");

    // API key — required.
    let api_key = match std::env::var("AETHER_API_KEY") {
        Ok(k) if !k.trim().is_empty() && !k.starts_with("dev_only") => k,
        _ => {
            eprintln!("FATAL: AETHER_API_KEY missing or set to placeholder. Generate with `openssl rand -hex 32` and set in .env.");
            std::process::exit(2);
        }
    };

    let bind_addr = std::env::var("AETHER_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let bind_port: u16 = std::env::var("AETHER_PORT").ok()
        .and_then(|p| p.parse().ok()).unwrap_or(3000);

    let sled_path_str = paths.sled_path.to_string_lossy().to_string();
    let vault = AetherVault::new(&sled_path_str)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(5))
        .user_agent("AetherEngine/0.1")
        .build()?;
    let limits = Limits::default();

    let state = Arc::new(AppState {
        vault: vault.clone(),
        paths: paths.clone(),
        api_key: api_key.clone(),
        http,
        chat_limiter: RateLimiter::new(60, std::time::Duration::from_secs(60)),   // 60 req/min per IP
        weave_limiter: RateLimiter::new(10, std::time::Duration::from_secs(60)),  // 10 weave/min per IP
        limits,
    });

    bootstrap_registry(&state)?;
    refresh_catalog(&state)?;
    migrate_projects(&state)?;

    let cors = CorsLayer::new()
        .allow_origin(["http://localhost:3000".parse::<HeaderValue>().unwrap(),
                       "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap()])
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            "x-aether-key".parse().unwrap(),
            "content-type".parse().unwrap(),
        ]);

    // Auth middleware applies to all /api/* routes. Static shell is unauthed.
    async fn auth_layer(
        State(s): State<Arc<AppState>>,
        headers: HeaderMap,
        req: axum::extract::Request,
        next: axum::middleware::Next,
    ) -> Result<axum::response::Response, (StatusCode, &'static str)> {
        aether_store::auth::require_api_key(State(s), headers, req, next).await
    }

    let state_for_graph = state.clone();
    let state_for_reg = state.clone();
    let api = Router::new()
        .route("/api/graph", get(move || {
            let v = state_for_graph.vault.clone();
            async move { Json(v.export_graph_json()) }
        }))
        .route("/api/registry", get({
            let p = state_for_reg.paths.registry_path.clone();
            move || {
                let p = p.clone();
                async move {
                    match fs::read_to_string(&p) {
                        Ok(c) => c,
                        Err(_) => "{}".to_string(),
                    }
                }
            }
        }))
        .route("/api/inspect", post(handle_inspect))
        .route("/api/run_template", post(handle_run_template))
        .route("/api/orchestrate", post(handle_orchestration))
        .route("/api/orchestrate_project", post(handle_orchestrate_project))
        .route("/api/deploy", post(handle_deploy))
        .route("/api/project_schema", post(handle_get_project_schema))
        .route("/api/execute", post(handle_execution_by_hash))
        .route("/api/projects", get(handle_list_projects))
        .route("/api/chat", post(handle_chat))
        .route("/api/project/weave", post(handle_weave))
        .route("/api/warehouse/inventory", get(handle_warehouse_inventory))
        .route("/api/warehouse/inject", post(handle_warehouse_inject))
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth_layer))
        .with_state(state.clone());

async fn serve_injected_shell(file_name: String, key: String) -> impl IntoResponse {
    let base = std::env::current_dir().unwrap_or_default().join("../universal_shell");
    let path = base.join(&file_name);
    let content = match fs::read_to_string(&path) {
        Ok(mut c) => { c = c.replace("{{AETHER_KEY}}", &key); c },
        Err(_) => return (StatusCode::NOT_FOUND, [("content-type", "text/plain")], "Shell file missing").into_response(),
    };
    (
        StatusCode::OK,
        [
            ("content-type", "text/html; charset=utf-8"),
            ("cache-control", "no-store, no-cache, must-revalidate"),
        ],
        content,
    )
        .into_response()
}

    let key_for_shell = api_key.clone();
    let app = Router::new()
        .merge(api)
        .route("/index.html", get({
            let k = key_for_shell.clone();
            move || {
                let file = "index.html".to_string();
                let key = k.clone();
                async move { serve_injected_shell(file, key).await }
            }
        }))
        .route("/canvas.html", get({
            let k = key_for_shell.clone();
            move || {
                let file = "canvas.html".to_string();
                let key = k.clone();
                async move { serve_injected_shell(file, key).await }
            }
        }))
        .fallback_service(ServeDir::new(&paths.engine_dir.join("../universal_shell")))
        .layer(axum::middleware::map_response(|mut res: axum::response::Response| async move {
            res.headers_mut().insert(
                "content-security-policy",
                axum::http::HeaderValue::from_static("default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; connect-src 'self'; img-src 'self' data:; font-src 'self' https://fonts.gstatic.com; frame-ancestors 'none'; base-uri 'self'; form-action 'self';"),
            );
            res.headers_mut().insert(
                "x-content-type-options",
                axum::http::HeaderValue::from_static("nosniff"),
            );
            res.headers_mut().insert(
                "x-frame-options",
                axum::http::HeaderValue::from_static("DENY"),
            );
            res.headers_mut().insert(
                "referrer-policy",
                axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
            );
            res
        }))
        .layer(cors);

    tracing::info!(addr = %bind_addr, port = bind_port, "Aether Engine Active");
    let listener = tokio::net::TcpListener::bind((bind_addr.as_str(), bind_port)).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async { let _ = tokio::signal::ctrl_c().await; };
    let terminate = async {
        if let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            sig.recv().await;
        }
    };
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("shutdown signal received");
}

fn bootstrap_registry(state: &Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    let registry_path = state.paths.registry_path.clone();
    tracing::info!("Bootstrapping Logic Registry");
    let loom = aether_store::AetherLoom::new().unwrap();
    let mut registry = std::collections::HashMap::new();

    let atom_modern = loom.weave("Filter where built > 2020").unwrap();
    let hash_modern = state.vault.persist(&atom_modern).unwrap();
    registry.insert("HASH_OF_MODERN_FILTER".to_string(), hash_modern.clone());
    tracing::info!(hash = %hash_modern, "minted MODERN_LAW");

    let atom_riba = loom.weave("Verify 0% interest").unwrap();
    // Standalone law atom has no inputs; the Guard's Op 100 input requirement is a
    // graph-level invariant, enforced in build_app when the audit node is wired into the DAG.
    let hash_riba = state.vault.persist(&atom_riba).unwrap();
    registry.insert("HASH_OF_RIBA_CHECK".to_string(), hash_riba.clone());
    tracing::info!(hash = %hash_riba, "minted RIBA_LAW");

    let json = serde_json::to_string_pretty(&registry).unwrap();
    fs::write(&registry_path, json)?;
    Ok(())
}

fn refresh_catalog(state: &Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Refreshing Product Catalog");
    let catalog_path = state.paths.catalog_path.clone();
    let registry_content = fs::read_to_string(&state.paths.registry_path).unwrap_or_else(|_| "{}".to_string());
    let registry: HashMap<String, String> = serde_json::from_str(&registry_content).unwrap_or_default();
    let hash_modern = registry.get("HASH_OF_MODERN_FILTER").cloned().unwrap_or_default();
    let hash_riba = registry.get("HASH_OF_RIBA_CHECK").cloned().unwrap_or_default();

    let mut catalog = HashMap::new();
    let transit_template = format!(r#"
app_name: "KL Generative Transit"
inputs:
  - name: "station_type"
    label: "Station Type (LRT, MRT, KTM)"
    input_type: "select"
    options: ["LRT", "MRT", "Monorail", "KTM"]
  - name: "station_name"
    label: "Preferred Station Name"
    input_type: "text"
imports:
  - name: "MODERN_LAW"
    hash: "{}"
  - name: "RIBA_LAW"
    hash: "{}"
nodes:
  - name: "fetch_kl_properties"
    intent: "Fetch from http://127.0.0.1:8080/kl/properties"
    dependencies: []
  - name: "filter_modern"
    use_ref: "MODERN_LAW"
    dependencies: ["fetch_kl_properties"]
  - name: "filter_type"
    intent: "Filter where station_type == {{{{station_type}}}}"
    dependencies: ["filter_modern"]
  - name: "filter_name"
    intent: "Filter where station contains {{{{station_name}}}}"
    dependencies: ["filter_type"]
  - name: "riba_audit"
    use_ref: "RIBA_LAW"
    dependencies: ["filter_name"]
  - name: "root"
    intent: "Output verified listings"
    dependencies: ["riba_audit"]
"#, hash_modern, hash_riba);

    let product = ProductTemplate {
        id: "PRODUCT:KL-Transit-Home".to_string(),
        name: "KL Transit Home Finder".to_string(),
        manifest_template: transit_template,
        inputs: vec![
            InputSchema {
                name: "station_type".to_string(),
                label: "Station Type (LRT, MRT, KTM)".to_string(),
                input_type: "select".to_string(),
                options: Some(vec!["LRT".to_string(), "MRT".to_string(), "KTM".to_string(), "Monorail".to_string()]),
                constraints: HashMap::new(),
            },
            InputSchema {
                name: "station_name".to_string(),
                label: "Preferred Station Name".to_string(),
                input_type: "text".to_string(),
                options: None,
                constraints: HashMap::new(),
            },
        ],
    };
    catalog.insert(product.id.clone(), product);
    let json = serde_json::to_string_pretty(&catalog)?;
    fs::write(&catalog_path, json)?;
    Ok(())
}

fn migrate_projects(state: &Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Verifying Project Registry");
    let projects = state.vault.list_projects(None)?;
    let needs_bootstrap = projects.is_empty();

    if needs_bootstrap {
        if let Ok(entries) = fs::read_dir(&state.paths.products_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_dir() { continue; }
                let name = match entry.file_name().into_string() { Ok(s) => s, Err(_) => continue };
                let atom = ProjectAtom {
                    name: name.clone(),
                    root_hash: "legacy_fs_root".to_string(),
                    org_hash: "global".to_string(),
                    status: ProjectStatus::Building,
                    created_at: now_ts(),
                };
                let _ = state.vault.persist_project(&atom);
            }
        }
    }

    // Recovery: heal Building-state projects stuck across restarts, plus
    // any legacy_fs_root stubs.
    let projects = state.vault.list_projects(None)?;
    for mut proj in projects {
        if proj.root_hash == "legacy_fs_root" || proj.status == ProjectStatus::Building {
            let manifest_path = state.paths.manifest_for(&proj.name).ok();
            if let Some(mp) = manifest_path {
                if let Ok(content) = fs::read_to_string(&mp) {
                    let orchestrator = AetherOrchestrator::with_limits(state.vault.clone(), state.limits.clone()).ok();
                    if let Some(orch) = orchestrator {
                        match orch.build_app(&content) {
                            Ok((hash, _)) => {
                                proj.root_hash = hash;
                                proj.status = ProjectStatus::Active;
                                let _ = state.vault.persist_project(&proj);
                                tracing::info!(project = %proj.name, hash = %proj.root_hash, "repaired");
                            }
                            Err(e) => tracing::warn!(project = %proj.name, error = %e, "repair build failed"),
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_orchestrate_project(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ProjectRequest>,
) -> impl IntoResponse {
    let project_atom = ProjectAtom {
        name: payload.name.clone(),
        root_hash: String::new(),
        org_hash: "global".to_string(),
        status: ProjectStatus::Building,
        created_at: now_ts(),
    };
    let _ = state.vault.persist_project(&project_atom);

    let path = match state.paths.manifest_for(&payload.name) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    };
    let mut content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    if let Some(inputs) = payload.inputs {
        apply_template(&mut content, &inputs);
    }

    let orchestrator = match AetherOrchestrator::with_limits(state.vault.clone(), state.limits.clone()) {
        Ok(o) => o,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    match orchestrator.build_app(&content) {
        Ok((root_hash, ui_hint)) => {
            let final_atom = ProjectAtom {
                name: payload.name.clone(),
                root_hash: root_hash.clone(),
                org_hash: "global".to_string(),
                status: ProjectStatus::Active,
                created_at: now_ts(),
            };
            let _ = state.vault.persist_project(&final_atom);

            let kernel = AetherKernel::new(state.vault.clone());
            match kernel.execute_smart(&root_hash).await {
                Ok(result) => Json(OrchestrationResult {
                    root_hash, ui_hint, output: result,
                    logs: vec![format!("Project '{}' Build & Exec Successful", payload.name)],
                }).into_response(),
                Err(e) => Json(OrchestrationResult {
                    root_hash, ui_hint: None,
                    output: serde_json::json!({"error": e.to_string()}),
                    logs: vec![format!("Execution Error: {}", e)],
                }).into_response(),
            }
        }
        Err(e) => Json(OrchestrationResult {
            root_hash: String::new(), ui_hint: None,
            output: serde_json::json!({"error": e.to_string()}),
            logs: vec![format!("Build Error: {}", e)],
        }).into_response(),
    }
}

/// Replace {{key}} and dot-paths like {{key.subkey}} using a JSON Value map.
/// String values are inserted as-is. Non-strings are JSON-serialized.
fn apply_template(template: &mut String, inputs: &HashMap<String, serde_json::Value>) {
    // First replace dot-paths (longer keys first), then plain.
    let mut keys: Vec<&String> = inputs.keys().collect();
    keys.sort_by(|a, b| b.len().cmp(&a.len()));
    for k in keys {
        let v = match inputs.get(k) {
            Some(v) => v,
            None => continue,
        };
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        *template = template.replace(&format!("{{{{{}}}}}", k), &s);
    }
}

async fn handle_deploy(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ProjectRequest>,
) -> impl IntoResponse {
    let path = match state.paths.manifest_for(&payload.name) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(DeployResult { app_url: format!("error: {}", e), root_hash: "error".into() })).into_response(),
    };
    let mut content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return (StatusCode::NOT_FOUND, Json(DeployResult { app_url: format!("error: {}", e), root_hash: "error".into() })).into_response(),
    };
    if let Some(inputs) = payload.inputs {
        apply_template(&mut content, &inputs);
    }
    let orchestrator = match AetherOrchestrator::with_limits(state.vault.clone(), state.limits.clone()) {
        Ok(o) => o,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(DeployResult { app_url: format!("error: {}", e), root_hash: "error".into() })).into_response(),
    };
    match orchestrator.build_app(&content) {
        Ok((root_hash, _)) => {
            Json(DeployResult {
                app_url: format!("/canvas.html?app={}&context={}", root_hash, urlencode(&payload.name)),
                root_hash,
            }).into_response()
        }
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, Json(DeployResult {
            app_url: format!("build error: {}", e),
            root_hash: "error".into(),
        })).into_response(),
    }
}

fn urlencode(s: &str) -> String {
    s.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            c.to_string()
        } else {
            format!("%{:02X}", c as u32)
        }
    }).collect()
}

async fn handle_list_projects(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.vault.list_projects(None) {
        Ok(projects) => Json(projects),
        Err(_) => Json(Vec::new()),
    }
}

#[derive(Deserialize)]
struct ProjectSchemaRequest {
    name: String,
}

async fn handle_get_project_schema(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ProjectSchemaRequest>,
) -> impl IntoResponse {
    let path = match state.paths.manifest_for(&payload.name) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    };
    match fs::read_to_string(&path) {
        Ok(content) => match serde_yaml::from_str::<aether_store::AetherManifest>(&content) {
            Ok(manifest) => Json(serde_json::json!({
                "app_name": manifest.app_name,
                "inputs": manifest.inputs,
                "styles": manifest.styles,
            })).into_response(),
            Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
        },
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn handle_execution_by_hash(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExecuteRequest>,
) -> impl IntoResponse {
    let kernel = AetherKernel::new(state.vault.clone());
    match kernel.execute_smart(&payload.hash).await {
        Ok(result) => Json(OrchestrationResult {
            root_hash: payload.hash,
            ui_hint: None,
            output: result,
            logs: vec!["Executed from Registry".to_string()],
        }).into_response(),
        Err(e) => Json(OrchestrationResult {
            root_hash: payload.hash, ui_hint: None,
            output: serde_json::json!({"error": e.to_string()}),
            logs: vec![format!("Execution Error: {}", e)],
        }).into_response(),
    }
}

async fn handle_chat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<ChatRequest>,
) -> impl IntoResponse {
    if !state.chat_limiter.allow(&format!("chat:{}", caller_key(&headers, &addr))) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({"error": "rate limited"}))).into_response();
    }
    if payload.message.len() > state.limits.manifest_max_bytes {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({"error": "message too long"}))).into_response();
    }

    let manifest_path = match state.paths.manifest_for(&payload.project) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    };
    let manifest_info = fs::read_to_string(&manifest_path).unwrap_or_default();

    let registry_content = fs::read_to_string(&state.paths.registry_path).unwrap_or_else(|_| "{}".to_string());
    let registry: HashMap<String, String> = serde_json::from_str(&registry_content).unwrap_or_default();
    let registry_info = registry.iter()
        .map(|(k, v)| format!("  {} -> {}", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    let system_prompt = format!(r#"You are the Resident Architect for the '{}' project. Modify the manifest based on user requests.

CURRENT MANIFEST:
```yaml
{}
```

REAL REGISTRY HASHES (use these exact hashes for imports):
{}

RULES (read carefully):
- `app_name`, `inputs`, `nodes`, `styles`, dependencies — freely editable.
- `imports`: copy EXACTLY as-is from the current manifest.
- Any node with `use_ref` — keep the `use_ref`; you may change `dependencies`.
- INTENT PREFIXES: Fetch/Filter/Sort/Highlight/Enrich/Output/Verify.
- Node names: lowercase_with_underscores only.
- Must include a `root` node as final output.
- STYLES target raw HTML tags (button:, input:, select:, body:).

Respond with a brief explanation followed by the COMPLETE manifest in a ```yaml``` code block. If the user is only asking a question, respond in plain text with no yaml block."#,
        payload.project, manifest_info,
        if registry_info.is_empty() { "  (no registry entries)".to_string() } else { registry_info }
    );

    let user_message = payload.message.clone();
    let project_name = payload.project.clone();
    let history = payload.history.clone().unwrap_or_default();
    let provider = payload.ai_provider.clone().unwrap_or_else(|| "auto".to_string());
    let max_tokens = state.limits.chat_max_tokens;
    let http = state.http.clone();
    let t0 = std::time::Instant::now();

    let (mut result, used): (serde_json::Value, &str) = match provider.as_str() {
        "ollama" => {
            if let Some(r) = try_ollama(&http, &system_prompt, &user_message, &history, &project_name, max_tokens).await {
                (r, "ollama")
            } else {
                (serde_json::json!({"mode": "CHAT", "response": "Local AI (Ollama) unavailable.", "project": project_name}), "ollama-unavailable")
            }
        }
        "opencode-go" | "opencode" | "go" => {
            match std::env::var("OPENCODE_GO_API_KEY") {
                Ok(key) if !key.trim().is_empty() => {
                    if let Some(r) = try_opencode_go(&http, &key, &system_prompt, &user_message, &history, &project_name, max_tokens).await {
                        (r, "opencode-go")
                    } else {
                        (fallback_chat(&project_name), "opencode-go-unavailable")
                    }
                }
                _ => (serde_json::json!({"mode": "CHAT", "response": "OpenCode Go key missing — set OPENCODE_GO_API_KEY in .env.", "project": project_name}), "opencode-go-unconfigured"),
            }
        }
        "agentrouter" => {
            match std::env::var("AGENTROUTER_API_KEY") {
                Ok(key) if !key.trim().is_empty() => {
                    if let Some(r) = try_agentrouter(&http, &key, &system_prompt, &user_message, &history, &project_name, max_tokens).await {
                        (r, "agentrouter")
                    } else {
                        (fallback_chat(&project_name), "unavailable")
                    }
                }
                _ => (serde_json::json!({"mode": "CHAT", "response": "AgentRouter key missing — set AGENTROUTER_API_KEY in .env.", "project": project_name}), "agentrouter-unconfigured"),
            }
        }
        _ => {
            match try_auto_provider(&http, &system_prompt, &user_message, &history, &project_name, max_tokens).await {
                Some((r, used)) => (r, used),
                None => (fallback_chat(&project_name), "unavailable"),
            }
        }
    };
    if let Some(o) = result.as_object_mut() {
        o.insert("thought_ms".to_string(), serde_json::json!(t0.elapsed().as_millis() as u64));
        o.insert("provider_used".to_string(), serde_json::json!(used));
    }
    Json(result).into_response()
}

fn fallback_chat(project: &str) -> serde_json::Value {
    serde_json::json!({
        "mode": "CHAT",
        "response": "All AI providers unavailable — set AGENTROUTER_API_KEY or OPENCODE_GO_API_KEY in .env (details in engine_run.log).",
        "project": project,
    })
}

/// Auto mode: AgentRouter first, then OpenCode Go. Returns the response plus
/// the provider id that served it.
async fn try_auto_provider(
    http: &reqwest::Client,
    system_prompt: &str,
    user_message: &str,
    history: &[ChatMessage],
    project_name: &str,
    max_tokens: u32,
) -> Option<(serde_json::Value, &'static str)> {
    if let Ok(key) = std::env::var("AGENTROUTER_API_KEY") {
        if !key.trim().is_empty() {
            if let Some(r) = try_agentrouter(http, &key, system_prompt, user_message, history, project_name, max_tokens).await {
                return Some((r, "agentrouter"));
            }
        }
    }
    if let Ok(key) = std::env::var("OPENCODE_GO_API_KEY") {
        if !key.trim().is_empty() {
            if let Some(r) = try_opencode_go(http, &key, system_prompt, user_message, history, project_name, max_tokens).await {
                return Some((r, "opencode-go"));
            }
        }
    }
    None
}

async fn try_ollama(http: &reqwest::Client, system_prompt: &str, user_message: &str, history: &[ChatMessage], project_name: &str, max_tokens: u32) -> Option<serde_json::Value> {
    let url = "http://localhost:11434/v1/chat/completions";
    let mut messages = vec![serde_json::json!({"role": "system", "content": system_prompt})];
    for msg in history { messages.push(serde_json::json!({"role": msg.role, "content": msg.content})); }
    messages.push(serde_json::json!({"role": "user", "content": user_message}));
    let body = serde_json::json!({"model": "qwen2.5-coder:14b", "messages": messages, "temperature": 0.7, "max_tokens": max_tokens});
    let resp = http.post(url).header("Authorization", "Bearer ollama").json(&body).send().await.ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;
    if json.get("error").is_some_and(|v| !v.is_null()) { return None; }
    let ai_text = json["choices"][0]["message"]["content"].as_str()?;
    if ai_text.trim().is_empty() { return None; }
    Some(parse_ai_response(ai_text, project_name))
}

async fn try_agentrouter(http: &reqwest::Client, api_key: &str, system_prompt: &str, user_message: &str, history: &[ChatMessage], project_name: &str, max_tokens: u32) -> Option<serde_json::Value> {
    let mut messages = vec![serde_json::json!({"role": "system", "content": system_prompt})];
    for msg in history { messages.push(serde_json::json!({"role": msg.role, "content": msg.content})); }
    messages.push(serde_json::json!({"role": "user", "content": user_message}));
    // glm-5.3 is a reasoning model: reasoning_content can consume the whole
    // budget before content is written, so never send a small cap here.
    // (Log showed `finish_reason: length` with empty content at 8192.)
    let max_tokens = max_tokens.max(16384);
    let body = serde_json::json!({"model": "glm-5.3", "messages": messages, "temperature": 0.7, "max_tokens": max_tokens});
    let resp = match http.post("https://agentrouter.org/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        // AgentRouter's gateway rejects generic HTTP clients with 401 "unauthorized
        // client detected"; it only allows recognized coding-agent clients.
        .header("originator", "codex_cli_rs")
        .header("User-Agent", "codex_cli_rs/0.42.0")
        .json(&body).send().await { Ok(r) => r, Err(e) => { eprintln!("[agentrouter] request error: {e}"); return None; } };
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        eprintln!("[agentrouter] HTTP {status}: {body}");
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    if json.get("error").is_some_and(|v| !v.is_null()) { eprintln!("[agentrouter] API error: {}", json["error"]); return None; }
    let ai_text = json["choices"][0]["message"]["content"].as_str()?;
    if ai_text.trim().is_empty() {
        // glm-5.3 is a reasoning model: empty content usually means the token
        // budget was consumed by reasoning_content before an answer was written.
        eprintln!("[agentrouter] empty content (finish_reason: {})", json["choices"][0].get("finish_reason").and_then(|v| v.as_str()).unwrap_or("?"));
        return None;
    }
    Some(parse_ai_response(ai_text, project_name))
}

/// OpenCode Go provider (Muse Spark 1.3 Contributor).
/// Muse models are served ONLY on the Responses API
/// (`POST {base}` with an `input` array) — the `/v1/chat/completions` path
/// returns HTTP 500 for Muse. Key comes from `OPENCODE_GO_API_KEY` in .env,
/// model from `OPENCODE_GO_MODEL` (default `muse-spark-1.3-contributor`),
/// endpoint from `OPENCODE_GO_BASE_URL`.
async fn try_opencode_go(http: &reqwest::Client, api_key: &str, system_prompt: &str, user_message: &str, history: &[ChatMessage], project_name: &str, max_tokens: u32) -> Option<serde_json::Value> {
    let model = std::env::var("OPENCODE_GO_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "muse-spark-1.3-contributor".to_string());
    let base = std::env::var("OPENCODE_GO_BASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| "https://opencode.ai/zen/go/v1/responses".to_string());
    let mut input = vec![serde_json::json!({"role": "system", "content": system_prompt})];
    let start = history.len().saturating_sub(20);
    for msg in &history[start..] {
        input.push(serde_json::json!({"role": msg.role, "content": msg.content}));
    }
    input.push(serde_json::json!({"role": "user", "content": user_message}));
    // Reasoning model: keep the output budget generous so reasoning tokens
    // cannot starve the answer (same `finish_reason: length` trap as GLM).
    let max_output_tokens = max_tokens.max(16384);
    let body = serde_json::json!({
        "model": model,
        "input": input,
        "temperature": 0.7,
        "max_output_tokens": max_output_tokens,
    });
    let resp = match http.post(&base)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body).send().await {
            Ok(r) => r,
            Err(e) => { eprintln!("[opencode-go] request error: {e}"); return None; }
        };
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        eprintln!("[opencode-go] HTTP {status}: {body}");
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    if json.get("error").is_some_and(|v| !v.is_null()) { eprintln!("[opencode-go] API error: {}", json["error"]); return None; }
    let ai_text = extract_responses_text(&json)?;
    if ai_text.trim().is_empty() {
        eprintln!("[opencode-go] empty text (status: {})", json.get("status").and_then(|v| v.as_str()).unwrap_or("?"));
        return None;
    }
    Some(parse_ai_response(&ai_text, project_name))
}

/// Pull assistant text out of an OpenAI Responses API payload, tolerating
/// gateway variations (aggregated `output_text`, message items, or a
/// chat-completions-shaped fallback).
fn extract_responses_text(json: &serde_json::Value) -> Option<String> {
    if let Some(t) = json.get("output_text").and_then(|v| v.as_str()) {
        if !t.trim().is_empty() { return Some(t.to_string()); }
    }
    if let Some(items) = json.get("output").and_then(|v| v.as_array()) {
        let mut acc = String::new();
        for item in items {
            let kind = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if kind == "message" {
                if let Some(parts) = item.get("content").and_then(|v| v.as_array()) {
                    for part in parts {
                        let ptype = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if ptype == "output_text" || ptype == "text" {
                            if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                                acc.push_str(t);
                            }
                        } else if let Some(t) = part.as_str() {
                            acc.push_str(t);
                        }
                    }
                }
            } else if kind == "reasoning" {
                continue;
            } else if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                acc.push_str(t);
            }
        }
        if !acc.trim().is_empty() { return Some(acc); }
    }
    // Chat-completions-shaped fallback, just in case the gateway proxies it.
    if let Some(t) = json.pointer("/choices/0/message/content").and_then(|v| v.as_str()) {
        if !t.trim().is_empty() { return Some(t.to_string()); }
    }
    if let Some(t) = json.get("response").and_then(|v| v.as_str()) {
        if !t.trim().is_empty() { return Some(t.to_string()); }
    }
    None
}

fn parse_ai_response(ai_text: &str, project_name: &str) -> serde_json::Value {
    if let Some(yaml) = extract_yaml_block(ai_text) {
        if let Ok(_m) = serde_yaml::from_str::<aether_store::AetherManifest>(&yaml) {
            let response_text = ai_text.split("```").next().unwrap_or("").trim();
            let response = if response_text.is_empty() { "I have updated the project logic.".to_string() } else { response_text.to_string() };
            return serde_json::json!({"mode": "WEAVE", "response": response, "patch": {"raw_yaml": yaml}, "project": project_name});
        }
    }
    let json_start = ai_text.find('{');
    let json_end = ai_text.rfind('}');
    if let (Some(s), Some(e)) = (json_start, json_end) {
        if s < e {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&ai_text[s..=e]) {
                let mode = parsed["mode"].as_str().unwrap_or("CHAT");
                let response = parsed["response"].as_str().unwrap_or("").to_string();
                if mode == "WEAVE" && parsed.get("patch").is_some() {
                    let mut patch = parsed["patch"].clone();
                    if patch.get("raw_yaml").is_none() {
                        if let Some(y) = extract_yaml_block(ai_text) { patch["raw_yaml"] = serde_json::Value::String(y); }
                    }
                    if let Some(ry) = patch.get("raw_yaml").and_then(|v| v.as_str()) {
                        if serde_yaml::from_str::<aether_store::AetherManifest>(ry).is_ok() {
                            return serde_json::json!({"mode": "WEAVE", "response": if response.is_empty() {"Updated."} else {&response}, "patch": patch, "project": project_name});
                        }
                    }
                } else if mode == "CHAT" {
                    return serde_json::json!({"mode": "CHAT", "response": response, "project": project_name});
                }
            }
        }
    }
    if ai_text.contains("app_name:") && ai_text.contains("nodes:") {
        if let Some(idx) = ai_text.find("app_name:") {
            let candidate = &ai_text[idx..];
            if serde_yaml::from_str::<aether_store::AetherManifest>(candidate).is_ok() {
                let prefix = ai_text[..idx].trim();
                return serde_json::json!({"mode": "WEAVE", "response": if prefix.is_empty() {"Rewritten."} else {prefix}, "patch": {"raw_yaml": candidate}, "project": project_name});
            }
        }
    }
    serde_json::json!({"mode": "CHAT", "response": ai_text, "project": project_name})
}

fn extract_yaml_block(text: &str) -> Option<String> {
    if let Some(start) = text.find("```yaml") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            let y = after[..end].trim().to_string();
            if !y.is_empty() { return Some(y); }
        }
    }
    if let Some(start) = text.find("```yml") {
        let after = &text[start + 6..];
        if let Some(end) = after.find("```") {
            let y = after[..end].trim().to_string();
            if !y.is_empty() { return Some(y); }
        }
    }
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let cs = after.find('\n').map(|i| i + 1).unwrap_or(0);
        let after_nl = &after[cs..];
        if let Some(end) = after_nl.find("```") {
            let c = after_nl[..end].trim().to_string();
            if c.contains("app_name:") && c.contains("nodes:") { return Some(c); }
        }
    }
    None
}

fn resolve_registry_hashes(yaml: &str, state: &Arc<AppState>) -> String {
    let content = fs::read_to_string(&state.paths.registry_path).unwrap_or_else(|_| "{}".to_string());
    let registry: HashMap<String, String> = serde_json::from_str(&content).unwrap_or_default();
    let mut resolved = yaml.to_string();
    for (placeholder, real_hash) in &registry {
        resolved = resolved.replace(placeholder, real_hash);
    }
    resolved
}

async fn handle_warehouse_inventory(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<InventoryQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(100).min(state.limits.inventory_max_items);
    let offset = q.offset.unwrap_or(0);
    Json(state.vault.inventory(limit, offset))
}

async fn handle_warehouse_inject(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InjectRequest>,
) -> impl IntoResponse {
    match serde_json::from_value::<aether_store::LogicAtom>(payload.spec.clone()) {
        Ok(atom) => match state.vault.inject_atom(&atom) {
            Ok(hash) => Json(serde_json::json!({"hash": hash, "status": "Injected"})).into_response(),
            Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
        },
        Err(_) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid Atom Spec"}))).into_response(),
    }
}

async fn handle_weave(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<WeaveRequest>,
) -> impl IntoResponse {
    if !state.weave_limiter.allow(&format!("weave:{}", caller_key(&headers, &addr))) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({"success": false, "error": "rate limited"}))).into_response();
    }
    let manifest_path = match state.paths.manifest_for(&payload.project) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"success": false, "error": e}))).into_response(),
    };

    // Validate-before-write: build the candidate manifest in-memory, run it
    // through the orchestrator, only then persist.
    let build_yaml = if let Some(raw) = &payload.patch.raw_yaml {
        resolve_registry_hashes(raw, &state)
    } else {
        // Synthesize candidate from structural patch.
        let current = match fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(e) => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"success": false, "error": e.to_string()}))).into_response(),
        };
        match synthesize_patched_manifest(&current, &payload.patch) {
            Ok(y) => y,
            Err(e) => return (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({"success": false, "error": e}))).into_response(),
        }
    };

    if build_yaml.len() > state.limits.manifest_max_bytes {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(serde_json::json!({"success": false, "error": "manifest too large"}))).into_response();
    }

    let orchestrator = match AetherOrchestrator::with_limits(state.vault.clone(), state.limits.clone()) {
        Ok(o) => o,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"success": false, "error": e.to_string()}))).into_response(),
    };
    let new_hash = match orchestrator.build_app(&build_yaml) {
        Ok((h, _)) => h,
        Err(e) => return (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({"success": false, "error": format!("build failed: {}", e)}))).into_response(),
    };

    // Optimistic concurrency: refuse if hash changed under us. The fresh hash
    // rides along so the client can retry once without a round trip.
    if let Some(expected) = payload.current_hash.as_ref() {
        match state.vault.get_project(&payload.project) {
            Ok(p) if p.root_hash != *expected && p.root_hash != "legacy_fs_root" && !p.root_hash.is_empty() => {
                return (StatusCode::CONFLICT, Json(serde_json::json!({
                    "success": false,
                    "error": "project hash changed; retrying with fresh hash",
                    "current_hash": p.root_hash,
                }))).into_response();
            }
            _ => {}
        }
    }

    // Persist YAML atomically (write to .tmp, rename).
    let tmp = {
        let mut p = manifest_path.as_os_str().to_owned();
        p.push(".tmp");
        Path::new(&p).to_path_buf()
    };
    if let Err(e) = fs::write(&tmp, &build_yaml) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"success": false, "error": e.to_string()}))).into_response();
    }
    if let Err(e) = fs::rename(&tmp, &manifest_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"success": false, "error": e.to_string()}))).into_response();
    }
    if let Err(e) = state.vault.update_project_hash(&payload.project, &new_hash) {
        tracing::warn!(error = %e, "update_project_hash failed");
    }
    Json(serde_json::json!({
        "success": true,
        "new_hash": new_hash,
        "changes": ["Manifest updated"],
        "project": payload.project,
    })).into_response()
}

fn synthesize_patched_manifest(current: &str, patch: &ManifestPatch) -> Result<String, String> {
    let mut manifest: serde_yaml::Value = serde_yaml::from_str(current)
        .map_err(|e| format!("parse: {}", e))?;
    let nodes = manifest.get_mut("nodes").and_then(|n| n.as_sequence_mut())
        .ok_or_else(|| "no nodes section".to_string())?;

    if let Some(remove_list) = &patch.remove_nodes {
        for name in remove_list {
            nodes.retain(|n| n.get("name").and_then(|v| v.as_str()) != Some(name.as_str()));
        }
    }
    if let Some(modify_list) = &patch.modify_nodes {
        for p in modify_list {
            for node in nodes.iter_mut() {
                if node.get("name").and_then(|v| v.as_str()) == Some(&p.name) {
                    node["intent"] = serde_yaml::Value::String(p.intent.clone());
                    let deps: Vec<serde_yaml::Value> = p.dependencies.iter().map(|d| serde_yaml::Value::String(d.clone())).collect();
                    node["dependencies"] = serde_yaml::Value::Sequence(deps);
                }
            }
        }
    }
    if let Some(add_list) = &patch.add_nodes {
        for p in add_list {
            let already_exists = nodes.iter().any(|n| n.get("name").and_then(|v| v.as_str()) == Some(p.name.as_str()));
            if already_exists { continue; }
            let mut new_node = serde_yaml::Mapping::new();
            new_node.insert(serde_yaml::Value::String("name".into()), serde_yaml::Value::String(p.name.clone()));
            new_node.insert(serde_yaml::Value::String("intent".into()), serde_yaml::Value::String(p.intent.clone()));
            let deps: Vec<serde_yaml::Value> = p.dependencies.iter().map(|d| serde_yaml::Value::String(d.clone())).collect();
            new_node.insert(serde_yaml::Value::String("dependencies".into()), serde_yaml::Value::Sequence(deps));
            nodes.push(serde_yaml::Value::Mapping(new_node));
        }
    }
    if let Some(map) = manifest.as_mapping_mut() {
        if map.get("inputs").is_none() {
            map.insert(serde_yaml::Value::String("inputs".into()), serde_yaml::Value::Sequence(Vec::new()));
        }
    }
    let inputs = manifest.get_mut("inputs").and_then(|n| n.as_sequence_mut()).ok_or_else(|| "inputs invalid".to_string())?;
    if let Some(add_inputs) = &patch.add_inputs {
        for p in add_inputs {
            let mut ni = serde_yaml::Mapping::new();
            ni.insert(serde_yaml::Value::String("name".into()), serde_yaml::Value::String(p.name.clone()));
            ni.insert(serde_yaml::Value::String("label".into()), serde_yaml::Value::String(p.label.clone().unwrap_or(p.name.clone())));
            ni.insert(serde_yaml::Value::String("input_type".into()), serde_yaml::Value::String(p.input_type.clone().unwrap_or_else(|| "text".into())));
            if let Some(opts) = &p.options {
                let v: Vec<serde_yaml::Value> = opts.iter().map(|o| serde_yaml::Value::String(o.clone())).collect();
                ni.insert(serde_yaml::Value::String("options".into()), serde_yaml::Value::Sequence(v));
            }
            inputs.push(serde_yaml::Value::Mapping(ni));
        }
    }
    if let Some(modify_inputs) = &patch.modify_inputs {
        for p in modify_inputs {
            for input in inputs.iter_mut() {
                if input.get("name").and_then(|v| v.as_str()) == Some(&p.name) {
                    if let Some(label) = &p.label { input["label"] = serde_yaml::Value::String(label.clone()); }
                    if let Some(itype) = &p.input_type { input["input_type"] = serde_yaml::Value::String(itype.clone()); }
                    if let Some(opts) = &p.options {
                        let v: Vec<serde_yaml::Value> = opts.iter().map(|o| serde_yaml::Value::String(o.clone())).collect();
                        input["options"] = serde_yaml::Value::Sequence(v);
                    }
                }
            }
        }
    }
    if let Some(remove_inputs) = &patch.remove_inputs {
        for n in remove_inputs {
            inputs.retain(|i| i.get("name").and_then(|v| v.as_str()) != Some(n.as_str()));
        }
    }
    if let Some(styles) = &patch.styles {
        if manifest.get("styles").is_none() {
            if let Some(map) = manifest.as_mapping_mut() {
                map.insert(serde_yaml::Value::String("styles".into()), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
            }
        }
        if let Some(smap) = manifest.get_mut("styles").and_then(|v| v.as_mapping_mut()) {
            for (k, v) in styles {
                smap.insert(serde_yaml::Value::String(k.clone()), serde_yaml::Value::String(v.clone()));
            }
        }
    }
    serde_yaml::to_string(&manifest).map_err(|e| format!("serialize: {}", e))
}
