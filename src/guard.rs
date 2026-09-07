use anyhow::Result;
use crate::ssrf;

pub struct AetherGuard;

impl AetherGuard {
    pub fn new() -> Self {
        Self
    }

    pub fn verify_compatibility(&self, atom: &crate::LogicAtom, input_atoms: &[crate::LogicAtom]) -> Result<()> {
        match atom.op_code {
            1 => { /* ADD uses raw bytes — no input graph contract */ }
            2 => {
                // FILTER requires an array-producing upstream node.
                if input_atoms.is_empty() {
                    return Err(anyhow::anyhow!("Filter (Op 2) requires at least one input (Source List)"));
                }
                let src = &input_atoms[0].op_code;
                if src == &1 {
                    return Err(anyhow::anyhow!("Type Mismatch: Filter cannot consume integer output of ADD (Op 1)"));
                }
            }
            3 => {
                // MERGE requires at least one array input.
                if input_atoms.is_empty() {
                    return Err(anyhow::anyhow!("Merge (Op 3) requires at least one input"));
                }
            }
            4 => {
                // SORT requires exactly one array input.
                if input_atoms.len() != 1 {
                    return Err(anyhow::anyhow!("Sort (Op 4) requires exactly one input"));
                }
                if input_atoms[0].op_code == 1 {
                    return Err(anyhow::anyhow!("Type Mismatch: Sort cannot consume scalar output of ADD (Op 1)"));
                }
            }
            5 => {
                // HIGHLIGHT requires one array input.
                if input_atoms.len() != 1 {
                    return Err(anyhow::anyhow!("Highlight (Op 5) requires exactly one input"));
                }
                if input_atoms[0].op_code == 1 {
                    return Err(anyhow::anyhow!("Type Mismatch: Highlight cannot consume scalar output of ADD (Op 1)"));
                }
            }
            6 => {
                // ENRICH: base array + optional lookup. Base must be array.
                if input_atoms.is_empty() {
                    return Err(anyhow::anyhow!("Enrich (Op 6) requires at least one base input"));
                }
                if input_atoms[0].op_code == 1 {
                    return Err(anyhow::anyhow!("Type Mismatch: Enrich base must be an array"));
                }
            }
            7 => {
                // OUTPUT pass-through; any input type allowed (including none).
            }
            50 => { /* REACTIVE_TRIGGER: config-driven */ }
            100 => {
                // FINANCIAL AUDIT: should sit downstream of data-producing node.
                if input_atoms.is_empty() {
                    return Err(anyhow::anyhow!("Audit (Op 100) requires at least one input to audit"));
                }
            }
            500 => {
                // IO: typically has no inputs (it's a source). Allow 0–1.
                if input_atoms.len() > 1 {
                    return Err(anyhow::anyhow!("IO (Op 500) accepts at most one input"));
                }
            }
            800 => {
                // GATEWAY: requires one internal result to mask.
                if input_atoms.len() != 1 {
                    return Err(anyhow::anyhow!("Gateway (Op 800) requires exactly one input"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn check(&self, _atom: &crate::LogicAtom) -> Result<()> {
        Ok(())
    }

    /// 0% Riba law — plain integer equality.
    pub fn verify_interest_free(&self, rate: i32) -> bool {
        rate == 0
    }

    /// Sovereignty law: sensitive data stays on .my / localhost.
    pub fn verify_sovereignty(&self, endpoint: &str, sensitivity: u8) -> bool {
        if sensitivity < 2 {
            return true;
        }
        match ssrf::check_endpoint(endpoint, sensitivity) {
            ssrf::SsrfVerdict::Allow => true,
            ssrf::SsrfVerdict::Deny(_) => false,
        }
    }

    /// Sustainability bound: usage < limit. Trivial int compare.
    pub fn verify_sustainability(&self, usage_metric: i32, limit: i32) -> bool {
        usage_metric < limit
    }
}