use anyhow::{Result, Ok};
use crate::{LogicAtom, write_blob};

// Placeholder for Candle-based LLM state
pub struct AetherLoom {
    // Reference to model/tokenizer would go here
}

impl AetherLoom {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    /// Constrains the AI to output ONLY a specific JSON format
    const SYSTEM_PROMPT: &'static str = "You are Aether-Loom. Output ONLY valid JSON for LogicAtom.";

    pub fn weave(&self, intent: &str) -> Result<LogicAtom> {
        // Default context for now (User request mandates "context_id" in DataAtom)
        // Since `weave` is called by Registry/Bootstrap (GLOBAL context), we default to "global".
        // But if `orchestrator` calls it, it might need to pass context.
        // For now, I'll update the signature to `weave(&self, intent: &str, context: &str)`.
        self.weave_with_context(intent, "global")
    }
    
    pub fn weave_with_context(&self, intent: &str, context: &str) -> Result<LogicAtom> {
        println!("[Loom] Processing Intent: '{}' in context '{}'", intent, context);

        let parts: Vec<&str> = intent.split_whitespace().collect();
        if parts.is_empty() {
             return Err(anyhow::anyhow!("Empty intent"));
        }

        // 1. Generic IO: "Fetch from <URL>"
        if parts[0] == "Fetch" && parts.contains(&"from") {
             if let Some(url_idx) = parts.iter().position(|&x| x == "from") {
                 if url_idx + 1 < parts.len() {
                     let url = parts[url_idx+1];
                     let contract = crate::IOContract {
                         endpoint: url.to_string(),
                         schema: serde_json::json!({"type": "array"}),
                         sensitivity: if url.contains("localhost") || url.contains("127.0.0.1") { 2 } else { 0 },
                     };
                     
                     let blob = serde_json::to_vec(&contract)?;
                     let ref_uri = write_blob(&blob)?;
                     
                     return Ok(LogicAtom {
                         op_code: 500,
                         inputs: vec![],
                         storage_ref: ref_uri,
                         context_id: context.to_string(),
                     });
                 }
             }
        }

        // 2. Generic Filter: "Filter where <field> <op> <value>"
        // Example: "Filter where built > 2020"
        if parts[0] == "Filter" && parts.get(1) == Some(&"where") && parts.len() >= 5 {
             let field = parts[2];
             let op = parts[3];
             // Support multi-word values (e.g. "Bukit Bintang")
             let val_str = parts[4..].join(" ");
             
             // Try parsing val as number, else string
             let val_json = if let std::result::Result::Ok(num) = val_str.parse::<i64>() {
                 serde_json::to_value(num)?
             } else {
                 serde_json::to_value(&val_str)?
             };

             let config = serde_json::json!({
                "field": field,
                "op": op,
                "val": val_json
             });

             let blob = serde_json::to_vec(&config)?;
             let ref_uri = write_blob(&blob)?;

             return Ok(LogicAtom {
                op_code: 2, // FILTER
                inputs: vec![],
                storage_ref: ref_uri,
                context_id: context.to_string(),
            });
        }
        
        // 3. Generic Financial: "Verify ..." (Placeholder)
        if parts[0] == "Verify" {
             // Just identity, empty blob
             let ref_uri = write_blob(&[])?;
             return Ok(LogicAtom {
                 op_code: 100, 
                 inputs: vec![],
                 storage_ref: ref_uri,
                 context_id: context.to_string(),
             });
        }
        

        // 4. Output/Identity
        if parts[0] == "Output" {
            let ref_uri = write_blob(&[])?;
             return Ok(LogicAtom {
                op_code: 100, // Identity
                inputs: vec![],
                storage_ref: ref_uri,
                context_id: context.to_string(),
            });
        }

        // 5. Merge/Union: "Merge <...>"
        if parts[0] == "Merge" {
             let ref_uri = write_blob(&[])?;
             return Ok(LogicAtom {
                 op_code: 3, // MERGE
                 inputs: vec![],
                 storage_ref: ref_uri,
                 context_id: context.to_string(),
             });
        }

        // 6. Web Scrape (Alias for Fetch)
        if parts[0] == "Web" && parts.contains(&"scrape") {
             // Hardcode the mock scraper URL for this demo intent
             let url = "http://127.0.0.1:8080/kl/properties";
             let contract = crate::IOContract {
                 endpoint: url.to_string(),
                 schema: serde_json::json!({"type": "array"}),
                 sensitivity: 2,
             };
             
             let blob = serde_json::to_vec(&contract)?;
             let ref_uri = write_blob(&blob)?;
             
             return Ok(LogicAtom {
                 op_code: 500,
                 inputs: vec![],
                 storage_ref: ref_uri,
                 context_id: context.to_string(),
             });
        }

        // 7. Trigger/Event: "Trigger data refresh when <event>"
        // Example: "Trigger data refresh when the dropdown value changes"
        if parts[0] == "Trigger" {
             // We can parse the event details here or just store the raw intent for the UI to interpret
             // For now, let's treat it as a Reactive Binding (OpCode 50)
             let blob = serde_json::to_vec(&serde_json::json!({
                 "event": intent
             }))?;
             let ref_uri = write_blob(&blob)?;
             
             return Ok(LogicAtom {
                 op_code: 50, // REACTIVE_TRIGGER
                 inputs: vec![],
                 storage_ref: ref_uri,
                 context_id: context.to_string(),
             });
        }


        // Fallback: Legacy "Add X and Y"
        if parts[0] == "Add" && parts.len() >= 4 {
             let a: i32 = parts[1].parse().unwrap_or(0);
             let b: i32 = parts[3].parse().unwrap_or(0);
             let blob = [a.to_le_bytes(), b.to_le_bytes()].concat();
             let ref_uri = write_blob(&blob)?;
             
             return Ok(LogicAtom {
                 op_code: 1,
                 inputs: vec![],
                 storage_ref: ref_uri,
                 context_id: context.to_string(),
             });
        }

        // --- PHASE II: SOVEREIGN SYNTHESIS PROTOCOL ---
        // Fallback: Instead of crashing, return a "Synthesis Request" (OpCode 600)
        // This tells the UI/Orchestrator: "I don't know this, but I am ready to learn."
        // The original intent is preserved in the blob for the factory to analyze.
        let blob = intent.as_bytes().to_vec();
        let ref_uri = write_blob(&blob)?;
        
        println!("[Loom] Intent '{}' unknown -> Triggering Synthesis (OpCode 600)", intent);
        
        Ok(LogicAtom {
            op_code: 600, // SYNTHESIS_REQUIRED
            inputs: vec![],
            storage_ref: ref_uri,
            context_id: context.to_string(),
        })
    }
}
