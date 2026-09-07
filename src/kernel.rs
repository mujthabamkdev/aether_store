use crate::{AetherVault, VaultError, LogicAtom, SsrfVerdict, check_endpoint};
use std::convert::TryInto;
use thiserror::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

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
        crate::read_blob(&atom.storage_ref)
            .map_err(|e| KernelError::Runtime(format!("Blob Fetch Error: {}", e)))
    }

    pub fn execute(&self, hash: &str) -> Result<i32, KernelError> {
        let atom = self.vault.fetch(hash).map_err(KernelError::Vault)?;
        let data = self.resolve_data(&atom)?;
        match atom.op_code {
            1 => {
                if data.len() < 8 { return Err(KernelError::Runtime("Invalid data length for ADD".into())); }
                let a = i32::from_le_bytes(data[0..4].try_into().unwrap());
                let b = i32::from_le_bytes(data[4..8].try_into().unwrap());
                Ok(a + b)
            }
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

    pub async fn execute_smart(&self, hash: &str) -> Result<serde_json::Value, KernelError> {
        // Arc-clone the vault to break the &self lifetime so we can Box::pin
        // recursively without lifetime gymnastics.
        let kernel = Arc::new(AetherKernel::new(self.vault.clone()));
        execute_recursive(kernel, hash.to_string(), 0, 32).await
    }
}

fn execute_recursive(
    kernel: Arc<AetherKernel>,
    hash: String,
    depth: usize,
    max_depth: usize,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, KernelError>> + Send>> {
    Box::pin(async move {
        if depth > max_depth {
            return Err(KernelError::Runtime(format!("execution depth {} exceeded cap {}", depth, max_depth)));
        }
        let atom = kernel.vault.fetch(&hash).map_err(KernelError::Vault)?;

        let mut futs = Vec::with_capacity(atom.inputs.len());
        for h in atom.inputs.clone() {
            let k = kernel.clone();
            futs.push(execute_recursive(k, h, depth + 1, max_depth));
        }
        let results = futures::future::join_all(futs).await;
        let mut input_results = Vec::with_capacity(results.len());
        for res in results { input_results.push(res?); }

        match atom.op_code {
            1 => Ok(serde_json::json!(0)),
            2 => {
                if let Some(list) = input_results.get(0) {
                    if let Some(array) = list.as_array() {
                        let data = kernel.resolve_data(&atom)?;
                        let cfg: serde_json::Value = serde_json::from_slice(&data).map_err(|e| KernelError::Runtime(e.to_string()))?;
                        let field = cfg["field"].as_str().unwrap_or("");
                        let op = cfg["op"].as_str().unwrap_or("");
                        let val_i = cfg["val"].as_i64();
                        let val_s = cfg["val"].as_str();
                        let filtered: Vec<_> = array.iter().filter(|item| match op {
                            ">" => item[field].as_i64().unwrap_or(0) > val_i.unwrap_or(0),
                            "<" => item[field].as_i64().unwrap_or(0) < val_i.unwrap_or(0),
                            "==" => {
                                let val = val_s.unwrap_or("");
                                if val == "All" { true } else { item[field].as_str().unwrap_or("") == val }
                            }
                            "!=" => item[field].as_str().unwrap_or("") != val_s.unwrap_or(""),
                            "contains" => item[field].as_str().unwrap_or("").contains(val_s.unwrap_or("")),
                            "not_contains" => !item[field].as_str().unwrap_or("").contains(val_s.unwrap_or("")),
                            _ => true,
                        }).cloned().collect();
                        return Ok(serde_json::Value::Array(filtered));
                    }
                }
                Ok(serde_json::json!([]))
            }
            3 => {
                let mut merged = Vec::new();
                for r in input_results { if let Some(a) = r.as_array() { merged.extend(a.clone()); } }
                Ok(serde_json::Value::Array(merged))
            }
            4 => {
                if let Some(r) = input_results.get(0) {
                    if let Some(arr) = r.as_array() {
                        let data = kernel.resolve_data(&atom)?;
                        let cfg: serde_json::Value = serde_json::from_slice(&data).map_err(|e| KernelError::Runtime(e.to_string()))?;
                        let field = cfg["field"].as_str().unwrap_or("price");
                        let order = cfg["order"].as_str().unwrap_or("asc");
                        let mut v = arr.clone();
                        v.sort_by(|a, b| {
                            let va = a[field].as_i64().unwrap_or(0);
                            let vb = b[field].as_i64().unwrap_or(0);
                            if order == "desc" { vb.cmp(&va) } else { va.cmp(&vb) }
                        });
                        Ok(serde_json::Value::Array(v))
                    } else { Ok(serde_json::json!([])) }
                } else { Ok(serde_json::json!([])) }
            }
            5 => {
                if let Some(r) = input_results.get(0) {
                    if let Some(arr) = r.as_array() {
                        let data = kernel.resolve_data(&atom)?;
                        let cfg: serde_json::Value = serde_json::from_slice(&data).map_err(|e| KernelError::Runtime(e.to_string()))?;
                        let mode = cfg["mode"].as_str().unwrap_or("min");
                        let field = cfg["field"].as_str().unwrap_or("price");
                        let mut tv = if mode == "min" { i64::MAX } else { i64::MIN };
                        for item in arr {
                            let v = item[field].as_i64().unwrap_or(0);
                            if mode == "min" { if v < tv { tv = v; } } else { if v > tv { tv = v; } }
                        }
                        let hl: Vec<serde_json::Value> = arr.iter().map(|item| {
                            let mut obj = item.as_object().unwrap().clone();
                            let v = item[field].as_i64().unwrap_or(0);
                            if v == tv { obj.insert("_highlighted".to_string(), serde_json::json!(true)); }
                            serde_json::Value::Object(obj)
                        }).collect();
                        Ok(serde_json::Value::Array(hl))
                    } else { Ok(serde_json::json!([])) }
                } else { Ok(serde_json::json!([])) }
            }
            6 => {
                if let Some(base) = input_results.get(0) {
                    if let Some(arr) = base.as_array() {
                        let mut enriched = arr.clone();
                        if let Some(extra) = input_results.get(1) {
                            if let Some(obj) = extra.as_object() {
                                for item in enriched.iter_mut() {
                                    if let Some(o) = item.as_object_mut() {
                                        for (k, v) in obj { o.insert(k.clone(), v.clone()); }
                                    }
                                }
                            }
                        }
                        Ok(serde_json::Value::Array(enriched))
                    } else { Ok(base.clone()) }
                } else { Ok(serde_json::json!([])) }
            }
            7 => Ok(input_results.get(0).cloned().unwrap_or_else(|| serde_json::json!([]))),
            50 => {
                let data = kernel.resolve_data(&atom)?;
                let cfg: serde_json::Value = serde_json::from_slice(&data).map_err(|e| KernelError::Runtime(e.to_string()))?;
                Ok(cfg)
            }
            100 => Ok(input_results.get(0).cloned().unwrap_or_else(|| serde_json::json!({"status": "Audited"}))),
            500 => execute_io(kernel, &hash).await,
            800 => Ok(if let Some(r) = input_results.get(0) {
                serde_json::json!({"origin": "0xSOVEREIGN_ROOT", "payload": r, "masked_fields": ["private_logic_trace"]})
            } else {
                serde_json::json!({"error": "Gateway has no input resonance"})
            }),
            600 => {
                let data = kernel.resolve_data(&atom)?;
                let intent = String::from_utf8_lossy(&data).to_string();
                Ok(serde_json::json!({"status": "SYNTHESIS_PENDING", "intent": intent, "hash": hash, "type": "Logic Gap"}))
            }
            _ => Ok(serde_json::json!(null)),
        }
    })
}

async fn execute_io(kernel: Arc<AetherKernel>, hash: &str) -> Result<serde_json::Value, KernelError> {
    let atom = kernel.vault.fetch(hash).map_err(KernelError::Vault)?;
    if atom.op_code != 500 { return Err(KernelError::InvalidOpCode(atom.op_code)); }
    let data = kernel.resolve_data(&atom)?;
    let contract: crate::IOContract = serde_json::from_slice(&data)
        .map_err(|e| KernelError::Runtime(format!("IO Contract Parse Error: {}", e)))?;
    tracing::info!(endpoint = %contract.endpoint, "IO fetch");

    match check_endpoint(&contract.endpoint, contract.sensitivity) {
        SsrfVerdict::Allow => {}
        SsrfVerdict::Deny(why) => return Err(KernelError::Runtime(format!("endpoint blocked by SSRF guard: {}", why))),
    }

    let cap = 8 * 1024 * 1024;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| KernelError::Runtime(format!("client build: {}", e)))?;
    let response = client.get(&contract.endpoint).send().await
        .map_err(|e| KernelError::Runtime(format!("Network Error: {}", e)))?;
    let bytes = response.bytes().await
        .map_err(|e| KernelError::Runtime(format!("read body: {}", e)))?;
    if bytes.len() > cap { return Err(KernelError::Runtime(format!("response exceeds {} bytes", cap))); }
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| KernelError::Runtime(format!("JSON Parse Error: {}", e)))?;
    Ok(json)
}