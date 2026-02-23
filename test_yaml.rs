fn main() {
    let yaml = r#"
app_name: "Test"
styles:
  --accent-color: purple
  .search-buttons {
    display: flex;
    gap: 10px;
  }
"#;
    match serde_yaml::from_str::<serde_yaml::Value>(yaml) {
        Ok(_) => println!("Parsed OK!"),
        Err(e) => println!("Error: {}", e),
    }
}
