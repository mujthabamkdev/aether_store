use z3::{Solver, SatResult};
use anyhow::Result;

pub struct AetherGuard;

impl AetherGuard {
    pub fn new() -> Self {
        Self
    }

    pub fn verify_compatibility(&self, atom: &crate::LogicAtom, input_atoms: &[crate::LogicAtom]) -> Result<()> {
        // Static Analysis of OpCode Connections
        match atom.op_code {
            2 => { // FILTER
                if input_atoms.is_empty() {
                     return Err(anyhow::anyhow!("Filter (Op 2) requires at least one input (Source List)"));
                }
                // Verify Input 0 is compatible (e.g., IO or Audit or Store, not ADD)
                // Assuming Op 500 (IO) produces List. Op 1 (ADD) produces Int.
                if input_atoms[0].op_code == 1 {
                     return Err(anyhow::anyhow!("Type Mismatch: Filter cannot consume integer output of ADD (Op 1)"));
                }
            },
            1 => { // ADD
                 // Needs no atoms (uses raw data) or atoms that produce bytes?
                 // My ADD legacy implementation uses Raw Data.
            },
            _ => {}
        }
        Ok(())
    }

    pub fn check(&self, atom: &crate::LogicAtom) -> Result<()> {
        // Existing checks
        Ok(())
    }

    /// Verifies if a mathematical operation is "Safe" (Example: 0% Riba Law)
    pub fn verify_interest_free(&self, rate: i32) -> bool {
        // Based on compiler error: Solver::new() takes no arguments
        let solver = Solver::new();
        
        // Based on compiler error: Int::from_i64 takes 1 argument (value)
        let interest_rate = z3::ast::Int::from_i64(rate as i64);
        let zero = z3::ast::Int::from_i64(0);

        // Law: interest_rate MUST equal 0
        // Use .eq() instead of deprecated ._eq()
        solver.assert(&interest_rate.eq(&zero));

        solver.check() == SatResult::Sat
    }

    pub fn verify_sovereignty(&self, endpoint: &str, sensitivity: u8) -> bool {
        if sensitivity >= 2 {
            // Law: Sovereign data MUST remain on localhost or .my domains
            return endpoint.contains("localhost") || endpoint.contains("127.0.0.1") || endpoint.contains(".my");
        }
        true
    }
}
