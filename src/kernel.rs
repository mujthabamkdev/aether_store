use crate::{AetherVault, VaultError, LogicAtom};
use std::convert::TryInto;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum KernelError {
    #[error("Vault error: {0}")]
    Vault(#[from] VaultError),
    #[error("Runtime error: {0}")]
    Runtime(String),
    #[error("Invalid OpCode: {0}")]
    InvalidOpCode(u16),
}

pub struct AetherKernel {
    pub vault: AetherVault,
}

impl AetherKernel {
    pub fn new(vault: AetherVault) -> Self {
        Self { vault }
    }

    /// Fetches a node by hash and executes its logic
    pub fn execute(&self, hash: &str) -> Result<i32, KernelError> {
        let atom = self.vault.fetch(hash).map_err(KernelError::Vault)?;
        
        match atom.op_code {
            1 => {
                // ADD
                if atom.data.len() < 8 { return Err(KernelError::Runtime("Invalid data length for ADD".into())); }
                let a = i32::from_le_bytes(atom.data[0..4].try_into().unwrap());
                let b = i32::from_le_bytes(atom.data[4..8].try_into().unwrap());
                Ok(a + b)
            },
            100 => {
                // FINANCIAL (No-op for execution, it's a data node for the guard)
                Ok(0)
            },
            _ => Err(KernelError::InvalidOpCode(atom.op_code)),
        }
    }

    pub fn execute_with_metrics(&self, hash: &str) -> Result<(i32, u128), KernelError> {
        let start = std::time::Instant::now();
        let result = self.execute(hash)?;
        let duration = start.elapsed().as_nanos();
        Ok((result, duration))
    }

    pub async fn execute_io(&self, hash: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let atom = self.vault.fetch(hash).map_err(|e| format!("Vault error: {}", e))?;
        
        if atom.op_code == 500 {
            // 1. Decode the IOContract from atom.data
            let contract: crate::IOContract = serde_json::from_slice(&atom.data)?;

            // 2. Fetch the data
            let response = reqwest::get(&contract.endpoint).await?.json::<serde_json::Value>().await?;

            // 3. Validate against the Schema (The 'Guard' of the outside world)
            let compiled_schema = jsonschema::validator_for(&contract.schema)?;
            if !compiled_schema.is_valid(&response) {
                return Err("External data violated the logical schema contract".into());
            }

            return Ok(response);
        }
        Err("Not an I/O Atom (OpCode 500)".into())
    }
}
