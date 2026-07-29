//! One pattern match, as a finding the shared report envelope can print.

use paredit_core_cli::report::{Finding, FindingSeverity};
use paredit_core_cli::shared::bounded_preview;
use paredit_core_syntax::selector::{LineIndex, PatternMatch};
use paredit_core_syntax::sexpr::ByteSpan;
use serde_json::{Value, json};

/// How much source text a match carries by default.
///
/// The same budget `inspect resolve` uses. A repository-wide search returns
/// far more matches than a single-file resolve does, so the case for keeping
/// each one short is only stronger here.
pub const DEFAULT_PREVIEW_BYTES: usize = 80;

/// One form the pattern matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternHit {
    pub span: ByteSpan,
    pub line: usize,
    pub column: usize,
    /// The tree path, so the hit feeds straight into `--path` on any edit.
    pub path: String,
    /// The stable content id, so it survives edits above it.
    pub id: Option<String>,
    pub preview: String,
    /// Each capture as `name=text`, in the matcher's name order.
    pub captures: Vec<(String, String)>,
}

impl PatternHit {
    /// Builds a hit from one match, reading `source` for the preview.
    #[must_use]
    pub fn new(
        found: &PatternMatch,
        source: &str,
        index: &LineIndex,
        id: Option<String>,
        preview_bytes: usize,
    ) -> Self {
        let position = index.position_of(source, found.span.start().get());
        let text = &source[found.span.start().get()..found.span.end().get()];
        Self {
            span: found.span,
            line: position.line(),
            column: position.column(),
            path: found.path.to_string(),
            id,
            preview: bounded_preview(text, preview_bytes),
            captures: found
                .captures
                .iter()
                .map(|capture| (capture.name.clone(), capture.text.clone()))
                .collect(),
        }
    }
}

impl Finding for PatternHit {
    fn kind(&self) -> &'static str {
        "match"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        let mut columns = vec![self.path.clone(), self.preview.clone()];
        columns.extend(
            self.captures
                .iter()
                .map(|(name, text)| format!("{name}={text}")),
        );
        columns
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        let captures = self
            .captures
            .iter()
            .map(|(name, text)| json!({ "name": name, "text": text }))
            .collect::<Vec<_>>();
        vec![
            ("path", Value::String(self.path.clone())),
            ("column", json!(self.column)),
            ("id", self.id.clone().map_or(Value::Null, Value::String)),
            ("preview", Value::String(self.preview.clone())),
            ("captures", Value::Array(captures)),
        ]
    }

    /// A match is an observation, not a defect.
    ///
    /// The pattern comes from the caller, so this command has no way to know
    /// whether finding it is good news or bad. `note` is the rung that says
    /// "here it is" without the tool pretending to an opinion it does not
    /// have; a caller who *is* hunting for a defect arms `--fail-on-match`.
    fn severity(&self) -> FindingSeverity {
        FindingSeverity::Note
    }

    fn message(&self) -> String {
        self.preview.clone()
    }
}
