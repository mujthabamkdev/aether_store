use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestImport {
    pub name: String,
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManifestNode {
    pub name: String,
    pub intent: Option<String>, // Optional if using existing logic
    pub use_ref: Option<String>, // Reference to an imported name
    pub ui_hint: Option<String>, // New Field: "Dashboard", "Card", "Table", etc.
    #[serde(default)]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AetherManifest {
    pub app_name: String,
    pub extends: Option<String>,
    #[serde(default)]
    pub inputs: Vec<crate::InputSchema>, // UI Form Def
    #[serde(default)]
    pub imports: Vec<ManifestImport>,
    pub nodes: Vec<ManifestNode>,
}
