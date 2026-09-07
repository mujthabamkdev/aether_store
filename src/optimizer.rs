use crate::{AetherLoom, LogicAtom};

pub struct AetherOptimizer {
    threshold_ns: u128,
}

impl AetherOptimizer {
    pub fn new(threshold_ns: u128) -> Self {
        Self { threshold_ns }
    }

    /// Checks if a hash is slow and requests an 'Optimized Weave' from the Loom
    pub fn optimize_if_needed(&self, hash: &str, duration: u128, loom: &AetherLoom) -> Option<LogicAtom> {
        if duration > self.threshold_ns {
            tracing::warn!(hash = %hash, duration = %duration, threshold = %self.threshold_ns, "Optimizer performance warning: slow node");
            tracing::info!("Optimizer triggering autonomous evolution...");
            
            // Task the Loom to generate a faster version.
            // In a real system, we would pass the original atom or its source intent.
            // Here we send a prompt that triggers the Loom's optimization heuristic.
            let intent = format!("Optimize the logic for the node with hash {}. Goal: Reduce execution time.", hash);
            
            match loom.weave(&intent) {
                Ok(atom) => return Some(atom),
                Err(e) => tracing::warn!(error = %e, "Optimizer failed to evolve"),
            }
        }
        None
    }
}
