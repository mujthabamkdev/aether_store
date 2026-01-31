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
}
