use aether_store::{AetherVault, AetherKernel, AetherOrchestrator};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vault = AetherVault::new("aether_db")?;
    // Orchestrator owns Loom and Guard internally now
    let orchestrator = AetherOrchestrator::new(vault.clone())?;
    let kernel = AetherKernel::new(vault.clone());

    println!("--- Aether Tool v1.0 (Orchestrator Mode) ---");
    
    // Check if guardian.yaml exists and load it automatically
    let manifest_path = "../guardian.yaml";
    
    if std::path::Path::new(manifest_path).exists() {
        println!("Found Manifest: {}", manifest_path);
        let content = fs::read_to_string(manifest_path)?;
        
        match orchestrator.build_app(&content) {
            Ok(root_hash) => {
                println!("\n[Success] App Built. Root Hash: {}", root_hash);
                if !root_hash.is_empty() {
                    println!("[Kernel] Executing Root Node...");
                    // We assume root node is executable (OpCode 1) for this demo
                    match kernel.execute(&root_hash) {
                        Ok(res) => println!("[Kernel] Root Result: {}", res),
                        Err(e) => println!("[Kernel] Execution Error (Example: Zakat node might not be executable yet): {}", e),
                    }
                }
            },
            Err(e) => println!("[Error] Orchestrator failed: {}", e),
        }

    } else {
        println!("No manifest found at {}. Please create one to use Orchestrator.", manifest_path);
    }

    Ok(())
}

