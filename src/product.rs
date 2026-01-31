use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InputSchema {
    pub name: String,       // Variable name in template (e.g. "station_type")
    pub label: String,      // Human readable label (e.g. "Station Type")
    pub input_type: String, // "text", "select", "number"
    pub options: Option<Vec<String>>, // For select
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProductTemplate {
    pub id: String,
    pub name: String,
    pub manifest_template: String, // YAML with {{variable}} placeholders
    pub inputs: Vec<InputSchema>,
}
