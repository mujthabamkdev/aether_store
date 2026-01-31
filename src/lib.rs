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

use sled::Db;
use blake3::Hasher;
use thiserror::Error;
use serde::{Serialize, Deserialize};

#[derive(Error, Debug)]
pub enum VaultError {
    #[error("Storage failure: {0}")]
    Storage(#[from] sled::Error),
    #[error("Logic node not found")]
    NotFound,
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
        for input_hash in &atom.inputs {
            if let Ok(input_atom) = self.fetch(input_hash) {
                if input_atom.context_id != "global" && input_atom.context_id != atom.context_id {
                     return Err(VaultError::Validation(format!(
                         "Context Isolation Violation: Atom '{}' ({}) cannot depend on Atom ({}) from different context '{}'", 
                         atom.context_id, atom.op_code, input_atom.context_id, input_atom.context_id
                     )));
                }
            }
        }
        
        Ok(self.persist(atom)?)
    }

    pub fn export_graph_json(&self) -> serde_json::Value {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for item in self.db.iter() {
            if let Ok((key, value)) = item {
                // Key is bytes, convert to hex string
                let hash = String::from_utf8_lossy(&key).to_string();
                
                if let Ok(atom) = serde_json::from_slice::<LogicAtom>(&value) {
                    // Add Node
                    nodes.push(serde_json::json!({
                        "data": { "id": hash, "label": format!("Op:{}", atom.op_code) }
                    }));

                    // Add Edges
                    for input_hash in atom.inputs {
                        edges.push(serde_json::json!({
                            "data": { "source": input_hash, "target": hash }
                        }));
                    }
                }
            }
        }

        serde_json::json!({
            "nodes": nodes,
            "edges": edges
        })
    }
}

fn extract_rate(data: &[u8]) -> i32 {
    if data.len() < 4 { return 0; }
    let mut arr = [0u8; 4];
    arr.copy_from_slice(&data[0..4]);
    i32::from_le_bytes(arr)
}
