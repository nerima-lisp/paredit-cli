use super::super::types::manifest::{
    RefactorApplyManifest, RefactorApplyManifestEdit, RefactorApplyManifestFile,
};
use paredit_core_cli::CliResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ByteOffset;
use paredit_core_syntax::sexpr::ByteSpan;
use serde_json::Value;
use std::path::PathBuf;

pub fn parse_refactor_apply_manifest(value: &Value) -> CliResult<RefactorApplyManifest> {
    let object = value.as_object().ok_or_else(|| {
        crate::error::ManifestError::Malformed(String::from(
            "refactor manifest must be a JSON object",
        ))
    })?;
    let policy = required_object(object.get("policy"), "policy")?;
    let summary = required_object(object.get("summary"), "summary")?;
    let files = required_array(object.get("files"), "files")?;

    Ok(RefactorApplyManifest {
        mode: required_string(object.get("mode"), "mode")?,
        from: required_string(object.get("from"), "from")?,
        to: required_string(object.get("to"), "to")?,
        policy_passed: required_bool(policy.get("passed"), "policy.passed")?,
        all_outputs_parse: required_bool(
            summary.get("all_outputs_parse"),
            "summary.all_outputs_parse",
        )?,
        files: files
            .iter()
            .enumerate()
            .map(|(index, file)| parse_refactor_apply_manifest_file(index, file))
            .collect::<CliResult<Vec<_>>>()?,
    })
}

fn parse_refactor_apply_manifest_file(
    index: usize,
    value: &Value,
) -> CliResult<RefactorApplyManifestFile> {
    let object = value.as_object().ok_or_else(|| {
        crate::error::ManifestError::Malformed(format!("files[{index}] must be a JSON object"))
    })?;
    let edits = required_array(object.get("edits"), &format!("files[{index}].edits"))?;
    let dialect_field = format!("files[{index}].dialect");
    let dialect_label = required_string(object.get("dialect"), &dialect_field)?;
    let dialect = dialect_label.parse::<Dialect>().map_err(|_| {
        crate::error::ManifestError::UnsupportedDialect {
            field: dialect_field.clone(),
            dialect: dialect_label.to_string(),
        }
    })?;

    Ok(RefactorApplyManifestFile {
        path: PathBuf::from(required_string(
            object.get("path"),
            &format!("files[{index}].path"),
        )?),
        dialect,
        changed: required_bool(object.get("changed"), &format!("files[{index}].changed"))?,
        output_parse_ok: required_bool(
            object.get("output_parse_ok"),
            &format!("files[{index}].output_parse_ok"),
        )?,
        input_hash: required_string(
            object.get("input_hash"),
            &format!("files[{index}].input_hash"),
        )?,
        output_hash: required_string(
            object.get("output_hash"),
            &format!("files[{index}].output_hash"),
        )?,
        edits: edits
            .iter()
            .enumerate()
            .map(|(edit_index, edit)| parse_refactor_apply_manifest_edit(index, edit_index, edit))
            .collect::<CliResult<Vec<_>>>()?,
    })
}

fn parse_refactor_apply_manifest_edit(
    file_index: usize,
    edit_index: usize,
    value: &Value,
) -> CliResult<RefactorApplyManifestEdit> {
    let object = value.as_object().ok_or_else(|| {
        crate::error::ManifestError::Malformed(format!(
            "files[{file_index}].edits[{edit_index}] must be a JSON object"
        ))
    })?;
    let start = required_usize(
        object.get("start"),
        &format!("files[{file_index}].edits[{edit_index}].start"),
    )?;
    let end = required_usize(
        object.get("end"),
        &format!("files[{file_index}].edits[{edit_index}].end"),
    )?;
    let replacement = required_string(
        object.get("replacement"),
        &format!("files[{file_index}].edits[{edit_index}].replacement"),
    )?;

    let span =
        ByteSpan::try_new(ByteOffset::new(start), ByteOffset::new(end)).ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "files[{file_index}].edits[{edit_index}] start must not exceed end"
            ))
        })?;

    Ok(RefactorApplyManifestEdit { span, replacement })
}

fn required_object<'a>(
    value: Option<&'a Value>,
    field: &str,
) -> Result<&'a serde_json::Map<String, Value>, crate::error::ManifestError> {
    value
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "missing required manifest field {field}"
            ))
        })?
        .as_object()
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "manifest field {field} must be an object"
            ))
        })
}

fn required_array<'a>(
    value: Option<&'a Value>,
    field: &str,
) -> Result<&'a Vec<Value>, crate::error::ManifestError> {
    value
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "missing required manifest field {field}"
            ))
        })?
        .as_array()
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "manifest field {field} must be an array"
            ))
        })
}

fn required_string(
    value: Option<&Value>,
    field: &str,
) -> Result<String, crate::error::ManifestError> {
    value
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "missing required manifest field {field}"
            ))
        })?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "manifest field {field} must be a string"
            ))
        })
}

fn required_bool(value: Option<&Value>, field: &str) -> Result<bool, crate::error::ManifestError> {
    value
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "missing required manifest field {field}"
            ))
        })?
        .as_bool()
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "manifest field {field} must be a boolean"
            ))
        })
}

fn required_usize(
    value: Option<&Value>,
    field: &str,
) -> Result<usize, crate::error::ManifestError> {
    let raw = value
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "missing required manifest field {field}"
            ))
        })?
        .as_u64()
        .ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "manifest field {field} must be an unsigned integer"
            ))
        })?;
    usize::try_from(raw).map_err(|_| {
        crate::error::ManifestError::Malformed(format!("manifest field {field} is too large"))
    })
}
