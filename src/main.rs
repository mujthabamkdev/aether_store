use aether_store::{AetherVault, AetherKernel, AetherOrchestrator, ProductTemplate, InputSchema, ProjectAtom, ProjectStatus};
use std::fs;
use std::sync::Arc;
use std::env;
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
    ui_hint: Option<String>,
    output: serde_json::Value,
    logs: Vec<String>,
}

#[derive(Deserialize)]
struct RunTemplateRequest {
    product_id: String,
    inputs: HashMap<String, String>,
}

#[derive(Deserialize)]
struct TemplateRequest {
    template: String,
}

#[derive(Deserialize)]
struct InspectRequest {
    format: String, // "json" or "dot"
}

#[derive(Deserialize)]
struct ChatRequest {
    project: String,
    hash: Option<String>,
    message: String,
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
    label: String,
    input_type: String, // text, select, number
    options: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ManifestPatch {
    add_nodes: Option<Vec<LogicNodePatch>>,
    modify_nodes: Option<Vec<LogicNodePatch>>,
    remove_nodes: Option<Vec<String>>,
    add_inputs: Option<Vec<InputPatch>>,
    modify_inputs: Option<Vec<InputPatch>>,
    remove_inputs: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct WeaveRequest {
    project: String,
    current_hash: Option<String>,
    patch: ManifestPatch,
}

#[derive(Serialize)]
struct InspectResult {
    dot_graph: String,
}

async fn handle_orchestration(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<OrchestrationRequest>,
) -> Json<OrchestrationResult> {
    
    // TODO: Extract Identity Hash from Headers
    // let user_hash = "mock_user_hash";
    
    // 1. Build the App from the manifest
    let orchestrator = AetherOrchestrator::new((*vault).clone()).unwrap(); 
    
    match orchestrator.build_app(&payload.manifest) {
        Ok((root_hash, ui_hint)) => {
            
            // 2. Verify Resonance (Sovereign Gate)
            // In a real implementation, we check if the user has permission to EXECUTE this root hash.
            // For now, we assume if they can build it, they can run it (Architect Mode).
            // if !vault.verify_resonance(user_hash, &root_hash) { ... }

            // 3. Execute
            let kernel = AetherKernel::new((*vault).clone());
            match kernel.execute_smart(&root_hash).await {
                Ok(result) => Json(OrchestrationResult {
                    root_hash,
                    ui_hint,
                    output: result,
                    logs: vec!["Execution Successful".to_string()]
                }),
                Err(e) => Json(OrchestrationResult {
                    root_hash,
                    ui_hint: None,
                    output: serde_json::json!({"error": e.to_string()}),
                    logs: vec![format!("Execution Error: {}", e)]
                })
            }
        },
        Err(e) => Json(OrchestrationResult {
            root_hash: String::new(),
            ui_hint: None,
            output: serde_json::json!({"error": e.to_string()}),
            logs: vec![format!("Build Error: {}", e)]
        })
    }
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
            Ok((root_hash, ui_hint)) => {
                let kernel = AetherKernel::new((*vault).clone());
                match kernel.execute_smart(&root_hash).await {
                    Ok(result) => Json(OrchestrationResult {
                        root_hash,
                        ui_hint,
                        output: result,
                        logs: vec!["Template Executed".to_string()]
                    }),
                    Err(e) => Json(OrchestrationResult {
                        root_hash,
                        ui_hint: None,
                        output: serde_json::json!({"error": e.to_string()}),
                        logs: vec![format!("Execution Error: {}", e)]
                    })
                }
            },
            Err(e) => Json(OrchestrationResult {
                root_hash: String::new(),
                ui_hint: None,
                output: serde_json::json!({"error": e.to_string()}),
                logs: vec![format!("Build Error: {}", e)]
            })
        }
    } else {
        Json(OrchestrationResult {
            root_hash: String::new(),
            ui_hint: None,
            output: serde_json::json!({"error": "Product ID not found"}),
            logs: vec!["Catalog Error".to_string()]
        })
    }
}

