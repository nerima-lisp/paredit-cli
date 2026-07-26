use anyhow::Result;

use crate::domain::sexpr::{ByteSpan, Path, SyntaxTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopLevelInsert {
    Append,
    Before,
    After,
}

impl TopLevelInsert {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

pub(crate) fn replace_span(input: &str, span: ByteSpan, replacement: &str) -> String {
    let mut output = String::with_capacity(input.len() - span.len() + replacement.len());
    output.push_str(&input[..span.start().get()]);
    output.push_str(replacement);
    output.push_str(&input[span.end().get()..]);
    output
}

pub(crate) fn replace_span_checked(
    input: &str,
    span: ByteSpan,
    replacement: &str,
) -> Result<String> {
    span.validate_against(input)
        .map_err(|error| anyhow::anyhow!("replacement span is invalid: {error}"))?;
    input
        .len()
        .checked_sub(span.len())
        .and_then(|retained| retained.checked_add(replacement.len()))
        .ok_or_else(|| anyhow::anyhow!("replacement output size overflow"))?;
    Ok(replace_span(input, span, replacement))
}

pub(crate) fn insert_top_level_form(
    input: &str,
    tree: &SyntaxTree,
    form: &str,
    insert: TopLevelInsert,
    anchor_path: Option<&Path>,
    command: &str,
) -> Result<(String, Option<ByteSpan>)> {
    match insert {
        TopLevelInsert::Append => Ok((append_top_level_form(input, form), None)),
        TopLevelInsert::Before | TopLevelInsert::After => {
            let anchor_path = anchor_path
                .ok_or_else(|| anyhow::anyhow!("--insert before/after requires --anchor-path"))?;
            let anchor_index = top_level_path_index(anchor_path, command)?;
            if anchor_index >= tree.root_children().len() {
                anyhow::bail!("anchor top-level path {anchor_path} is out of range");
            }
            let anchor = tree.select_path(anchor_path)?;
            let anchor_span = anchor.span();
            let (offset, inserted) = match insert {
                TopLevelInsert::Before => {
                    (anchor_span.start().get(), format!("{}\n\n", form.trim()))
                }
                TopLevelInsert::After => (anchor_span.end().get(), format!("\n\n{}", form.trim())),
                TopLevelInsert::Append => {
                    anyhow::bail!("append insertion does not use an anchor")
                }
            };
            let mut output = String::with_capacity(input.len() + inserted.len());
            output.push_str(&input[..offset]);
            output.push_str(&inserted);
            output.push_str(&input[offset..]);
            Ok((output, Some(anchor_span)))
        }
    }
}

fn append_top_level_form(input: &str, form: &str) -> String {
    if input.trim().is_empty() {
        format!("{}\n", form.trim())
    } else {
        format!("{}\n\n{}\n", input.trim_end(), form.trim())
    }
}

fn top_level_path_index(path: &Path, command: &str) -> Result<usize> {
    match path.indexes() {
        [index] => Ok(index.get()),
        _ => anyhow::bail!("{command} requires a top-level path, for example --path 2"),
    }
}
