use aether_store::{AetherVault, LogicAtom};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vault = AetherVault::new("aether_db")?;

    // Create a sample Logic Atom (e.g., An 'Addition' rule)
    let atom = LogicAtom {
        op_code: 1, 
        inputs: vec![], 
        data: vec![5, 10] 
    };

    // Save to the Warehouse
    let hash = vault.persist(&atom)?;
    println!("Atom stored with identity: {}", hash);

    // Add a sanity check for deduplication
    let hash2 = vault.persist(&atom)?;
    assert_eq!(hash, hash2, "Deduplication failed: hashes should be identical");
    println!("Deduplication verified: {}", hash2);

    // Retrieve by identity
    let retrieved = vault.fetch(&hash)?;
    println!("Successfully retrieved Atom with OpCode: {}", retrieved.op_code);
    assert_eq!(atom, retrieved, "Retrieval failed: data mismatch");

    Ok(())
}