async fn handle_inspect(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<InspectRequest>,
) -> Json<InspectResult> {
    let result = if payload.format == "json" {
        vault.export_graph_json().to_string()
    } else {
        vault.export_graph_viz()
    };
    
    Json(InspectResult {
        dot_graph: result
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();
    
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

    // --- MIGRATION & REPAIR ---
    println!("[System] Verifying Project Registry...");
    if let Ok(projects) = vault.list_projects() {
        if projects.is_empty() {
             println!("[Migration] Registry empty. Scanning filesystem...");
             // ... (Existing Migration Logic - can be simplified or merged) ...
             let projects_dir = "../../products"; 
             if let Ok(entries) = fs::read_dir(projects_dir) {
                 for entry in entries {
                     if let Ok(entry) = entry {
                         if entry.path().is_dir() {
                             if let Ok(name) = entry.file_name().into_string() {
                                 // Register as legacy first, let repair logic handle build
                                 println!("[Migration] Discovered: {}", name);
                                 let atom = aether_store::ProjectAtom {
                                     name: name.clone(),
                                     root_hash: "legacy_fs_root".to_string(), 
                                     org_hash: "global".to_string(),
                                     status: aether_store::ProjectStatus::Active,
                                     created_at: 0,
                                 };
                                 let _ = vault.persist_project(&atom);
                             }
                         }
                     }
                 }
             }
        }
    }

    // REPAIR PASS: Fix any "legacy_fs_root" projects
    if let Ok(projects) = vault.list_projects() {
        for mut proj in projects {
            if proj.root_hash == "legacy_fs_root" {
                println!("[Repair] Project '{}' needs logic build...", proj.name);
                let manifest_path = format!("../../products/{}/manifest.yaml", proj.name);
                
                if let Ok(content) = fs::read_to_string(&manifest_path) {
                    if let Ok(orchestrator) = AetherOrchestrator::new(vault.as_ref().clone()) {
                        match orchestrator.build_app(&content) {
                            Ok((hash, _)) => {
                                println!("[Repair] Built '{}' -> Root Hash: {}", proj.name, hash);
                                proj.root_hash = hash;
                                proj.status = aether_store::ProjectStatus::Active;
                                let _ = vault.persist_project(&proj);
                            },
                            Err(e) => println!("[Repair] Failed to build '{}': {}", proj.name, e),
                        }
                    }
                } else {
                    println!("[Repair] Manifest missing for '{}'", proj.name);
                }
            }
        }
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
        .route("/api/deploy", post(handle_deploy))
        .route("/api/project_schema", post(handle_get_project_schema))
        .route("/api/execute", post(handle_execution_by_hash))
        .route("/api/projects", get(handle_list_projects))
        .route("/api/chat", post(handle_chat))
        .route("/api/project/weave", post(handle_weave))
        .route("/api/warehouse/inventory", get(handle_warehouse_inventory))
        .route("/api/warehouse/inject", post(handle_warehouse_inject))
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
    // 1. Initial Status: Building
    let project_atom = ProjectAtom {
        name: payload.name.clone(),
        root_hash: String::new(),
        org_hash: "legacy_org".to_string(),
        status: ProjectStatus::Building,
        created_at: 0,
    };
    let _ = vault.persist_project(&project_atom); // Persist Initial State

    // 2. Load Manifest (Currently FS, future Sled)
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
                Ok((root_hash, ui_hint)) => {
                     // 3. Update Status: Active
                     let _ = vault.update_project_status(&payload.name, ProjectStatus::Active);
                     // Update Root Hash in separate atomic op or refetch-modify-save (Simplified here)
                     // Ideally persist_project should be upsert. 
                     // For now, re-save with Hash
                     let final_atom = ProjectAtom {
                        name: payload.name.clone(),
                        root_hash: root_hash.clone(),
                        org_hash: "legacy_org".to_string(),
                        status: ProjectStatus::Active,
                        created_at: 0,
                     };
                     let _ = vault.persist_project(&final_atom);

                     // Exec
                    let kernel = AetherKernel::new((*vault).clone());
                    match kernel.execute_smart(&root_hash).await {
                        Ok(result) => Json(OrchestrationResult {
                            root_hash,
                            ui_hint,
                            output: result,
                            logs: vec![format!("Project '{}' Build & Exec Successful", payload.name)]
                        }),
                        Err(e) => Json(OrchestrationResult {
                            root_hash,
                            ui_hint: None,
                            output: serde_json::json!({"error": e.to_string()}),
                            logs: vec![format!("Execution Error: {}", e)]
                        })
                    }
                },
                Err(e) => Json(OrchestrationResult {
                     root_hash: String::new(),
                     ui_hint: None,
                     output: serde_json::json!({"error": e.to_string()}),
                     logs: vec![format!("Build Error: {}", e)]
                })
             }
        },
        Err(e) => Json(OrchestrationResult {
             root_hash: String::new(),
             ui_hint: None,
             output: serde_json::json!({"error": e.to_string()}),
             logs: vec![format!("Manifest Read Error: {}", e)]
        })
    }
}

