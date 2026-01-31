use crate::AetherVault;

pub struct AetherKernel {
    pub vault: AetherVault,
}

impl AetherKernel {
    pub fn new(vault: AetherVault) -> Self {
        Self { vault }
    }

    /// Fetches a node by hash and executes its logic
    pub fn execute(&self, hash: &str) -> Result<i32, Box<dyn std::error::Error>> {
        let atom = self.vault.fetch(hash)?;
        
        match atom.op_code {
            // OpCode 1: Simple Addition
            1 => {
                // For now, we assume 'data' contains two i32 values
                if atom.data.len() < 8 {
                    return Err("Insufficient data for ADD operation".into());
                }
                
                let a = i32::from_le_bytes(atom.data[0..4].try_into()?);
                let b = i32::from_le_bytes(atom.data[4..8].try_into()?);
                Ok(a + b)
            }
            _ => Err(format!("Unknown OpCode: {}", atom.op_code).into()),
        }
    }
}
