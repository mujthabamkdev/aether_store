use std::fs;
use aether_store::AetherManifest;

fn main() {
    let content = fs::read_to_string("../../products/transit-home/manifest.yaml").unwrap();
    match serde_yaml::from_str::<AetherManifest>(&content) {
        Ok(_) => println!("Parsed Successfully!"),
        Err(e) => println!("Parse Error: {}", e),
    }
}
