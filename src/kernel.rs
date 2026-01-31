use crate::AetherVault;
use std::convert::TryInto;

pub struct AetherKernel {
    pub vault: AetherVault,
}

impl AetherKernel {
    pub fn new(vault: AetherVault) -> Self {
        Self { vault }
    }

    /// Fetches a node by hash and executes its logic
    pub fn execute(&self, hash: &str) -> Result<i32, Box<dyn std::error::Error>> {
        // Step 1: Hydrate logic from the warehouse
        let atom = self.vault.fetch(hash)?;
        
        match atom.op_code {
            // OpCode 1: i32 Addition
            1 => {
                if atom.data.len() < 8 {
                    return Err("Insufficient data for ADD operation".into());
                }
                
                // Decode Little-Endian bytes into integers
                let a = i32::from_le_bytes(atom.data[0..4].try_into()?);
                let b = i32::from_le_bytes(atom.data[4..8].try_into()?);
                
                Ok(a + b)
            }
            _ => Err(format!("Unsupported OpCode: {}", atom.op_code).into()),
        }
    }
}
