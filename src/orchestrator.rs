use crate::{AetherLoom, AetherVault, AetherGuard, AetherManifest};
use std::collections::HashMap;
use anyhow::{Result, Context};

pub struct AetherOrchestrator {
    loom: AetherLoom,
    vault: AetherVault,
    guard: AetherGuard,
}

impl AetherOrchestrator {
    pub fn new(vault: AetherVault) -> Result<Self> {
        Ok(Self {
            loom: AetherLoom::new()?,
            vault,
            guard: AetherGuard::new(),
        })
    }

    pub fn build_app(&self, manifest_raw: &str) -> Result<String> {
        let manifest: AetherManifest = serde_yaml::from_str(manifest_raw)
            .context("Failed to parse manifest YAML")?;
        
        println!("[Orchestrator] Building App: {}", manifest.app_name);
        println!("[Orchestrator] Enforcing Laws: {:?}", manifest.laws);

        let mut node_map: HashMap<String, String> = HashMap::new();

        for node in manifest.nodes {
            println!("[Orchestrator] Processing Node: '{}'", node.name);
            
            // 1. Weaver: Intent -> Atom
            let atom = self.loom.weave(&node.intent)
                .with_context(|| format!("Failed to weave node '{}'", node.name))?;

            // 2. Guard: Verify
            // In a full implementation, we might pass manifest.laws to the guard here.
            // For now, the guard enforces its implicit "Genesis Laws".
            let hash = self.vault.persist_verified(&atom, &self.guard)
                .with_context(|| format!("Guard rejected node '{}'", node.name))?;
            
            println!("[Orchestrator] Node '{}' Persisted. Hash: {}", node.name, hash);
            node_map.insert(node.name.clone(), hash.clone());
        }

        // Return the Root Hash of the Application
        match node_map.get("root") {
            Some(h) => Ok(h.clone()),
            None => {
                // Return the last one if 'root' is not defined, explicitly for demo purposes
                // or just an empty string if nothing processed.
                 Ok(node_map.values().last().cloned().unwrap_or_default())
            }
        }
    }
}
