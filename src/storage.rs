use std::fs;
use std::path::Path;
use std::collections::HashMap;
use blake3::Hasher;

const BLOB_DIR: &str = "../blobs";

/// Build a reverse index from BLAKE3 hash -> absolute file path by scanning
/// the blob directory. This allows Python SHA-256-named blobs to coexist with
/// Rust BLAKE3-named blobs: we index everything by BLAKE3 content hash.
fn build_index() -> HashMap<String, String> {
    let mut index = HashMap::new();
    let dir = Path::new(BLOB_DIR);
    if !dir.exists() { return index; }
    for entry in fs::read_dir(dir).unwrap_or_else(|_| fs::read_dir(".").unwrap()).flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Ok(bytes) = fs::read(&path) {
                let hash = blake3::hash(&bytes).to_hex().to_string();
                index.insert(hash, path.to_string_lossy().to_string());
            }
        }
    }
    index
}

/// Read a blob. Tries (1) direct path from BLAKE3 content address,
/// (2) direct literal file path (for Python SHA-256 refs), (3) reverse
/// index lookup by BLAKE3 content hash.
pub fn read_blob(uri: &str) -> std::io::Result<Vec<u8>> {
    if uri.starts_with("local://") {
        let suffix = &uri[8..];
        // Try direct literal path first.
        let literal = format!("{}/{}", BLOB_DIR, suffix);
        if Path::new(&literal).exists() {
            return fs::read(&literal);
        }
        // Try direct BLAKE3-named file (standard case).
        let direct = format!("{}/{}", BLOB_DIR, suffix);
        if Path::new(&direct).exists() {
            return fs::read(&direct);
        }
        // Fallback: build reverse index and find file with matching content hash.
        let index = build_index();
        let file_path = index.get(suffix).cloned()
            .or_else(|| index.values().find(|_| false).cloned())
            .unwrap_or_else(|| direct);
        return fs::read(&file_path);
    }
    Err(std::io::Error::new(std::io::ErrorKind::Other, "Unsupported Storage Scheme"))
}

pub fn write_blob(data: &[u8]) -> std::io::Result<String> {
    fs::create_dir_all(BLOB_DIR)?;
    let mut hasher = Hasher::new();
    hasher.update(data);
    let hash = hasher.finalize().to_hex().to_string();
    let path = format!("{}/{}", BLOB_DIR, hash);
    if !Path::new(&path).exists() {
        fs::write(&path, data)?;
    }
    Ok(format!("local://{}", hash))
}

pub fn ensure_store() -> std::io::Result<()> {
    fs::create_dir_all(BLOB_DIR)
}
