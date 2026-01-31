use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IOContract {
    pub endpoint: String,    // e.g., "http://localhost:8080/shopee/balance"
    pub schema: serde_json::Value, // The JSON Schema the response must follow
    pub sensitivity: u8,     // 0: Public, 1: Private, 2: Sovereign (Local Only)
}
