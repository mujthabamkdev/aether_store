use aether_store::{AetherVault, AetherKernel, AetherOrchestrator};
use std::fs;
use std::sync::Arc;
use axum::{Router, routing::get, Json};
use tower_http::services::ServeDir;

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
    // Since we are running single-threaded logically (CLI then Server), re-opening or sharing is fine.
    // However, sled::Db is thread safe.
    
    // Changing AetherVault to allow cloning (we did derive Clone earlier!)
    let vault_for_cli = (*vault).clone(); 
    
    let orchestrator = AetherOrchestrator::new(vault_for_cli.clone())?;
    let kernel = AetherKernel::new(vault_for_cli.clone());

    println!("--- Aether Tool v1.0 (Orchestrator + Logic Grid Mode) ---");
    
    // ... CLI Logic ...
    let manifest_path = "../guardian.yaml";
    if std::path::Path::new(manifest_path).exists() {
        println!("Found Manifest: {}", manifest_path);
        let content = fs::read_to_string(manifest_path)?;
        
        match orchestrator.build_app(&content) {
            Ok(root_hash) => {
                println!("\n[Success] App Built. Root Hash: {}", root_hash);
                if !root_hash.is_empty() {
                    println!("[Kernel] Executing Root Node...");
                    match kernel.execute(&root_hash) {
                        Ok(res) => println!("[Kernel] Root Result: {}", res),
                        Err(e) => println!("[Kernel] Execution Error: {}", e),
                    }
                }
            },
            Err(e) => println!("[Error] Orchestrator failed: {}", e),
        }
    } else {
        println!("No manifest found. Skipping build.");
    }

    // Start Web Server
    let user_vault = Arc::clone(&vault);
    let app = Router::new()
        .route("/api/graph", get(move || async move { 
            Json(user_vault.export_graph_json()) 
        }))
        .fallback_service(ServeDir::new("static"));

    println!("\n[Logic Grid] Visualization Active: http://localhost:3000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