#[derive(Serialize)]
struct DeployResult {
    app_url: String,
    root_hash: String,
}

async fn handle_deploy(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<ProjectRequest>,
) -> Json<DeployResult> {
    // 1. Build & Orchestrate to freeze logic
    let path = format!("../../products/{}/manifest.yaml", payload.name); 
    if let Ok(mut content) = fs::read_to_string(&path) {
         if let Some(inputs) = payload.inputs {
             for (k, v) in inputs {
                 content = content.replace(&format!("{{{{{}}}}}", k), &v);
             }
         }
         
         let orchestrator = AetherOrchestrator::new((*vault).clone()).unwrap();
         if let Ok((root_hash, _)) = orchestrator.build_app(&content) {
             return Json(DeployResult {
                 app_url: format!("http://localhost:3000/?app={}", root_hash),
                 root_hash,
             });
         }
    }
    Json(DeployResult {
        app_url: "error".to_string(),
        root_hash: "error".to_string()
    })
}

async fn handle_list_projects(
    State(vault): State<Arc<AetherVault>>,
) -> Json<Vec<ProjectAtom>> {
    // 1. Fetch from Sled (Source of Truth)
    if let Ok(projects) = vault.list_projects() {
        return Json(projects);
    }
    
    // Fallback? Or just empty.
    Json(Vec::new())
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
            return Json(serde_json::json!({
                "app_name": manifest.app_name,
                "inputs": manifest.inputs
            }));
        }
    }
    Json(serde_json::json!({"app_name": payload.name, "inputs": []}))
}

async fn handle_execution_by_hash(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<ExecuteRequest>,
) -> Json<OrchestrationResult> {
    let kernel = AetherKernel::new((*vault).clone());
    match kernel.execute_smart(&payload.hash).await {
         Ok(result) => Json(OrchestrationResult {
            root_hash: payload.hash,
            ui_hint: None, // Logic Execution doesn't re-parse manifest, so hint is lost unless stored in Atom?
            // For now, raw execution has no hint.
            output: result,
            logs: vec!["Executed from Registry".to_string()]
        }),
        Err(e) => Json(OrchestrationResult {
            root_hash: payload.hash,
            ui_hint: None,
            output: serde_json::json!({"error": e.to_string()}),
            logs: vec![format!("Execution Error: {}", e)]
        })
    }
}

