use aether_store::{AetherVault, AetherGuard, AetherLoom, AetherKernel};
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vault = AetherVault::new("aether_db")?;
    let guard = AetherGuard::new();
    let loom = AetherLoom::new()?;
    // Pass a clone of the vault to the kernel so we keep one for persistence
    let kernel = AetherKernel::new(vault.clone());

    println!("--- Aether Tool v1.0 ---");
    println!("Describe the logic you want to create (e.g., 'Add 50 and 100', 'Calculate Zakat for 10000'):");
    print!("> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let intent = input.trim();

    if intent.is_empty() {
        println!("No input provided.");
        return Ok(());
    }

    // 1. Weaver: Intent -> Atom
    println!("\n[Loom] Weaving intent into LogicAtom...");
    let atom = match loom.weave(intent) {
        Ok(a) => a,
        Err(e) => {
            println!("[Loom] Failed: {}", e);
            return Ok(());
        }
    };
    println!("[Loom] Created Atom: {:?}", atom);

    // 2. Guard: Atom -> Verified Hash
    println!("[Guard] Verifying Genesis Laws...");
    let hash = match vault.persist_verified(&atom, &guard) {
        Ok(h) => h,
        Err(e) => {
            println!("[Guard] BLOCKED: {}", e);
            return Ok(());
        }
    };
    println!("[Guard] Verified & Persisted. Hash: {}", hash);

    // 3. Kernel: Hash -> Result
    // Only run if it's executable (OpCode 1). 
    // Zakat (OpCode 100) logic isn't implemented in Kernel yet, but let's see.
    if atom.op_code == 1 {
        println!("[Kernel] Executing logic...");
        match kernel.execute(&hash) {
            Ok(res) => println!("[Kernel] Result: {}", res),
            Err(e) => println!("[Kernel] Execution Error: {}", e),
        }
    } else {
        println!("[Kernel] OpCode {} is storage-only for now (or not implemented in execution engine).", atom.op_code);
    }

    Ok(())
}
