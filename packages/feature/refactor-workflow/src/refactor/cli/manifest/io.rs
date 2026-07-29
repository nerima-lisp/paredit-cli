use super::super::types::manifest::LoadedRefactorManifest;
use paredit_core_cli::CliResult;
use paredit_core_cli::shared::read_text_file_with_limit;
use paredit_core_cli::shared::stable_text_hash;
use serde_json::Value;
use std::path::Path as FsPath;

const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

pub fn read_refactor_manifest_file(
    manifest_path: &FsPath,
    expected_hash: Option<&str>,
) -> CliResult<LoadedRefactorManifest> {
    let manifest_text = read_text_file_with_limit(manifest_path, MAX_MANIFEST_BYTES).map_err(
        crate::error::RefactorContext::new(format!(
            "failed to read manifest {}",
            manifest_path.display()
        )),
    )?;
    let hash = stable_text_hash(&manifest_text);
    if let Some(expected_hash) = expected_hash {
        if expected_hash != hash {
            return Err(paredit_core_cli::error::FeatureRefusal::message(
                paredit_core_cli::diagnosis::ErrorCode::RefusalTargetChanged,
                format!(
                    "manifest hash mismatch for {}: expected {}, found {}",
                    manifest_path.display(),
                    expected_hash,
                    hash
                ),
            )
            .into());
        }
    }
    let value: Value = serde_json::from_str(&manifest_text).map_err(|source| {
        paredit_core_cli::CliError::Json {
            context: format!("failed to parse manifest {}", manifest_path.display()),
            source,
        }
    })?;
    Ok(LoadedRefactorManifest { value, hash })
}