async fn handle_chat(
    Json(payload): Json<ChatRequest>,
) -> Json<serde_json::Value> {
    // Read project manifest for context
    let manifest_path = format!("../../products/{}/manifest.yaml", payload.project);
    let manifest_info = fs::read_to_string(&manifest_path).unwrap_or_default();
    
    // Build system prompt (shared by all APIs)
    let system_prompt = format!(r#"You are the Resident Architect for the '{}' project. Your job is to analyze user requests and generate manifest patches to modify the project's logic.

CURRENT MANIFEST:
```yaml
{}
```

AVAILABLE NODE OPERATIONS:
1. ADD nodes - create new logic nodes with name, intent, and dependencies
2. MODIFY nodes - change existing node intent or dependencies  
3. REMOVE nodes - delete nodes by name

RESPONSE FORMAT:
If the user wants to modify the project logic (add features, fix bugs, add data sources, etc.), respond with JSON:
{{
  "mode": "WEAVE",
  "response": "Brief explanation of what you'll do",
  "patch": {{
    "add_nodes": [{{ "name": "node_name", "intent": "what it does", "dependencies": ["parent_node"] }}],
    "modify_nodes": [{{ "name": "existing_node", "intent": "new intent", "dependencies": ["deps"] }}],
    "remove_nodes": ["node_to_remove"]
  }}
}}

If the user is just asking questions (explain, what is, how does), respond with JSON:
{{
  "mode": "CHAT",
  "response": "Your helpful explanation"
}}

IMPORTANT:
- For data scraping requests, add a node with intent describing the scraping task
- For optimization, modify existing nodes or add caching nodes
- For new features, add appropriate nodes with clear intents
- REWIRING IS CRITICAL: If you add a node X that should be part of a flow A->B, you MUST:
   1. Add X with dependency [A]
   2. MODIFY B to change its dependency from [A] to [X]
- Failure to rewire means the new node will be ignored.
- INPUT SCHEMA: If your new logic requires user input (e.g., sort order, price limit), you MUST add it to `add_inputs`:
    {{ "name": "var_name", "label": "User Label", "input_type": "select|text|number", "options": ["opt1", "opt2"] }}
    Then use `{{var_name}}` in your node intent.
- SYNC INPUTS: If you add support for a new value (e.g. "KTM" station) in logic, you MUST also add it to the `modify_inputs` options list if an input exists.
- MODIFYING INPUTS: Use `modify_inputs` to update options or labels.
- REMOVING INPUTS: Use `remove_inputs`: ["var_name"]
- Keep node names lowercase with underscores
- Be concise but helpful in your response"#, payload.project, manifest_info.chars().take(2000).collect::<String>());

    let client = reqwest::Client::new();
    let user_message = payload.message.clone();
    let project_name = payload.project.clone();
    
    // 1. Try OpenRouter (Primary)
    if let Ok(or_key) = env::var("OPENROUTER_API_KEY") {
        println!("[AI] Trying OpenRouter API...");
        if let Some(result) = try_openrouter(&client, &or_key, &system_prompt, &user_message, &project_name).await {
            return Json(result);
        }
        println!("[AI Warning] OpenRouter failed, falling back...");
    }

    // 2. Try Gemini (Fallback)
    if let Ok(gemini_key) = env::var("GEMINI_API_KEY") {
        println!("[AI] Trying Gemini API...");
        if let Some(result) = try_gemini(&client, &gemini_key, &system_prompt, &user_message, &project_name).await {
            return Json(result);
        }
    }
    
    Json(serde_json::json!({
        "mode": "CHAT",
        "response": "⚠️ All AI APIs are unavailable. Please check your API keys or try again later.",
        "project": project_name
    }))
}

async fn handle_warehouse_inventory(
    State(vault): State<Arc<AetherVault>>,
) -> Json<Vec<serde_json::Value>> {
    let inventory = vault.inventory();
    Json(inventory)
}

#[derive(Deserialize)]
struct InjectRequest {
    spec: serde_json::Value, // The logic atom spec
}

async fn handle_warehouse_inject(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<InjectRequest>,
) -> Json<serde_json::Value> {
    // 1. Hash the spec (Deterministic ID)
    let spec_bytes = serde_json::to_vec(&payload.spec).unwrap();
    let hash = blake3::hash(&spec_bytes).to_hex().to_string();
    
    // 2. Persist to Sled (Simulated logic atom wrapper)
    if let Ok(atom) = serde_json::from_value::<aether_store::LogicAtom>(payload.spec.clone()) {
        match vault.inject_atom(&atom) {
            Ok(hash) => Json(serde_json::json!({"hash": hash, "status": "Injected"})),
            Err(e) => Json(serde_json::json!({"error": e.to_string()}))
        }
    } else {
        Json(serde_json::json!({"error": "Invalid Atom Spec"}))
    }
}

async fn try_openrouter(
    client: &reqwest::Client,
    api_key: &str,
    system_prompt: &str,
    user_message: &str,
    project_name: &str,
) -> Option<serde_json::Value> {
    let body = serde_json::json!({
        "model": "google/gemini-2.0-flash-001", // Specific model
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_message}
        ],
        "temperature": 0.7,
        "max_tokens": 2000
    });
    
    // Debug
    println!("[OpenRouter] Sending request for project: {}", project_name);

    let response = client.post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("HTTP-Referer", "http://localhost:3000")
        .header("X-Title", "Aether Engine")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;
    
    let text = response.text().await.ok()?;
    // Debug raw response
    println!("[OpenRouter] Raw response len: {}", text.len());
    
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    
    // Check for errors
    if json.get("error").is_some() {
        println!("[OpenRouter] Error: {}", json["error"]["message"].as_str().unwrap_or("Unknown"));
        return None;
    }
    
    let ai_text = json["choices"][0]["message"]["content"].as_str()?;
    if ai_text.trim().is_empty() {
        println!("[OpenRouter] Error: Empty response text");
        return None;
    }
    
    println!("[OpenRouter] Success! Response: {}...", ai_text.chars().take(100).collect::<String>());
    
    Some(parse_ai_response(ai_text, project_name))
}

