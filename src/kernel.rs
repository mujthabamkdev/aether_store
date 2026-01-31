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

    fn resolve_data(&self, atom: &LogicAtom) -> Result<Vec<u8>, KernelError> {
        // Lazy Load from Storage
        crate::read_blob(&atom.storage_ref)
            .map_err(|e| KernelError::Runtime(format!("Blob Fetch Error: {}", e)))
    }

    /// Fetches a node by hash and executed its logic (Legacy Sync)
    pub fn execute(&self, hash: &str) -> Result<i32, KernelError> {
        let atom = self.vault.fetch(hash).map_err(KernelError::Vault)?;
        let data = self.resolve_data(&atom)?;
        
        match atom.op_code {
            1 => {
                // ADD
                if data.len() < 8 { return Err(KernelError::Runtime("Invalid data length for ADD".into())); }
                let a = i32::from_le_bytes(data[0..4].try_into().unwrap());
                let b = i32::from_le_bytes(data[4..8].try_into().unwrap());
                Ok(a + b)
            },
            100 => Ok(0),
            _ => Err(KernelError::InvalidOpCode(atom.op_code)),
        }
    }

    pub fn execute_with_metrics(&self, hash: &str) -> Result<(i32, u128), KernelError> {
        let start = std::time::Instant::now();
        let result = self.execute(hash)?;
        let duration = start.elapsed().as_nanos();
        Ok((result, duration))
    }
    
    /// Smart Execution: recursive pipeline that returns JSON (Async)
    pub async fn execute_smart(&self, hash: &str) -> Result<serde_json::Value, KernelError> {
        let atom = self.vault.fetch(hash).map_err(KernelError::Vault)?;

        // Recursive: Execute dependencies in parallel (Async Resonance)
        let futures = atom.inputs.iter().map(|h| Box::pin(self.execute_smart(h)));
        let results = futures::future::join_all(futures).await;

        let mut input_results = Vec::new();
        for res in results {
            input_results.push(res?);
        }

        match atom.op_code {
            1 => { // ADD (Legacy wrapper)
                 Ok(serde_json::json!(0)) 
            },
            2 => { // FILTER
                // Input 0: The List
                // Data: The Filter Logic JSON
                if let Some(list) = input_results.get(0) {
                    if let Some(array) = list.as_array() {
                        let data = self.resolve_data(&atom)?;
                        let filter_config: serde_json::Value = serde_json::from_slice(&data)
                            .map_err(|e| KernelError::Runtime(e.to_string()))?;
                        let field = filter_config["field"].as_str().unwrap_or("");
                        let op = filter_config["op"].as_str().unwrap_or("");
                        let val_i = filter_config["val"].as_i64();
                        let val_s = filter_config["val"].as_str();

                        // Debug print
                        println!("[Kernel] Filtering {} items with {} {} {}", array.len(), field, op, val_s.unwrap_or("NUM"));

                        let filtered: Vec<_> = array.iter().filter(|item| {
                            match op {
                                ">" => item[field].as_i64().unwrap_or(0) > val_i.unwrap_or(0),
                                "<" => item[field].as_i64().unwrap_or(0) < val_i.unwrap_or(0),
                                "==" => item[field].as_str().unwrap_or("") == val_s.unwrap_or(""),
                                "!=" => item[field].as_str().unwrap_or("") != val_s.unwrap_or(""),
                                "contains" => item[field].as_str().unwrap_or("").contains(val_s.unwrap_or("")),
                                "not_contains" => !item[field].as_str().unwrap_or("").contains(val_s.unwrap_or("")),
                                _ => true
                            }
                        }).cloned().collect();
                        
                        return Ok(serde_json::Value::Array(filtered));
                    }
                }
                Ok(serde_json::json!([]))
            },
            100 => { // FINANCIAL / AUDIT (Identity)
                if let Some(res) = input_results.get(0) {
                    Ok(res.clone())
                } else {
                    Ok(serde_json::json!({"status": "Audited"}))
                }
            },
            500 => { // IO
                self.execute_io(hash).await
            },
            _ => Ok(serde_json::json!(null))
        }
    }

    pub async fn execute_io(&self, hash: &str) -> Result<serde_json::Value, KernelError> {
        let atom = self.vault.fetch(hash).map_err(KernelError::Vault)?;
        
        if atom.op_code == 500 {
            let data = self.resolve_data(&atom)?;
            let contract: crate::IOContract = serde_json::from_slice(&data)
                .map_err(|e| KernelError::Runtime(format!("IO Contract Parse Error: {}", e)))?;
            println!("[Kernel] Fetching IO: {}", contract.endpoint);
            
            let response = reqwest::get(&contract.endpoint).await
                .map_err(|e| KernelError::Runtime(format!("Network Error: {}", e)))?
                .json::<serde_json::Value>().await
                .map_err(|e| KernelError::Runtime(format!("JSON Parse Error: {}", e)))?;
                
            return Ok(response);
        }
        Err(KernelError::InvalidOpCode(atom.op_code))
    }
}
