use super::super::super::*;
use super::super::types::manifest::{
    RefactorApplyManifest, RefactorApplyManifestEdit, RefactorApplyManifestFile,
};

pub(in crate::presentation::cli) fn parse_refactor_apply_manifest(
    value: &Value,
) -> Result<RefactorApplyManifest> {
    let object = value
        .as_object()
        .context("refactor manifest must be a JSON object")?;
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
            .collect::<Result<Vec<_>>>()?,
    })
}

fn parse_refactor_apply_manifest_file(
    index: usize,
    value: &Value,
) -> Result<RefactorApplyManifestFile> {
    let object = value
        .as_object()
        .with_context(|| format!("files[{index}] must be a JSON object"))?;
    let edits = required_array(object.get("edits"), &format!("files[{index}].edits"))?;
    let dialect_field = format!("files[{index}].dialect");
    let dialect_label = required_string(object.get("dialect"), &dialect_field)?;
    let dialect = dialect_label.parse::<Dialect>().with_context(|| {
        format!("manifest field {dialect_field} has invalid dialect {dialect_label:?}")
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
            .collect::<Result<Vec<_>>>()?,
    })
}

fn parse_refactor_apply_manifest_edit(
    file_index: usize,
    edit_index: usize,
    value: &Value,
) -> Result<RefactorApplyManifestEdit> {
    let object = value.as_object().with_context(|| {
        format!("files[{file_index}].edits[{edit_index}] must be a JSON object")
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
        ByteSpan::try_new(ByteOffset::new(start), ByteOffset::new(end)).with_context(|| {
            format!("files[{file_index}].edits[{edit_index}] start must not exceed end")
        })?;

    Ok(RefactorApplyManifestEdit { span, replacement })
}

fn required_object<'a>(
    value: Option<&'a Value>,
    field: &str,
) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .with_context(|| format!("missing required manifest field {field}"))?
        .as_object()
        .with_context(|| format!("manifest field {field} must be an object"))
}

fn required_array<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a Vec<Value>> {
    value
        .with_context(|| format!("missing required manifest field {field}"))?
        .as_array()
        .with_context(|| format!("manifest field {field} must be an array"))
}

fn required_string(value: Option<&Value>, field: &str) -> Result<String> {
    value
        .with_context(|| format!("missing required manifest field {field}"))?
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("manifest field {field} must be a string"))
}

fn required_bool(value: Option<&Value>, field: &str) -> Result<bool> {
    value
        .with_context(|| format!("missing required manifest field {field}"))?
        .as_bool()
        .with_context(|| format!("manifest field {field} must be a boolean"))
}

fn required_usize(value: Option<&Value>, field: &str) -> Result<usize> {
    let raw = value
        .with_context(|| format!("missing required manifest field {field}"))?
        .as_u64()
        .with_context(|| format!("manifest field {field} must be an unsigned integer"))?;
    usize::try_from(raw).with_context(|| format!("manifest field {field} is too large"))
}
