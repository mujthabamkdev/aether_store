use aether_store::{AetherVault, AetherGuard, LogicAtom};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vault = AetherVault::new("aether_db")?;
    let guard = AetherGuard::new();

    // 1. Define "Halal" Logic (OpCode 100, Rate 0)
    let halal_atom = LogicAtom {
        op_code: 100, 
        inputs: vec![], 
        data: 0i32.to_le_bytes().to_vec()
    };

    match vault.persist_verified(&halal_atom, &guard) {
        Ok(hash) => println!("Halal Atom persisted: {}", hash),
        Err(e) => println!("Unexpected error: {}", e),
    }

    // 2. Define "Haram" Logic (OpCode 100, Rate 5)
    let haram_atom = LogicAtom {
        op_code: 100, 
        inputs: vec![], 
        data: 5i32.to_le_bytes().to_vec()
    };

    match vault.persist_verified(&haram_atom, &guard) {
        Ok(_) => println!("Error: Riba was allowed!"),
        Err(e) => println!("Success: The Guard blocked Riba. Reason: {}", e),
    }

    Ok(())
}
