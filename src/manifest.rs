use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestImport {
    pub name: String,
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestNode {
    pub name: String,
    pub intent: Option<String>, // Optional if using existing logic
    pub use_ref: Option<String>, // Reference to an imported name
    pub dependencies: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AetherManifest {
    pub app_name: String,
    pub extends: Option<String>, // Recursion Key
    #[serde(default)]
    pub imports: Vec<ManifestImport>,
    // Laws are now handled via Imports/Guard, removed legacy string list
    pub nodes: Vec<ManifestNode>,
}
