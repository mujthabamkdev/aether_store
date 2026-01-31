pub mod kernel;
pub mod guard;
pub mod loom;
pub use kernel::AetherKernel;
pub use guard::AetherGuard;
pub use loom::AetherLoom;

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

    pub fn persist_verified(&self, atom: &LogicAtom, guard: &AetherGuard) -> Result<String, Box<dyn std::error::Error>> {
        // If it's a financial op, verify 0% Riba
        if atom.op_code == 100 && !guard.verify_interest_free(extract_rate(&atom.data)) {
            return Err("Violation of Genesis Law: Riba Detected".into());
        }
        
        Ok(self.persist(atom)?)
    }
}

fn extract_rate(data: &[u8]) -> i32 {
    if data.len() < 4 { return 0; }
    let mut arr = [0u8; 4];
    arr.copy_from_slice(&data[0..4]);
    i32::from_le_bytes(arr)
}
