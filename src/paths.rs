use std::path::{Path, PathBuf};

/// Resolve all engine filesystem paths from the binary's location so the
/// engine behaves the same regardless of CWD.
///
/// Layout assumption: `warehouse/engine/<binary>` is one level under `warehouse/`,
/// and `warehouse/` is two levels under the repo root.
#[derive(Clone)]
pub struct Paths {
    pub repo_root: PathBuf,
    pub warehouse_dir: PathBuf,
    pub engine_dir: PathBuf,
    pub products_dir: PathBuf,
    pub blobs_dir: PathBuf,
    pub catalog_path: PathBuf,
    pub registry_path: PathBuf,
    pub sled_path: PathBuf,
}

impl Paths {
    pub fn discover() -> Self {
        let engine_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        // Walk up to find products/ marker (works regardless of CWD).
        let mut cursor: Option<&Path> = Some(engine_dir.as_path());
        let mut repo_root = engine_dir.clone();
        while let Some(c) = cursor {
            if c.join("products").is_dir() {
                repo_root = c.to_path_buf();
                break;
            }
            cursor = c.parent();
        }
        let warehouse_dir = repo_root.join("warehouse");
        Self {
            products_dir: repo_root.join("products"),
            blobs_dir: warehouse_dir.join("blobs"),
            catalog_path: warehouse_dir.join("catalog.json"),
            registry_path: warehouse_dir.join("registry.json"),
            sled_path: engine_dir.join("aether_db"),
            repo_root,
            warehouse_dir,
            engine_dir,
        }
    }

    /// Path to a project's manifest. Validates that the resolved path is
    /// contained in `products_dir` (no traversal outside the projects tree).
    pub fn manifest_for(&self, project: &str) -> Result<PathBuf, String> {
        if project.is_empty()
            || project.contains("..")
            || project.contains('/')
            || project.contains('\\')
            || project.contains('\0')
        {
            return Err(format!("invalid project name: {:?}", project));
        }
        let candidate = self.products_dir.join(project).join("manifest.yaml");
        let canonical_products = self
            .products_dir
            .canonicalize()
            .unwrap_or_else(|_| self.products_dir.clone());
        let candidate_parent = candidate
            .parent()
            .ok_or("manifest path has no parent")?
            .to_path_buf();
        let canonical_parent = candidate_parent
            .canonicalize()
            .unwrap_or(candidate_parent);
        if !canonical_parent.starts_with(&canonical_products) {
            return Err("manifest escapes products directory".into());
        }
        Ok(candidate)
    }
}