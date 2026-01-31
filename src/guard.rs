use z3::{Solver, SatResult};

pub struct AetherGuard;

impl AetherGuard {
    pub fn new() -> Self {
        Self
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
}
