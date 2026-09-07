pub mod storage;
pub mod kernel;
pub mod guard;
pub mod loom;
pub mod manifest;
pub mod orchestrator;
pub mod optimizer;
pub mod io;
pub mod product;
pub mod auth;
pub mod paths;
pub mod ssrf;
pub mod sanitize;
pub mod ratelimit;

pub use storage::{write_blob, read_blob};
pub use kernel::AetherKernel;
pub use guard::AetherGuard;
pub use loom::AetherLoom;
pub use manifest::AetherManifest;
pub use product::{ProductTemplate, InputSchema};
pub use orchestrator::AetherOrchestrator;
pub use optimizer::AetherOptimizer;
pub use io::IOContract;
pub use paths::Paths;
pub use ratelimit::{RateLimiter, Limits};
pub use ssrf::{check_endpoint, SsrfVerdict};

pub const OP_PERMISSION: u16 = 10;
pub const OP_GATEWAY: u16 = 800;

use sled::Db;
use blake3::Hasher;
use thiserror::Error;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IdentityAtom {
    pub public_key: String,
    pub role: String,
    pub org_hash: String,
    pub access_nodes: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ProjectStatus {
    Building,
    Active,
    Archived,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProjectAtom {
    pub name: String,
    pub root_hash: String,
    pub org_hash: String,
    pub status: ProjectStatus,
    pub created_at: u64,
}

#[derive(Error, Debug)]
pub enum VaultError {
    #[error("Storage failure: {0}")]
    Storage(#[from] sled::Error),
    #[error("Logic node not found")]
    NotFound,
    #[error("Identity not found")]
    IdentityNotFound,
    #[error("Validation failed: {0}")]
    Validation(String),
}

/// Engine-wide shared state. Created once in main() and threaded into axum
/// via Router::with_state.
pub struct AppState {
    pub vault: AetherVault,
    pub paths: Paths,
    pub api_key: String,
    pub http: reqwest::Client,
    pub chat_limiter: RateLimiter,
    pub weave_limiter: RateLimiter,
    pub limits: Limits,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LogicAtom {
    pub op_code: u16,
    pub inputs: Vec<String>,
    pub storage_ref: String,
    #[serde(default = "default_context")]
    pub context_id: String,
}

fn default_context() -> String {
    "global".to_string()
}

#[derive(Clone)]
pub struct AetherVault {
    db: Db,
}

impl AetherVault {
    pub fn new(path: &str) -> Result<Self, VaultError> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    pub fn persist(&self, atom: &LogicAtom) -> Result<String, VaultError> {
        let serialized = serde_json::to_vec(atom)
            .map_err(|e| VaultError::Validation(format!("serialize: {}", e)))?;
        let mut hasher = Hasher::new();
        hasher.update(&serialized);
        let hash = hasher.finalize().to_hex().to_string();
        self.db.insert(hash.as_bytes(), serialized)?;
        Ok(hash)
    }

    pub fn persist_batch(&self, atoms: Vec<LogicAtom>) -> Result<String, VaultError> {
        let mut hashes = Vec::new();
        for atom in &atoms {
            hashes.push(self.persist(atom)?);
        }
        let mut current_level = hashes;
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in current_level.chunks(2) {
                let mut hasher = Hasher::new();
                hasher.update(chunk[0].as_bytes());
                if chunk.len() > 1 {
                    hasher.update(chunk[1].as_bytes());
                } else {
                    hasher.update(chunk[0].as_bytes());
                }
                next_level.push(hasher.finalize().to_hex().to_string());
            }
            current_level = next_level;
        }
        Ok(current_level[0].clone())
    }

    pub fn fetch(&self, hash: &str) -> Result<LogicAtom, VaultError> {
        match self.db.get(hash.as_bytes())? {
            Some(data) => serde_json::from_slice(&data)
                .map_err(|e| VaultError::Validation(format!("decode: {}", e))),
            None => Err(VaultError::NotFound),
        }
    }

    pub fn persist_verified(&self, atom: &LogicAtom, guard: &AetherGuard) -> Result<String, VaultError> {
        let blob = storage::read_blob(&atom.storage_ref)
            .map_err(|e| VaultError::Validation(format!("Blob Load Error: {}", e)))?;

        if atom.op_code == 100 {
            if !guard.verify_interest_free(extract_rate(&blob)) {
                return Err(VaultError::Validation("Violation of Genesis Law: Riba Detected".to_string()));
            }
        }

        if atom.op_code == 500 {
            match serde_json::from_slice::<crate::IOContract>(&blob) {
                Ok(contract) => {
                    if !guard.verify_sovereignty(&contract.endpoint, contract.sensitivity) {
                        return Err(VaultError::Validation(
                            "Violation of Sovereignty Law: Sovereign data must stay in .my or localhost".to_string(),
                        ));
                    }
                }
                Err(_) => {
                    return Err(VaultError::Validation("Invalid IO Contract data".to_string()));
                }
            }
        }

        let mut input_atoms = Vec::new();
        for input_hash in &atom.inputs {
            match self.fetch(input_hash) {
                Ok(input_atom) => {
                    if input_atom.context_id != "global" && input_atom.context_id != atom.context_id {
                        return Err(VaultError::Validation(format!(
                            "Context Isolation Violation: Atom '{}' ({}) from '{}' cannot depend on Atom ({}) from '{}'",
                            atom.op_code, atom.context_id, atom.context_id, input_atom.op_code, input_atom.context_id
                        )));
                    }
                    input_atoms.push(input_atom);
                }
                Err(_) => {
                    return Err(VaultError::Validation(format!("Missing Dependency: {}", input_hash)));
                }
            }
        }

        guard.verify_compatibility(atom, &input_atoms)
            .map_err(|e: anyhow::Error| VaultError::Validation(e.to_string()))?;

        let data = serde_json::to_vec(atom)
            .map_err(|e| VaultError::Validation(format!("serialize: {}", e)))?;
        let hash = blake3::hash(&data).to_string();
        self.db.insert(hash.as_bytes(), data)?;
        Ok(hash)
    }

    pub fn persist_identity(&self, identity: &IdentityAtom) -> Result<String, VaultError> {
        let serialized = serde_json::to_vec(identity)
            .map_err(|e| VaultError::Validation(format!("serialize: {}", e)))?;
        let hash = blake3::hash(identity.public_key.as_bytes()).to_string();
        self.db.insert(format!("ID:{}", hash).as_bytes(), serialized)?;
        Ok(hash)
    }

    pub fn fetch_identity(&self, hash: &str) -> Result<IdentityAtom, VaultError> {
        match self.db.get(format!("ID:{}", hash).as_bytes())? {
            Some(data) => serde_json::from_slice(&data)
                .map_err(|e| VaultError::Validation(format!("decode: {}", e))),
            None => Err(VaultError::IdentityNotFound),
        }
    }

    pub fn persist_project(&self, project: &ProjectAtom) -> Result<String, VaultError> {
        let serialized = serde_json::to_vec(project)
            .map_err(|e| VaultError::Validation(format!("serialize: {}", e)))?;
        let key = format!("PROJ:{}", project.name);
        self.db.insert(key.as_bytes(), serialized)?;
        Ok(project.name.clone())
    }

    /// List projects filtered by org. Empty org_hash means "global" listing.
    pub fn list_projects(&self, org_hash: Option<&str>) -> Result<Vec<ProjectAtom>, VaultError> {
        let mut projects = Vec::new();
        let prefix = "PROJ:";
        for item in self.db.scan_prefix(prefix) {
            if let Ok((_, value)) = item {
                if let Ok(proj) = serde_json::from_slice::<ProjectAtom>(&value) {
                    if let Some(org) = org_hash {
                        if proj.org_hash != org {
                            continue;
                        }
                    }
                    projects.push(proj);
                }
            }
        }
        Ok(projects)
    }

    pub fn get_project(&self, name: &str) -> Result<ProjectAtom, VaultError> {
        let key = format!("PROJ:{}", name);
        match self.db.get(key.as_bytes())? {
            Some(data) => serde_json::from_slice(&data)
                .map_err(|e| VaultError::Validation(format!("decode: {}", e))),
            None => Err(VaultError::NotFound),
        }
    }

    pub fn update_project_status(&self, name: &str, status: ProjectStatus) -> Result<(), VaultError> {
        let mut proj = self.get_project(name)?;
        proj.status = status;
        self.persist_project(&proj)?;
        Ok(())
    }

    pub fn update_project_hash(&self, name: &str, hash: &str) -> Result<(), VaultError> {
        let mut proj = self.get_project(name)?;
        proj.root_hash = hash.to_string();
        self.persist_project(&proj)?;
        Ok(())
    }

    pub fn verify_resonance(&self, user_hash: &str, project_hash: &str) -> bool {
        if let Ok(identity) = self.fetch_identity(user_hash) {
            for permission_hash in &identity.access_nodes {
                if let Ok(perm_node) = self.fetch(permission_hash) {
                    if perm_node.op_code == OP_PERMISSION {
                        if perm_node.inputs.contains(&project_hash.to_string()) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Paginated inventory. Returns at most `limit` items starting at `offset`.
    pub fn inventory(&self, limit: usize, offset: usize) -> Vec<serde_json::Value> {
        let mut atoms = Vec::new();
        let mut skipped = 0usize;
        for item in self.db.iter() {
            if let Ok((key, value)) = item {
                let key_str = String::from_utf8_lossy(&key).to_string();
                if key_str.starts_with("ID:") || key_str.starts_with("PROJ:") {
                    continue;
                }
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                if atoms.len() >= limit {
                    break;
                }
                if let Ok(atom) = serde_json::from_slice::<LogicAtom>(&value) {
                    atoms.push(serde_json::json!({
                        "hash": key_str,
                        "op_code": atom.op_code,
                        "context_id": atom.context_id,
                    }));
                }
            }
        }
        atoms
    }

    pub fn inject_atom(&self, atom: &LogicAtom) -> Result<String, VaultError> {
        let blob = serde_json::to_vec(atom)
            .map_err(|e| VaultError::Validation(format!("serialize: {}", e)))?;
        let hash = blake3::hash(&blob).to_hex().to_string();
        self.db.insert(hash.as_bytes(), blob)?;
        Ok(hash)
    }

    pub fn export_graph_json(&self) -> serde_json::Value {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for item in self.db.iter() {
            if let Ok((key, value)) = item {
                let key_str = String::from_utf8_lossy(&key).to_string();
                if key_str.starts_with("ID:") {
                    if let Ok(identity) = serde_json::from_slice::<IdentityAtom>(&value) {
                        let id_hash = key_str.replace("ID:", "");
                        nodes.push(serde_json::json!({
                            "data": { "id": id_hash, "label": format!("User:{}", identity.role), "type": "identity" }
                        }));
                        for access in identity.access_nodes {
                            edges.push(serde_json::json!({
                                "data": { "source": id_hash, "target": access, "label": "owns_access" }
                            }));
                        }
                    }
                } else if !key_str.starts_with("PROJ:") {
                    if let Ok(atom) = serde_json::from_slice::<LogicAtom>(&value) {
                        nodes.push(serde_json::json!({
                            "data": { "id": key_str, "label": format!("Op:{}", atom.op_code), "type": "logic" }
                        }));
                        for input_hash in atom.inputs {
                            edges.push(serde_json::json!({
                                "data": { "source": input_hash, "target": key_str }
                            }));
                        }
                    }
                }
            }
        }
        serde_json::json!({ "nodes": nodes, "edges": edges })
    }

    pub fn export_graph_viz(&self) -> String {
        let mut dot = String::from("digraph AetherLogic {\n");
        for item in self.db.iter() {
            if let Ok((key, value)) = item {
                let key_str = String::from_utf8_lossy(&key).to_string();
                if key_str.starts_with("ID:") {
                    let short_hash = &key_str[3..11.min(key_str.len())];
                    dot.push_str(&format!("    \"{}\" [label=\"Identity\\n{}\" shape=box];\n", key_str, short_hash));
                } else if !key_str.starts_with("PROJ:") {
                    let hash = key_str;
                    let short_hash = &hash[0..8.min(hash.len())];
                    if let Ok(atom) = serde_json::from_slice::<LogicAtom>(&value) {
                        dot.push_str(&format!("    \"{}\" [label=\"Op:{}\\n{}\"];\n", hash, atom.op_code, short_hash));
                        for input_hash in atom.inputs {
                            dot.push_str(&format!("    \"{}\" -> \"{}\";\n", input_hash, hash));
                        }
                    }
                }
            }
        }
        dot.push_str("}");
        dot
    }
}

fn extract_rate(data: &[u8]) -> i32 {
    if data.len() < 4 { return 0; }
    let mut arr = [0u8; 4];
    arr.copy_from_slice(&data[0..4]);
    i32::from_le_bytes(arr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_manifest_parse() {
        let content = match fs::read_to_string("../../products/transit-home/manifest.yaml") {
            Ok(c) => c,
            Err(_) => return, // Skip if CWD is wrong (CI / cargo run --release).
        };
        if let Err(e) = serde_yaml::from_str::<AetherManifest>(&content) {
            panic!("FAIL: {}", e);
        }
    }

    #[test]
    fn test_ssrf_blocks_metadata() {
        assert!(matches!(check_endpoint("http://169.254.169.254/latest/meta-data/", 0), SsrfVerdict::Deny(_)));
        assert!(matches!(check_endpoint("http://10.0.0.5/x", 0), SsrfVerdict::Deny(_)));
        assert!(matches!(check_endpoint("http://localhost:8080/x", 2), SsrfVerdict::Allow));
        assert!(matches!(check_endpoint("http://api.iproperty.com.my/x", 2), SsrfVerdict::Allow));
        assert!(matches!(check_endpoint("http://api.iproperty.com.my/x", 0), SsrfVerdict::Allow));
        assert!(matches!(check_endpoint("ftp://example.com/", 0), SsrfVerdict::Deny(_)));
    }

    #[test]
    fn test_ssrf_loopback_sovereignty() {
        // Sovereign (sensitivity 2) data may live on loopback.
        assert!(matches!(check_endpoint("http://127.0.0.1:8080/kl/properties", 2), SsrfVerdict::Allow));
        assert!(matches!(check_endpoint("http://[::1]:8080/x", 2), SsrfVerdict::Allow));
        // Non-sovereign fetches are still SSRF-guarded against loopback.
        assert!(matches!(check_endpoint("http://127.0.0.1:8080/x", 0), SsrfVerdict::Deny(_)));
        // Sovereign data on a private (non-loopback) network is still blocked.
        assert!(matches!(check_endpoint("http://10.0.0.5/x", 2), SsrfVerdict::Deny(_)));
    }
}
