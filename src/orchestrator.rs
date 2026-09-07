use crate::{AetherLoom, AetherVault, AetherGuard, AetherManifest, Limits};
use std::collections::{HashMap, HashSet};
use anyhow::{Result, Context};

pub struct AetherOrchestrator {
    loom: AetherLoom,
    vault: AetherVault,
    guard: AetherGuard,
    limits: Limits,
}

impl AetherOrchestrator {
    pub fn new(vault: AetherVault) -> Result<Self> {
        Ok(Self {
            loom: AetherLoom::new()?,
            vault,
            guard: AetherGuard::new(),
            limits: Limits::default(),
        })
    }

    pub fn with_limits(vault: AetherVault, limits: Limits) -> Result<Self> {
        Ok(Self {
            loom: AetherLoom::new()?,
            vault,
            guard: AetherGuard::new(),
            limits,
        })
    }

    pub fn build_app(&self, manifest_raw: &str) -> Result<(String, Option<String>)> {
        let raw_len = manifest_raw.len();
        if raw_len > self.limits.manifest_max_bytes {
            anyhow::bail!("manifest exceeds {} bytes (got {})", self.limits.manifest_max_bytes, raw_len);
        }

        let manifest: AetherManifest = serde_yaml::from_str(manifest_raw)
            .context("Failed to parse manifest YAML")?;

        if manifest.nodes.len() > self.limits.manifest_max_nodes {
            anyhow::bail!("manifest exceeds {} nodes (got {})", self.limits.manifest_max_nodes, manifest.nodes.len());
        }

        let mut final_manifest = manifest;

        if let Some(ref parent_name) = final_manifest.extends {
            tracing::info!(child = %final_manifest.app_name, parent = %parent_name, "extends");
            let parent_path = crate::Paths::discover().manifest_for(parent_name)
                .map_err(|e| anyhow::anyhow!("resolving parent manifest path: {}", e))?;
            let parent_raw = std::fs::read_to_string(&parent_path)
                .with_context(|| format!("reading parent {}", parent_path.display()))?;
            let parent: AetherManifest = serde_yaml::from_str(&parent_raw)
                .context("parsing parent manifest")?;

            let mut merged_imports = parent.imports;
            merged_imports.extend(final_manifest.imports);
            final_manifest.imports = merged_imports;

            let mut merged_nodes = parent.nodes;
            merged_nodes.extend(final_manifest.nodes);
            final_manifest.nodes = merged_nodes;
        }

        tracing::info!(app = %final_manifest.app_name, "Building App");

        let mut import_map: HashMap<String, String> = HashMap::new();
        for import_item in final_manifest.imports {
            import_map.insert(import_item.name, import_item.hash);
        }

        let mut node_map: HashMap<String, String> = HashMap::new();
        let mut root_hint: Option<String> = None;

        let mut available_deps: HashSet<String> = import_map.keys().cloned().collect();
        let mut pending_nodes: std::collections::VecDeque<crate::manifest::ManifestNode> = final_manifest.nodes.into();
        let mut sorted_nodes = Vec::new();
        let mut stuck_counter = 0;

        while let Some(node) = pending_nodes.pop_front() {
            let all_met = node.dependencies.iter().all(|d| available_deps.contains(d));
            if all_met {
                available_deps.insert(node.name.clone());
                sorted_nodes.push(node);
                stuck_counter = 0;
            } else {
                pending_nodes.push_back(node);
                stuck_counter += 1;
                if stuck_counter > pending_nodes.len() && !pending_nodes.is_empty() {
                    anyhow::bail!(
                        "topological sort failed: cyclic or missing dependency detected after {} nodes",
                        sorted_nodes.len()
                    );
                }
            }
        }

        for node in sorted_nodes {
            tracing::info!(node = %node.name, "processing");
            if node.name == "root" {
                root_hint = node.ui_hint.clone();
            }

            let mut atom = if let Some(ref intent) = node.intent {
                self.loom.weave_with_context(intent, &final_manifest.app_name)?
            } else if let Some(ref ref_name) = node.use_ref {
                let mut hash = import_map.get(ref_name).cloned()
                    .ok_or_else(|| anyhow::anyhow!("import not found: {}", ref_name))?;
                if let Ok(reg_str) = std::fs::read_to_string(
                    crate::Paths::discover().registry_path
                ) {
                    if let Ok(reg) = serde_json::from_str::<HashMap<String, String>>(&reg_str) {
                        if let Some(true_hash) = reg.get(&hash) {
                            hash = true_hash.clone();
                        }
                    }
                }
                tracing::info!(ref_name = %ref_name, hash = %hash, "linking master atom");
                let master_atom = self.vault.fetch(&hash)
                    .with_context(|| format!("master atom {} not found", hash))?;
                crate::LogicAtom {
                    op_code: master_atom.op_code,
                    inputs: vec![],
                    storage_ref: master_atom.storage_ref.clone(),
                    context_id: final_manifest.app_name.clone(),
                }
            } else {
                anyhow::bail!("node '{}' must have either 'intent' or 'use_ref'", node.name);
            };

            for dep_name in &node.dependencies {
                if let Some(dep_hash) = node_map.get(dep_name) {
                    atom.inputs.push(dep_hash.clone());
                } else {
                    tracing::warn!(dep = %dep_name, node = %node.name, "dependency not found, dropping");
                }
            }

            // Depth check: input chain length.
            let mut depth = 0usize;
            let mut frontier: Vec<String> = atom.inputs.clone();
            let mut visited: HashSet<String> = HashSet::new();
            while let Some(h) = frontier.pop() {
                if !visited.insert(h.clone()) { continue; }
                depth += 1;
                if depth > self.limits.manifest_max_depth * 8 {
                    anyhow::bail!("node '{}' input chain too deep", node.name);
                }
                if let Ok(parent) = self.vault.fetch(&h) {
                    frontier.extend(parent.inputs);
                }
            }

            let hash = self.vault.persist_verified(&atom, &self.guard)
                .with_context(|| format!("Guard rejected node '{}'", node.name))?;
            tracing::info!(node = %node.name, hash = %hash, "persisted");
            node_map.insert(node.name.clone(), hash.clone());
        }

        match node_map.get("root") {
            Some(h) => Ok((h.clone(), root_hint)),
            None => {
                let last = node_map.values().last().cloned().unwrap_or_default();
                Ok((last, root_hint))
            }
        }
    }
}