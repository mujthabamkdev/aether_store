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
        
        let mut final_manifest = manifest;
        
        // RECURSION: If extends is set, load parent and merge
        if let Some(ref parent_name) = final_manifest.extends {
            println!("[Orchestrator] Recursion: '{}' extends '{}'", final_manifest.app_name, parent_name);
            let parent_path = format!("../../products/{}/manifest.yaml", parent_name);
            let parent_raw = std::fs::read_to_string(&parent_path)
                .with_context(|| format!("Failed to read parent manifest at '{}'", parent_path))?;
            
            let parent: AetherManifest = serde_yaml::from_str(&parent_raw)
                .context("Failed to parse parent manifest")?;
                
            // Merge Strategies
            // 1. Imports: Child overrides Parent (if duplicates, though vec doesn't map, so we append unique?)
            //    Actually, simple append. If name collision, Child's usage will pick one (last one? first one? map insert logic)
            //    We convert to map below, so last one wins. We want Child to win. 
            //    So append Child imports AFTER Parent imports.
            let mut merged_imports = parent.imports;
            merged_imports.extend(final_manifest.imports);
            final_manifest.imports = merged_imports;
            
            // 2. Nodes: Parent Nodes come FIRST (Global Laws), then Child Nodes.
            //    Logic graph executes by dependency. If Child depends on Parent, Parent must exist in map.
            //    So we process Parent Nodes first.
            let mut merged_nodes = parent.nodes;
            merged_nodes.extend(final_manifest.nodes);
            final_manifest.nodes = merged_nodes;
        }

        println!("[Orchestrator] Building App: {}", final_manifest.app_name);
        // Laws are applied via Registry imports now

        // 0. Resolve Imports
        let mut import_map: HashMap<String, String> = HashMap::new();
        for import_item in final_manifest.imports {
            import_map.insert(import_item.name, import_item.hash);
        }

        let mut node_map: HashMap<String, String> = HashMap::new();

        for node in final_manifest.nodes {
            println!("[Orchestrator] Processing Node: '{}'", node.name);
            
            // 1. Resolve Logic: Intent (New) vs use_ref (Linked)
            let mut atom = if let Some(ref intent) = node.intent {
                // Generative Mode: Ask Loom (Use Manifest App Name as Context)
                 self.loom.weave_with_context(intent, &final_manifest.app_name)
                    .with_context(|| format!("Failed to weave node '{}'", node.name))?
            } else if let Some(ref ref_name) = node.use_ref {
                // Linker Mode: Fetch from Registry/Vault
                if let Some(hash) = import_map.get(ref_name) {
                    println!("[Orchestrator] Linking to Master Atom: {} -> {}", ref_name, hash);
                    // Fetch the master atom to use as a template
                    // We need to clone it because we will modify its inputs (dependencies)
                    let master_atom = self.vault.fetch(hash)
                        .with_context(|| format!("Failed to fetch imported atom '{}' ({})", ref_name, hash))?;
                    
                    // Create a new instance (same logic/data, new inputs)
                    // Context ID: Keep the Master's Context (e.g., "global") or override?
                    // Inheritance Principle: If I use "Global Riba Law", I am creating a "Project X Riba Check" node?
                    // No, the node *is* the application of the law.
                    // If the node is "My Law Check", it belongs to "Project X".
                    // But the logic comes from "Global".
                    // Let's set the context of this specific *instance* (node in the graph) to Project X.
                    // This allows "Project X" to execute it.
                    // If we kept "global", then "Project X" executing "global" atom is fine IF Guard allows Global.
                    // BUT, if we set it to Project X, we are "contextualizing" the instance.
                    // Let's set it to Project X (Manifest App Name).
                    crate::LogicAtom {
                        op_code: master_atom.op_code,
                        inputs: vec![], // Will be filled below
                        storage_ref: master_atom.storage_ref.clone(),
                        context_id: final_manifest.app_name.clone(),
                    }
                } else {
                    return Err(anyhow::anyhow!("Import reference '{}' not found in manifest imports", ref_name));
                }
            } else {
                return Err(anyhow::anyhow!("Node '{}' must have either 'intent' or 'use_ref'", node.name));
            };

            // 1.5 Link Dependencies
            for dep_name in &node.dependencies {
                if let Some(dep_hash) = node_map.get(dep_name) {
                    atom.inputs.push(dep_hash.clone());
                } else {
                    println!("[Orchestrator] Warning: Dependency '{}' not found for node '{}'", dep_name, node.name);
                }
            }

            // 2. Guard: Verify
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