fn parse_ai_response(ai_text: &str, project_name: &str) -> serde_json::Value {
    // Try to find JSON in the response
    let json_start = ai_text.find('{');
    let json_end = ai_text.rfind('}');
    
    if let (Some(start), Some(end)) = (json_start, json_end) {
        let json_str = &ai_text[start..=end];
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
            let mode = parsed["mode"].as_str().unwrap_or("CHAT");
            let response = parsed["response"].as_str().unwrap_or("").to_string();
            
            if mode == "WEAVE" && parsed.get("patch").is_some() {
                return serde_json::json!({
                    "mode": "WEAVE",
                    "response": response,
                    "patch": parsed["patch"],
                    "project": project_name
                });
            } else {
                return serde_json::json!({
                    "mode": "CHAT",
                    "response": response,
                    "project": project_name
                });
            }
        }
    }
    
    // Return as plain text
    serde_json::json!({
        "mode": "CHAT",
        "response": ai_text,
        "project": project_name
    })
}

async fn try_gemini(
    client: &reqwest::Client,
    api_key: &str,
    system_prompt: &str,
    user_message: &str,
    project_name: &str,
) -> Option<serde_json::Value> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-preview-05-20:generateContent?key={}",
        api_key
    );
    
    let body = serde_json::json!({
        "contents": [{
            "parts": [{"text": format!("{}\n\nUser request: {}", system_prompt, user_message)}]
        }],
        "generationConfig": {"temperature": 0.7, "topP": 0.95, "maxOutputTokens": 1024}
    });
    
    let response = client.post(&url).json(&body).send().await.ok()?;
    let text = response.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    
    // Check for errors
    if json.get("error").is_some() {
        let code = json["error"]["code"].as_i64().unwrap_or(0);
        println!("[Gemini] Error {}: {}", code, json["error"]["message"].as_str().unwrap_or("Unknown"));
        return None;
    }
    
    let ai_text = json["candidates"][0]["content"]["parts"][0]["text"].as_str()?;
    println!("[Gemini] Success! Response: {}...", ai_text.chars().take(100).collect::<String>());
    
    Some(parse_ai_response(ai_text, project_name))
}

