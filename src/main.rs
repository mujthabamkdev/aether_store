use aether_store::{AetherVault, AetherKernel, AetherOrchestrator, ProductTemplate, InputSchema};
use std::fs;
use std::sync::Arc;
use axum::{Router, routing::{get, post}, Json, extract::{State, Query}, http::Method};
use tower_http::{services::ServeDir, cors::{CorsLayer, Any}};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
struct OrchestrationRequest {
    manifest: String,
    // inputs: HashMap<String, Value> // Future extension
}

#[derive(Serialize)]
struct OrchestrationResult {
    root_hash: String,
    output: serde_json::Value,
    logs: Vec<String>,
}

#[derive(Deserialize)]
struct RunTemplateRequest {
    product_id: String,
    inputs: HashMap<String, String>,
}

#[derive(Deserialize)]
struct InspectQuery {
    id: String,
}

async fn handle_orchestration(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<OrchestrationRequest>,
) -> Json<OrchestrationResult> {
    
    // 1. Build the App from the manifest
    // Since Orchestrator is stateless but needs vault, we create one.
    // (In real app, we might cache Orchestrators or keep one in State too)
    let orchestrator = AetherOrchestrator::new((*vault).clone()).unwrap(); // Clone underlying vault (wrapper)
    
    match orchestrator.build_app(&payload.manifest) {
        Ok(root_hash) => {
            // 2. Execute
            let kernel = AetherKernel::new((*vault).clone());
            match kernel.execute_smart(&root_hash).await {
                Ok(result) => Json(OrchestrationResult {
                    root_hash,
                    output: result,
                    logs: vec!["Execution Successful".to_string()]
                }),
                Err(e) => Json(OrchestrationResult {
                    root_hash,
                    output: serde_json::json!({"error": e.to_string()}),
                    logs: vec![format!("Execution Error: {}", e)]
                })
            }
        },
        Err(e) => Json(OrchestrationResult {
            root_hash: String::new(),
            output: serde_json::json!({"error": e.to_string()}),
            logs: vec![format!("Build Error: {}", e)]
        })
    }
}

async fn handle_inspect(
    Query(query): Query<InspectQuery>,
) -> Json<serde_json::Value> {
    let catalog_path = "../catalog.json";
    if let Ok(content) = fs::read_to_string(catalog_path) {
        let catalog: HashMap<String, ProductTemplate> = serde_json::from_str(&content).unwrap_or_default();
        if let Some(product) = catalog.get(&query.id) {
             return Json(serde_json::json!(product));
        }
    }
    Json(serde_json::json!({"error": "Product not found"}))
}

