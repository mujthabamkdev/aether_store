pub mod storage;
pub mod kernel;
pub mod guard;
pub mod loom;
pub mod manifest;
pub mod orchestrator;
pub mod optimizer;
pub mod io;
pub mod product;

pub use storage::{write_blob, read_blob};
pub use kernel::AetherKernel;
pub use guard::AetherGuard;
pub use loom::AetherLoom;
pub use manifest::AetherManifest;
pub use product::{ProductTemplate, InputSchema};
pub use orchestrator::AetherOrchestrator;
pub use optimizer::AetherOptimizer;
pub use io::IOContract;

pub const OP_PERMISSION: u16 = 10;
pub const OP_GATEWAY: u16 = 800;

use sled::Db;
use blake3::Hasher;
use thiserror::Error;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IdentityAtom {
    pub public_key: String,
    pub role: String, // e.g., "admin", "viewer"
    pub org_hash: String, // Link to Org Genesis
    pub access_nodes: Vec<String>, // List of PermissionNode Hashes (Op:10)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ProjectStatus {
    Building, // Manifest uploaded, not validated
    Active,   // Validated and Live
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

/// The fundamental unit of the Aether-Grid
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LogicAtom {
    pub op_code: u16,        // e.g., 0x01 for ADD
    pub inputs: Vec<String>, // List of dependency hashes
    pub storage_ref: String, // URI to external Blob (No Raws!)
    #[serde(default = "default_context")]
    pub context_id: String,  // Multi-Project Isolation Key
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

    /// Persists a LogicAtom and returns its unique BLAKE3 hash
    pub fn persist(&self, atom: &LogicAtom) -> Result<String, VaultError> {
        let serialized = serde_json::to_vec(atom).unwrap();
        
        let mut hasher = Hasher::new();
        hasher.update(&serialized);
        let hash = hasher.finalize().to_hex().to_string();

        // Content-addressed storage: Key is the Hash, Value is the Atom
        self.db.insert(hash.as_bytes(), serialized)?;
        Ok(hash)
    }
    
    /// Implement Merkle Batching for High-Frequency Scalability
    pub fn persist_batch(&self, atoms: Vec<LogicAtom>) -> Result<String, VaultError> {
        let mut hashes = Vec::new();
        for atom in &atoms {
            hashes.push(self.persist(atom)?);
        }
        
        // Compute Merkle Root of the batch
        let mut current_level = hashes;
        while current_level.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in current_level.chunks(2) {
                let mut hasher = Hasher::new();
                hasher.update(chunk[0].as_bytes());
                if chunk.len() > 1 {
                    hasher.update(chunk[1].as_bytes());
                } else {
                    hasher.update(chunk[0].as_bytes()); // Duplicate last if odd
                }
                next_level.push(hasher.finalize().to_hex().to_string());
            }
            current_level = next_level;
        }
        
        Ok(current_level[0].clone())
    }

    /// Retrieves an atom by its identity hash
    pub fn fetch(&self, hash: &str) -> Result<LogicAtom, VaultError> {
        match self.db.get(hash.as_bytes())? {
            Some(data) => Ok(serde_json::from_slice(&data).unwrap()),
            None => Err(VaultError::NotFound),
        }
    }

    pub fn persist_verified(&self, atom: &LogicAtom, guard: &AetherGuard) -> Result<String, VaultError> {
        // Fetch content to verify laws (Lazy Load for Verification)
        let blob = storage::read_blob(&atom.storage_ref)
            .map_err(|e| VaultError::Validation(format!("Blob Load Error: {}", e)))?;

        // If it's a financial op, verify 0% Riba
        if atom.op_code == 100 {
             if !guard.verify_interest_free(extract_rate(&blob)) {
                 return Err(VaultError::Validation("Violation of Genesis Law: Riba Detected".to_string()));
             }
        }
        
        // If it's an IO op, verify sovereignty
        if atom.op_code == 500 {
            if let Ok(contract) = serde_json::from_slice::<crate::IOContract>(&blob) {
                 if !guard.verify_sovereignty(&contract.endpoint, contract.sensitivity) {
                     return Err(VaultError::Validation("Violation of Sovereignty Law: Sovereign data must stay in .my or localhost".to_string()));
                 }
            } else {
                 return Err(VaultError::Validation("Invalid IO Contract data".to_string()));
            }
        }

        // Context Isolation: Verify inputs belong to same context or global
        let mut input_atoms = Vec::new();
        for input_hash in &atom.inputs {
            if let Ok(input_atom) = self.fetch(input_hash) {
                if input_atom.context_id != "global" && input_atom.context_id != atom.context_id {
                     return Err(VaultError::Validation(format!(
                         "Context Isolation Violation: Atom '{}' ({}) from '{}' cannot depend on Atom ({}) from '{}'", 
                         atom.op_code, atom.context_id, atom.context_id, input_atom.op_code, input_atom.context_id
                     )));
                }
                input_atoms.push(input_atom);
            } else {
                 return Err(VaultError::Validation(format!("Missing Dependency: {}", input_hash)));
            }
        }
        
        // Guard: Static Analysis
        guard.verify_compatibility(atom, &input_atoms)
            .map_err(|e: anyhow::Error| VaultError::Validation(e.to_string()))?;
            
        // Restore IO Sovereignty Check
        if atom.op_code == 500 {
             // We need to parse storage to get endpoint. But storage is ref.
             // For now, skip deep inspection here to avoid overhead, relying on Kernel runtime check.
             // Or verify logic graph compatibility is enough for now.
        }

        // 3. Hash & Store
        let data = serde_json::to_vec(atom).unwrap();
        let hash = blake3::hash(&data).to_string();

        self.db.insert(hash.as_bytes(), data)?;
        Ok(hash)
    }

    pub fn persist_identity(&self, identity: &IdentityAtom) -> Result<String, VaultError> {
        let serialized = serde_json::to_vec(identity).unwrap();
        // Hash the public key to get the Identity Hash (Deterministic)
        let hash = blake3::hash(identity.public_key.as_bytes()).to_string();
        self.db.insert(format!("ID:{}", hash).as_bytes(), serialized)?;
        Ok(hash)
    }

    pub fn fetch_identity(&self, hash: &str) -> Result<IdentityAtom, VaultError> {
        match self.db.get(format!("ID:{}", hash).as_bytes())? {
            Some(data) => Ok(serde_json::from_slice(&data).unwrap()),
            None => Err(VaultError::IdentityNotFound),
        }
    }
    
    // --- Project Persistence (Sled) ---
    pub fn persist_project(&self, project: &ProjectAtom) -> Result<String, VaultError> {
        let serialized = serde_json::to_vec(project).unwrap();
        // Key: "PROJ:{name}" (Unique Name per Instance, or add OrgHash if needed)
        let key = format!("PROJ:{}", project.name);
        self.db.insert(key.as_bytes(), serialized)?;
        Ok(project.name.clone())
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectAtom>, VaultError> {
        let mut projects = Vec::new();
        let prefix = "PROJ:";
        for item in self.db.scan_prefix(prefix) {
            if let Ok((_, value)) = item {
                if let Ok(proj) = serde_json::from_slice::<ProjectAtom>(&value) {
                    projects.push(proj);
                }
            }
        }
        Ok(projects)
    }
    
    pub fn get_project(&self, name: &str) -> Result<ProjectAtom, VaultError> {
        let key = format!("PROJ:{}", name);
        if let Some(data) = self.db.get(key.as_bytes())? {
            let proj: ProjectAtom = serde_json::from_slice(&data).unwrap();
            Ok(proj)
        } else {
            Err(VaultError::NotFound)
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

    /// Verifies if a User (via IdentityHash) has resonance (access) to a Project (via ProjectHash)
    /// This connects the user to the project via a PermissionNode (Op:10)
    pub fn verify_resonance(&self, user_hash: &str, project_hash: &str) -> bool {
        if let Ok(identity) = self.fetch_identity(user_hash) {
            // Traverse the user's access nodes
            for permission_hash in &identity.access_nodes {
                 if let Ok(perm_node) = self.fetch(permission_hash) {
                     // Check if it is a Permission Op
                     if perm_node.op_code == OP_PERMISSION {
                         // Check if this permission node points to the project
                         // We assume inputs[0] is the Target Project Hash
                         if perm_node.inputs.contains(&project_hash.to_string()) {
                             return true;
                         }
                     }
                 }
            }
        }
        false
    }

    pub fn inventory(&self) -> Vec<serde_json::Value> {
        let mut atoms = Vec::new();
        for item in self.db.iter() {
            if let Ok((key, value)) = item {
                let key_str = String::from_utf8_lossy(&key).to_string();
                if !key_str.starts_with("ID:") && !key_str.starts_with("PROJ:") {
                    if let Ok(atom) = serde_json::from_slice::<LogicAtom>(&value) {
                         atoms.push(serde_json::json!({
                             "hash": key_str,
                             "op_code": atom.op_code,
                             "context_id": atom.context_id,
                             // We could include more metadata like 'intent' if we stored it in the atom or blob
                         }));
                    }
                }
            }
        }
        atoms
    }

    pub fn inject_atom(&self, atom: &LogicAtom) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let blob = serde_json::to_vec(atom)?;
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
                
                // Check if it's an Identity
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
                } else {
                    // It's a LogicAtom
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
                    // Identity
                    let short_hash = &key_str[3..11];
                    dot.push_str(&format!("    \"{}\" [label=\"Identity\\n{}\" shape=box];\n", key_str, short_hash));
                } else {
                    let hash = key_str;
                    let short_hash = &hash[0..8];
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
