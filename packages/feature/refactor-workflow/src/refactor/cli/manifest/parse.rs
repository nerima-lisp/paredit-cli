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

    let offset = |value: usize, field: &str| {
        ByteOffset::try_new(value).ok_or_else(|| {
            crate::error::ManifestError::Malformed(format!(
                "files[{file_index}].edits[{edit_index}].{field} exceeds the maximum byte offset"
            ))
        })
    };
    let span =
        ByteSpan::try_new(offset(start, "start")?, offset(end, "end")?).ok_or_else(|| {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_refactor_apply_manifest;

    fn manifest(start: u64, end: u64) -> serde_json::Value {
        json!({
            "mode": "rename-symbol",
            "from": "alpha",
            "to": "beta",
            "policy": { "passed": true },
            "summary": { "all_outputs_parse": true },
            "files": [{
                "path": "example.lisp",
                "dialect": "common-lisp",
                "changed": true,
                "output_parse_ok": true,
                "input_hash": "in",
                "output_hash": "out",
                "edits": [{ "start": start, "end": end, "replacement": "beta" }],
            }],
        })
    }

    /// A manifest is input, and `refactor check` exists to judge one without
    /// touching a file.
    ///
    /// An offset past `u32::MAX` used to reach `ByteOffset::new`, whose assert
    /// aborted the process at exit code 101 — so the command whose whole job
    /// is to reject a bad manifest crashed on one instead.
    #[test]
    fn an_edit_offset_beyond_the_bound_is_a_malformed_manifest() {
        for (start, end) in [
            (5_000_000_000_u64, 5_000_000_001_u64),
            (0, 5_000_000_000),
            (u64::from(u32::MAX) + 1, u64::from(u32::MAX) + 2),
        ] {
            let error = parse_refactor_apply_manifest(&manifest(start, end))
                .expect_err("an out-of-range offset is not a usable edit");
            let rendered = error.to_string();
            assert!(
                rendered.contains("exceeds the maximum byte offset"),
                "expected a malformed-manifest error, got {rendered}"
            );
        }
    }

    /// The bound check must not disturb a manifest that is in range, including
    /// one sitting exactly on the largest representable offset.
    #[test]
    fn edit_offsets_within_the_bound_still_parse() {
        for (start, end) in [(0_u64, 0_u64), (1, 4), (0, u64::from(u32::MAX))] {
            let parsed = parse_refactor_apply_manifest(&manifest(start, end))
                .expect("an in-range manifest parses");
            let span = parsed.files[0].edits[0].span;
            assert_eq!(span.start().get() as u64, start);
            assert_eq!(span.end().get() as u64, end);
        }
    }

    /// The pre-existing ordering check still fires, and is not shadowed by the
    /// new bound check on the way past.
    #[test]
    fn an_inverted_edit_span_is_still_rejected() {
        let error = parse_refactor_apply_manifest(&manifest(9, 4))
            .expect_err("start after end is not a usable edit");
        assert!(
            error.to_string().contains("start must not exceed end"),
            "expected the ordering error, got {error}"
        );
    }
}
