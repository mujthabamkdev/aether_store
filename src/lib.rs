pub mod kernel;
pub mod guard;
pub mod loom;
pub mod manifest;
pub mod orchestrator;
pub use kernel::AetherKernel;
pub use guard::AetherGuard;
pub use loom::AetherLoom;
pub use manifest::AetherManifest;
pub use orchestrator::AetherOrchestrator;

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
    pub data: Vec<u8>,       // Constants or static parameters
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

    /// Retrieves an atom by its identity hash
    pub fn fetch(&self, hash: &str) -> Result<LogicAtom, VaultError> {
        match self.db.get(hash.as_bytes())? {
            Some(data) => Ok(serde_json::from_slice(&data).unwrap()),
            None => Err(VaultError::NotFound),
        }
    }

    pub fn persist_verified(&self, atom: &LogicAtom, guard: &AetherGuard) -> Result<String, VaultError> {
        // If it's a financial op, verify 0% Riba
        if atom.op_code == 100 && !guard.verify_interest_free(extract_rate(&atom.data)) {
            return Err(VaultError::Validation("Violation of Genesis Law: Riba Detected".to_string()));
        }
        
        Ok(self.persist(atom)?)
    }

    pub fn export_graph_json(&self) -> serde_json::Value {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for item in self.db.iter() {
            if let Ok((key, value)) = item {
                // Key is bytes, convert to hex string (or utf8 if it was persisted as utf8? Persist used .as_bytes() on utf8 string of hex)
                // In persist: hash.as_bytes(). Hash is a string of hex. So this is valid utf8.
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
