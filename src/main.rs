use aether_store::{AetherVault, LogicAtom, AetherKernel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vault = AetherVault::new("aether_db")?;
    let kernel = AetherKernel::new(vault);

    // 1. Define Logic (Add 10 + 20)
    let mut data = Vec::new();
    data.extend_from_slice(&10i32.to_le_bytes());
    data.extend_from_slice(&20i32.to_le_bytes());

    let atom = LogicAtom {
        op_code: 1, 
        inputs: vec![], 
        data 
    };

    // 2. Persist to Warehouse
    let hash = kernel.vault.persist(&atom)?;
    println!("Logic persisted with Hash: {}", hash);

    // 3. Execute from Hash
    let result = kernel.execute(&hash)?;
    println!("Execution Result: {}", result); // Should print 30

    Ok(())
}
