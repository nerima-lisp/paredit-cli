use super::super::types::manifest::LoadedRefactorManifest;
use anyhow::{Context, Result};
use paredit_core_cli::shared::read_text_file_with_limit;
use paredit_core_cli::shared::stable_text_hash;
use serde_json::Value;
use std::path::Path as FsPath;

const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

pub fn read_refactor_manifest_file(
    manifest_path: &FsPath,
    expected_hash: Option<&str>,
) -> Result<LoadedRefactorManifest> {
    let manifest_text = read_text_file_with_limit(manifest_path, MAX_MANIFEST_BYTES)
        .with_context(|| format!("failed to read manifest {}", manifest_path.display()))?;
    let hash = stable_text_hash(&manifest_text);
    if let Some(expected_hash) = expected_hash {
        if expected_hash != hash {
            anyhow::bail!(
                "manifest hash mismatch for {}: expected {}, found {}",
                manifest_path.display(),
                expected_hash,
                hash
            );
        }
    }
    let value: Value = serde_json::from_str(&manifest_text)
        .with_context(|| format!("failed to parse manifest {}", manifest_path.display()))?;
    Ok(LoadedRefactorManifest { value, hash })
}
