use std::fs;
use std::path::Path;
use blake3::Hasher;
use std::io::{self, Write};

const BLOB_DIR: &str = "../blobs";

pub fn ensure_store() -> io::Result<()> {
    if !Path::new(BLOB_DIR).exists() {
        fs::create_dir_all(BLOB_DIR)?;
    }
    Ok(())
}

pub fn write_blob(data: &[u8]) -> io::Result<String> {
    ensure_store()?;
    
    let mut hasher = Hasher::new();
    hasher.update(data);
    let hash = hasher.finalize().to_hex().to_string();
    
    let path = format!("{}/{}", BLOB_DIR, hash);
    if !Path::new(&path).exists() {
        let mut file = fs::File::create(&path)?;
        file.write_all(data)?;
    }
    
    // Return the Storage URI
    Ok(format!("local://{}", hash))
}

pub fn read_blob(uri: &str) -> io::Result<Vec<u8>> {
    if uri.starts_with("local://") {
        let hash = &uri[8..];
        let path = format!("{}/{}", BLOB_DIR, hash);
        return fs::read(path);
    }
    Err(io::Error::new(io::ErrorKind::Other, "Unsupported Storage Scheme"))
}