async fn handle_run_template(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<RunTemplateRequest>,
) -> Json<OrchestrationResult> {
    let catalog_path = "../catalog.json";
    let content = fs::read_to_string(catalog_path).unwrap_or_default();
    let catalog: HashMap<String, ProductTemplate> = serde_json::from_str(&content).unwrap_or_default();

    if let Some(product) = catalog.get(&payload.product_id) {
        // Hydrate Template
        let mut manifest = product.manifest_template.clone();
        for (key, val) in payload.inputs {
            let placeholder = format!("{{{{{}}}}}", key); // {{key}}
            manifest = manifest.replace(&placeholder, &val);
        }

        // Build & Run
        let orchestrator = AetherOrchestrator::new((*vault).clone()).unwrap();
         match orchestrator.build_app(&manifest) {
            Ok(root_hash) => {
                let kernel = AetherKernel::new((*vault).clone());
                match kernel.execute_smart(&root_hash).await {
                    Ok(result) => Json(OrchestrationResult {
                        root_hash,
                        output: result,
                        logs: vec!["Template Executed".to_string()]
                    }),
                    Err(e) => Json(OrchestrationResult {
                        root_hash,
                        output: serde_json::json!({"error": e.to_string()}),
                        logs: vec![format!("Execution Error: {}", e)]
                    })
                }
            },
            Err(e) => Json(OrchestrationResult {
                root_hash: String::new(),
                output: serde_json::json!({"error": e.to_string()}),
                logs: vec![format!("Build Error: {}", e)]
            })
        }
    } else {
        Json(OrchestrationResult {
            root_hash: String::new(),
            output: serde_json::json!({"error": "Product ID not found"}),
            logs: vec!["Catalog Error".to_string()]
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vault = Arc::new(AetherVault::new("aether_db")?);
    // Orchestrator owns Loom and Guard internally now
    // Since vault is Arc, we can try to clone or make Orchestrator accept Arc
    // Current Orchestrator::new takes AetherVault (owned).
    // Let's create a separate vault instance for Orchestrator/CLI or update Orchestrator to use Arc.
    // For simplicity given constraints, we'll Clone the inner vault for CLI operations
    // Wait, AetherVault is strict.
    
    // Easier path: Use the standard flow for CLI, then spin up the server with a shared reference or re-open.
    // However, sled::Db is thread safe.
    
    // --- Registry Bootstrap (Atoms) ---
    // Ensure core logic atoms exist in the vault and registry
    let registry_path = "../registry.json";
    // Always load atoms first (Bootstrap Registry)
    if !std::path::Path::new(registry_path).exists() {
        println!("[System] Bootstrapping Logic Registry...");
        let loom = aether_store::AetherLoom::new().unwrap();
        let guard = aether_store::AetherGuard::new();
        let mut registry = std::collections::HashMap::new();

        // 1. MODERN_LAW
        let atom_modern = loom.weave("Filter where built > 2020").unwrap();
        let hash_modern = vault.persist_verified(&atom_modern, &guard).unwrap();
        registry.insert("HASH_OF_MODERN_FILTER".to_string(), hash_modern.clone());
        println!("[Registry] Minted MODERN_LAW: {}", hash_modern);

        // 2. RIBA_LAW
        let atom_riba = loom.weave("Verify 0% interest").unwrap();
        let hash_riba = vault.persist_verified(&atom_riba, &guard).unwrap();
        registry.insert("HASH_OF_RIBA_CHECK".to_string(), hash_riba.clone());
        println!("[Registry] Minted RIBA_LAW: {}", hash_riba);
        
        let json = serde_json::to_string_pretty(&registry).unwrap();
        fs::write(registry_path, json).unwrap();
    }
    
    // Read Registry for Hashes needed in templates
    let registry_content = fs::read_to_string(registry_path).unwrap_or("{}".to_string());
    let registry: HashMap<String, String> = serde_json::from_str(&registry_content).unwrap_or_default();
    let hash_modern = registry.get("HASH_OF_MODERN_FILTER").unwrap_or(&"ERROR".to_string()).clone();
    let hash_riba = registry.get("HASH_OF_RIBA_CHECK").unwrap_or(&"ERROR".to_string()).clone();

    // --- Catalog Bootstrap (Templates) ---
    let catalog_path = "../catalog.json";
    if !std::path::Path::new(catalog_path).exists() {
        println!("[System] Bootstrapping Product Catalog...");
        let mut catalog = HashMap::new();
        
        let transit_template = format!(r#"
app_name: "KL Generative Transit"
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
    intent: "Filter where station_type == {{station_type}}"
    dependencies: ["filter_modern"]
  - name: "filter_name"
    intent: "Filter where station contains {{station_name}}"
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
                },
                InputSchema {
                    name: "station_name".to_string(),
                    label: "Preferred Station Name".to_string(),
                    input_type: "text".to_string(),
                    options: None,
                }
            ],
        };
        
        catalog.insert(product.id.clone(), product);
        let json = serde_json::to_string_pretty(&catalog).unwrap();
        fs::write(catalog_path, json).unwrap();
    }

    // --- Start Web Server ---
    let user_vault = Arc::clone(&vault);
    
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/graph", get(move || async move { 
            Json(user_vault.export_graph_json()) 
        }))
        .route("/api/registry", get(|| async {
            match fs::read_to_string("../registry.json") {
                Ok(content) => content,
                Err(_) => "{}".to_string()
            }
        }))
        .route("/api/inspect", get(handle_inspect))
        .route("/api/run_template", post(handle_run_template))
        .route("/api/orchestrate", post(handle_orchestration))
        .route("/api/orchestrate_project", post(handle_orchestrate_project))
        .route("/api/project_schema", post(handle_get_project_schema))
        .route("/api/execute", post(handle_execution_by_hash))
        .route("/api/projects", get(handle_list_projects))
        .with_state(Arc::clone(&vault))
        .layer(cors)
        .fallback_service(ServeDir::new("../universal_shell"));

    println!("[Engine] Universal Logic Engine Active: http://localhost:3000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

#[derive(Deserialize)]
struct ExecuteRequest {
    hash: String,
}

#[derive(Deserialize)]
struct ProjectRequest {
    name: String,
    inputs: Option<HashMap<String, String>>,
}

async fn handle_orchestrate_project(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<ProjectRequest>,
) -> Json<OrchestrationResult> {
    let path = format!("../../products/{}/manifest.yaml", payload.name); 
    match fs::read_to_string(&path) {
        Ok(mut content) => {
             // Templating: Replace {{key}} with value
             if let Some(inputs) = payload.inputs {
                 for (k, v) in inputs {
                     content = content.replace(&format!("{{{{{}}}}}", k), &v);
                 }
             }

             let orchestrator = AetherOrchestrator::new((*vault).clone()).unwrap(); 
             // Build
             match orchestrator.build_app(&content) {
                Ok(root_hash) => {
                     // Exec
                    let kernel = AetherKernel::new((*vault).clone());
                    match kernel.execute_smart(&root_hash).await {
                        Ok(result) => Json(OrchestrationResult {
                            root_hash,
                            output: result,
                            logs: vec![format!("Project '{}' Build & Exec Successful", payload.name)]
                        }),
                        Err(e) => Json(OrchestrationResult {
                            root_hash,
                            output: serde_json::json!({"error": e.to_string()}),
                            logs: vec![format!("Execution Error: {}", e)]
                        })
                    }
                },
                Err(e) => Json(OrchestrationResult {
                     root_hash: String::new(),
                     output: serde_json::json!({"error": e.to_string()}),
                     logs: vec![format!("Build Error: {}", e)]
                })
             }
        },
        Err(e) => Json(OrchestrationResult {
             root_hash: String::new(),
             output: serde_json::json!({"error": e.to_string()}),
             logs: vec![format!("Manifest Read Error: {}", e)]
        })
    }
}

async fn handle_list_projects() -> Json<Vec<String>> {
    let projects_dir = "../../products"; // Relative to warehouse/engine
    let mut projects = Vec::new();

    if let Ok(entries) = fs::read_dir(projects_dir) {
        for entry in entries {
             if let Ok(entry) = entry {
                 if let Ok(file_type) = entry.file_type() {
                     if file_type.is_dir() {
                         if let Ok(name) = entry.file_name().into_string() {
                             projects.push(name);
                         }
                     }
                 }
             }
        }
    }
    Json(projects)
}

#[derive(Deserialize)]
struct ProjectSchemaRequest {
    name: String,
}

async fn handle_get_project_schema(
    Json(payload): Json<ProjectSchemaRequest>,
) -> Json<serde_json::Value> {
    let path = format!("../../products/{}/manifest.yaml", payload.name);
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(manifest) = serde_yaml::from_str::<aether_store::AetherManifest>(&content) {
            return Json(serde_json::json!(manifest.inputs));
        }
    }
    Json(serde_json::json!([]))
}

async fn handle_execution_by_hash(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<ExecuteRequest>,
) -> Json<OrchestrationResult> {
    let kernel = AetherKernel::new((*vault).clone());
    match kernel.execute_smart(&payload.hash).await {
         Ok(result) => Json(OrchestrationResult {
            root_hash: payload.hash,
            output: result,
            logs: vec!["Executed from Registry".to_string()]
        }),
        Err(e) => Json(OrchestrationResult {
            root_hash: payload.hash,
            output: serde_json::json!({"error": e.to_string()}),
            logs: vec![format!("Execution Error: {}", e)]
        })
    }
}

