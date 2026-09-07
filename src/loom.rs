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

    pub fn weave(&self, intent: &str) -> Result<LogicAtom> {
        // Default context for now (User request mandates "context_id" in DataAtom)
        // Since `weave` is called by Registry/Bootstrap (GLOBAL context), we default to "global".
        // But if `orchestrator` calls it, it might need to pass context.
        // For now, I'll update the signature to `weave(&self, intent: &str, context: &str)`.
        self.weave_with_context(intent, "global")
    }
    
    pub fn weave_with_context(&self, intent_raw: &str, context: &str) -> Result<LogicAtom> {
        // Semantic Normalization
        let intent = intent_raw.trim();
        let intent_lower = intent.to_lowercase();
        let parts: Vec<&str> = intent.split_whitespace().collect();
        
        tracing::info!(intent = %intent, context = %context, "Loom processing intent");

        if parts.is_empty() {
             return Err(anyhow::anyhow!("Empty intent"));
        }

        // --- SEMANTIC PARSING (Keyword-based, not just positional) ---

        // 1. IO / FETCH
        // Keywords: "fetch", "get", "scrape", "load" + "from"
        if (intent_lower.starts_with("fetch") || intent_lower.starts_with("get") || intent_lower.starts_with("scrape")) 
           && intent_lower.contains("from") {
             if let Some(url_idx) = parts.iter().position(|&x| x == "from" || x == "From") {
                 if url_idx + 1 < parts.len() {
                     let url = parts[url_idx+1];
                     
                     // Basic validation: must look like a URL to be treated as an IO fetch
                     if !url.starts_with("http://") && !url.starts_with("https://") {
                         // Fallback to synthesis if it's not a real URL
                         let blob = intent.as_bytes().to_vec();
                         let ref_uri = write_blob(&blob)?;
                         return Ok(LogicAtom {
                             op_code: 600, // SYNTHESIS_REQUIRED
                             inputs: vec![],
                             storage_ref: ref_uri,
                             context_id: context.to_string(),
                         });
                     }

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

        // 2. FILTER
        // Keywords: "filter", "where", "keep", "select"
        if intent_lower.starts_with("filter") || intent_lower.starts_with("select") || intent_lower.contains("where") {
            // Heuristic: "Filter where <field> <op> <val>"
            // Find "where" or just assume structure if "filter" is first
            let start_idx = parts.iter().position(|&x| x.to_lowercase() == "where").unwrap_or(0);
            
            // We need at least field and op
            if parts.len() > start_idx + 2 {
                let field = parts[start_idx + 1];
                let op = parts[start_idx + 2];
                // Value might be the rest
                let val_str = if parts.len() > start_idx + 3 {
                    parts[start_idx+3..].join(" ")
                } else {
                    String::new() // Empty value for "contains {{search}}" when search is empty
                };
                
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
        }
        
        // 3. FINANCIAL AUDIT / VERIFY
        // Keywords: "verify", "check", "audit", "compliance"
        if intent_lower.starts_with("verify") || intent_lower.contains("audit") || intent_lower.contains("compliance") {
             // Just identity, empty blob for now
             let ref_uri = write_blob(&[])?;
             return Ok(LogicAtom {
                 op_code: 100, 
                 inputs: vec![],
                 storage_ref: ref_uri,
                 context_id: context.to_string(),
             });
        }
        
        // 4. SORT
        // Keywords: "sort", "order", "arrange"
        if intent_lower.starts_with("sort") || intent_lower.starts_with("order") {
             // Look for "by"
             let by_idx = parts.iter().position(|&x| x.to_lowercase() == "by").unwrap_or(0);
             if parts.len() > by_idx + 1 {
                 let field = parts[by_idx + 1];
                 let order = if intent_lower.contains("desc") { "desc" } else { "asc" };
                 
                 let blob = serde_json::to_vec(&serde_json::json!({
                     "field": field,
                     "order": order
                 }))?;
                 let ref_uri = write_blob(&blob)?;
                 return Ok(LogicAtom {
                     op_code: 4, // SORT
                     inputs: vec![],
                     storage_ref: ref_uri,
                     context_id: context.to_string(),
                 });
             }
        }

        // 5. HIGHLIGHT
        // Keywords: "highlight", "mark", "flag"
        if intent_lower.starts_with("highlight") || intent_lower.starts_with("mark") {
             let mode = if intent_lower.contains("cheapest") || intent_lower.contains("lowest") || intent_lower.contains("min") { 
                 "min" 
             } else { 
                 "max" 
             };
             let field = "price"; // Default for demo, or parse
             
             let blob = serde_json::to_vec(&serde_json::json!({
                 "mode": mode,
                 "field": field
             }))?;
             let ref_uri = write_blob(&blob)?;
             return Ok(LogicAtom {
                 op_code: 5, // HIGHLIGHT
                 inputs: vec![],
                 storage_ref: ref_uri,
                 context_id: context.to_string(),
             });
        }

        // 6. ENRICH
        if intent_lower.starts_with("enrich") {
             let ref_uri = write_blob(&[])?;
             return Ok(LogicAtom {
                 op_code: 6,
                 inputs: vec![],
                 storage_ref: ref_uri,
                 context_id: context.to_string(),
             });
        }
        
        // 7. OUTPUT
        if intent_lower.starts_with("output") 
            || intent_lower.starts_with("show") 
            || intent_lower.starts_with("display") 
            || intent_lower.starts_with("print")
            || intent_lower.contains("root node")
            || intent_lower.starts_with("root") 
        {
            let ref_uri = write_blob(&[])?;
             return Ok(LogicAtom {
                op_code: 7, 
                inputs: vec![],
                storage_ref: ref_uri,
                context_id: context.to_string(),
            });
        }
        
        // 8. ADD (Legacy)
        if intent_lower.starts_with("add") && parts.len() >= 4 {
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

        // --- SYNTHESIS FALLBACK ---
        let blob = intent.as_bytes().to_vec();
        let ref_uri = write_blob(&blob)?;
        
        tracing::info!(intent = %intent, "Loom synthesis fallback OpCode 600");
        
        Ok(LogicAtom {
            op_code: 600, // SYNTHESIS_REQUIRED
            inputs: vec![],
            storage_ref: ref_uri,
            context_id: context.to_string(),
        })
    }
}
