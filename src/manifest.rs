use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestNode {
    pub name: String,
    pub intent: String, // e.g., "Add user balance"
    pub dependencies: Vec<String>, // Names of other manifest nodes
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AetherManifest {
    pub app_name: String,
    pub laws: Vec<String>, // Genesis rules to apply (e.g., "0_riba")
    pub nodes: Vec<ManifestNode>,
}
