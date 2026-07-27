use paredit_core_syntax::sexpr::ByteSpan;

#[derive(Debug)]
pub struct RefactorPreviewEdit {
    span: ByteSpan,
    replacement: String,
}

impl RefactorPreviewEdit {
    #[must_use]
    pub const fn new(span: ByteSpan, replacement: String) -> Self {
        Self { span, replacement }
    }

    #[must_use]
    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    #[must_use]
    pub const fn start(&self) -> usize {
        self.span.start().get()
    }

    #[must_use]
    pub const fn end(&self) -> usize {
        self.span.end().get()
    }

    #[must_use]
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}