async fn handle_weave(
    State(vault): State<Arc<AetherVault>>,
    Json(payload): Json<WeaveRequest>,
) -> Json<serde_json::Value> {
    let manifest_path = format!("../../products/{}/manifest.yaml", payload.project);
    
    // Read current manifest
    let manifest_content = match fs::read_to_string(&manifest_path) {
        Ok(content) => content,
        Err(e) => return Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to read manifest: {}", e)
        }))
    };
    
    // Parse manifest
    let mut manifest: serde_yaml::Value = match serde_yaml::from_str(&manifest_content) {
        Ok(m) => m,
        Err(e) => return Json(serde_json::json!({
            "success": false,
            "error": format!("Failed to parse manifest: {}", e)
        }))
    };
    
    // Get nodes array
    let nodes = match manifest.get_mut("nodes").and_then(|n| n.as_sequence_mut()) {
        Some(n) => n,
        None => return Json(serde_json::json!({
            "success": false,
            "error": "Manifest has no nodes section"
        }))
    };
    
    let mut changes = Vec::new();
    
    // Remove nodes
    if let Some(remove_list) = &payload.patch.remove_nodes {
        for name in remove_list {
            nodes.retain(|n| n.get("name").and_then(|v| v.as_str()) != Some(name.as_str()));
            changes.push(format!("Removed: {}", name));
        }
    }
    
    // Modify nodes
    if let Some(modify_list) = &payload.patch.modify_nodes {
        for patch in modify_list {
            for node in nodes.iter_mut() {
                if node.get("name").and_then(|v| v.as_str()) == Some(&patch.name) {
                    node["intent"] = serde_yaml::Value::String(patch.intent.clone());
                    let deps: Vec<serde_yaml::Value> = patch.dependencies.iter()
                        .map(|d| serde_yaml::Value::String(d.clone())).collect();
                    node["dependencies"] = serde_yaml::Value::Sequence(deps);
                    changes.push(format!("Modified: {}", patch.name));
                }
            }
        }
    }
    
    // Add nodes
    if let Some(add_list) = &payload.patch.add_nodes {
        for patch in add_list {
            let mut new_node = serde_yaml::Mapping::new();
            new_node.insert(serde_yaml::Value::String("name".into()), serde_yaml::Value::String(patch.name.clone()));
            new_node.insert(serde_yaml::Value::String("intent".into()), serde_yaml::Value::String(patch.intent.clone()));
            let deps: Vec<serde_yaml::Value> = patch.dependencies.iter()
                .map(|d| serde_yaml::Value::String(d.clone())).collect();
            new_node.insert(serde_yaml::Value::String("dependencies".into()), serde_yaml::Value::Sequence(deps));
            nodes.push(serde_yaml::Value::Mapping(new_node));
            changes.push(format!("Added: {}", patch.name));
        }
    }

    // --- Input Patching ---
    // Ensure inputs section exists
    if manifest.get("inputs").is_none() {
        if let Some(map) = manifest.as_mapping_mut() {
            map.insert(serde_yaml::Value::String("inputs".into()), serde_yaml::Value::Sequence(Vec::new()));
        }
    }

    let inputs = match manifest.get_mut("inputs").and_then(|n| n.as_sequence_mut()) {
        Some(n) => n,
        None => return Json(serde_json::json!({
            "success": false,
            "error": "Manifest inputs section missing or invalid"
        }))
    };

    if let Some(add_inputs) = &payload.patch.add_inputs {
        for patch in add_inputs {
            let mut new_input = serde_yaml::Mapping::new();
            new_input.insert(serde_yaml::Value::String("name".into()), serde_yaml::Value::String(patch.name.clone()));
            new_input.insert(serde_yaml::Value::String("label".into()), serde_yaml::Value::String(patch.label.clone()));
            new_input.insert(serde_yaml::Value::String("input_type".into()), serde_yaml::Value::String(patch.input_type.clone()));
            if let Some(opts) = &patch.options {
                 let opt_vec: Vec<serde_yaml::Value> = opts.iter().map(|o| serde_yaml::Value::String(o.clone())).collect();
                 new_input.insert(serde_yaml::Value::String("options".into()), serde_yaml::Value::Sequence(opt_vec));
            }
            inputs.push(serde_yaml::Value::Mapping(new_input));
            changes.push(format!("Added Input: {}", patch.name));
        }
    }

    if let Some(remove_inputs) = &payload.patch.remove_inputs {
        for name in remove_inputs {
            inputs.retain(|n| n.get("name").and_then(|v| v.as_str()) != Some(name.as_str()));
            changes.push(format!("Removed Input: {}", name));
        }
    }
    
    // Write manifest
    let new_yaml = match serde_yaml::to_string(&manifest) {
        Ok(s) => s,
        Err(e) => return Json(serde_json::json!({"success": false, "error": format!("Serialize error: {}", e)}))
    };
    
    if let Err(e) = fs::write(&manifest_path, &new_yaml) {
        return Json(serde_json::json!({"success": false, "error": format!("Write error: {}", e)}));
    }
    
    // Build new hash using orchestrator
    let orchestrator = match aether_store::AetherOrchestrator::new((*vault).clone()) {
        Ok(o) => o,
        Err(e) => return Json(serde_json::json!({"success": false, "error": format!("Orchestrator error: {}", e)}))
    };
    let new_hash = match orchestrator.build_app(&new_yaml) {
        Ok((h, _)) => h,
        Err(e) => return Json(serde_json::json!({"success": false, "error": format!("Build error: {}", e)}))
    };
    
    println!("[Weave] '{}' updated -> {}", payload.project, new_hash);
    
    // CRITICAL FIX: Persist the new hash to the Vault so the UI sees it!
    if let Err(e) = vault.update_project_hash(&payload.project, &new_hash) {
         println!("[Weave Error] Failed to update project hash: {}", e);
         // Don't fail the request, but warn.
    }

    Json(serde_json::json!({
        "success": true,
        "new_hash": new_hash,
        "changes": changes,
        "project": payload.project
    }))
}

