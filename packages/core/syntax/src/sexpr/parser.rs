use thiserror::Error;

use crate::dialect::Dialect;

use super::reader_policy::{DialectReaderPolicy, ReaderMacro};
use super::tree::{Comment, Node, NodeKind, ReaderPrefix, SyntaxTree};
use super::types::{ByteOffset, ByteSpan, Delimiter, NodeId};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("unexpected closing delimiter '{delimiter}' at byte {position}")]
    UnexpectedClose { delimiter: char, position: usize },
    #[error("unsupported reader dispatch '{dispatch}' at byte {position}")]
    UnsupportedReaderDispatch { dispatch: String, position: usize },
    #[error("mismatched closing delimiter '{found}' at byte {position}; expected '{expected}'")]
    MismatchedClose {
        found: char,
        expected: char,
        position: usize,
    },
    #[error("unclosed list starting at byte {0}")]
    UnclosedList(usize),
    #[error("unterminated string starting at byte {0}")]
    UnterminatedString(usize),
    #[error("unterminated block comment starting at byte {0}")]
    UnterminatedBlockComment(usize),
    #[error("unterminated multiple-escape symbol starting at byte {0}")]
    UnterminatedSymbol(usize),
    #[error("single escape at byte {0} is missing an escaped character")]
    DanglingSingleEscape(usize),
    #[error("reader prefix or discard at byte {0} is missing a form")]
    MissingReaderForm(usize),
    #[error(
        "discarded reader form complexity limit exceeded at byte {position}: maximum is {limit} parser frames"
    )]
    ResourceLimitExceeded { position: usize, limit: usize },
}

pub(super) const MAX_DISCARDED_FORM_STACK_FRAMES: usize = 65_536;

pub(in crate::sexpr) struct Parser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: ByteOffset,
    nodes: Vec<Node>,
    stack: Vec<NodeId>,
    comments: Vec<Comment>,
    policy: DialectReaderPolicy,
    /// Nesting depth of `#;` datum comments currently being skipped. While
    /// positive, inner trivia is folded into the enclosing datum comment span
    /// instead of being recorded as separate comments.
    suppress_depth: usize,
}

#[derive(Debug, Clone, Copy)]
struct PrefixToken {
    kind: ReaderPrefix,
    span: ByteSpan,
}

#[derive(Debug, Clone, Copy)]
enum SkipFrame {
    Form {
        missing_at: usize,
    },
    List {
        open_pos: ByteOffset,
        expected_close: u8,
    },
}

impl<'a> Parser<'a> {
    pub(in crate::sexpr) fn new(input: &'a str) -> Self {
        Self::with_dialect(input, Dialect::Unknown)
    }

    pub(in crate::sexpr) fn with_dialect(input: &'a str, dialect: Dialect) -> Self {
        let root = Node {
            kind: NodeKind::Root,
            delimiter: None,
            reader_prefixes: Vec::new(),
            reader_prefix_spans: Vec::new(),
            parent: None,
            children: Vec::new(),
            span: ByteSpan::new(ByteOffset::new(0), ByteOffset::new(input.len())),
            open: None,
            close: None,
            symbol_offset: 0,
            opaque_reader_form: false,
        };
        Self {
            input,
            bytes: input.as_bytes(),
            pos: ByteOffset::new(0),
            nodes: vec![root],
            stack: vec![NodeId::ROOT],
            comments: Vec::new(),
            policy: DialectReaderPolicy::new(dialect),
            suppress_depth: 0,
        }
    }

    pub(in crate::sexpr) fn parse(&mut self) -> std::result::Result<SyntaxTree, ParseError> {
        while self.pos.get() < self.bytes.len() {
            self.skip_trivia()?;
            if self.pos.get() >= self.bytes.len() {
                break;
            }
            self.form()?;
        }
        if self.stack.len() > 1 {
            if let Some(open) = self
                .stack
                .last()
                .and_then(|node_id| self.nodes.get(node_id.get()))
                .and_then(|node| node.open)
            {
                return Err(ParseError::UnclosedList(open.get()));
            }
            return Err(ParseError::UnclosedList(self.pos.get()));
        }
        Ok(SyntaxTree {
            nodes: std::mem::take(&mut self.nodes),
            comments: std::mem::take(&mut self.comments),
            source: self.input.to_string(),
        })
    }

    pub(in crate::sexpr) fn repair_unclosed_lists(
        &mut self,
    ) -> std::result::Result<String, ParseError> {
        match self.parse() {
            Ok(_) => Ok(self.input.to_owned()),
            Err(ParseError::UnclosedList(_)) => {
                let mut repaired = self.input.to_owned();
                for node_id in self.stack.iter().skip(1).rev() {
                    let delimiter = self.nodes[node_id.get()]
                        .delimiter
                        .expect("parser stack contains only lists after the root");
                    repaired.push(delimiter.close());
                }
                Ok(repaired)
            }
            Err(error) => Err(error),
        }
    }

    fn skip_trivia(&mut self) -> std::result::Result<(), ParseError> {
        loop {
            while self.pos.get() < self.bytes.len()
                && self.policy.is_whitespace(self.current_byte())
            {
                self.advance();
            }
            if self.pos.get() < self.bytes.len() && self.current_byte_is_block_comment() {
                let start = self.pos;
                self.skip_block_comment()?;
                self.record_comment(start, self.pos);
                continue;
            }
            if self.pos.get() < self.bytes.len() {
                if let Some(width) = self.policy.line_comment_width(self.bytes, self.pos.get()) {
                    let start = self.pos;
                    self.advance_by(width);
                    while self.pos.get() < self.bytes.len() && self.current_byte() != b'\n' {
                        self.advance();
                    }
                    self.record_comment(start, self.pos);
                    continue;
                }
            }
            break;
        }
        Ok(())
    }

    /// Records the byte range `[start, end)` as a comment, unless we are inside a
    /// `#;` datum comment whose enclosing span already covers it.
    fn record_comment(&mut self, start: ByteOffset, end: ByteOffset) {
        if self.suppress_depth > 0 || end.get() <= start.get() {
            return;
        }
        let text = self.input[start.get()..end.get()].to_string();
        let own_line = self.is_line_start(start.get());
        self.comments.push(Comment {
            span: ByteSpan::new(start, end),
            text,
            own_line,
        });
    }

    /// Returns `true` when only whitespace precedes byte `start` on its line.
    fn is_line_start(&self, start: usize) -> bool {
        let mut index = start;
        while index > 0 {
            let byte = self.bytes[index - 1];
            if byte == b'\n' {
                return true;
            }
            if self.policy.is_whitespace(byte) {
                index -= 1;
            } else {
                return false;
            }
        }
        true
    }

    fn open_list_with_prefixes(&mut self, prefixes: Vec<PrefixToken>) {
        let Some(&parent) = self.stack.last() else {
            debug_assert!(false, "parser stack unexpectedly empty when opening list");
            return;
        };
        let id = NodeId::new(self.nodes.len());
        let Some(delimiter) = self.policy.delimiter_from_open(self.current_byte()) else {
            debug_assert!(
                false,
                "open_list_with_prefixes called on non-opening delimiter"
            );
            return;
        };
        let start = prefixes
            .first()
            .map(|prefix| prefix.span.start())
            .unwrap_or(self.pos);
        let reader_prefixes = prefixes.iter().map(|prefix| prefix.kind).collect();
        let reader_prefix_spans = prefixes.iter().map(|prefix| prefix.span).collect();
        self.nodes.push(Node {
            kind: NodeKind::List,
            delimiter: Some(delimiter),
            reader_prefixes,
            reader_prefix_spans,
            parent: Some(parent),
            children: Vec::new(),
            span: ByteSpan::new(start, ByteOffset::new(self.pos.get() + 1)),
            open: Some(self.pos),
            close: None,
            symbol_offset: 0,
            opaque_reader_form: false,
        });
        self.nodes[parent.get()].children.push(id);
        self.stack.push(id);
        self.advance();
    }

    fn close_list(&mut self) -> std::result::Result<(), ParseError> {
        let Some(delimiter) = self.policy.delimiter_from_close(self.current_byte()) else {
            debug_assert!(false, "close_list called on non-closing delimiter");
            return Ok(());
        };
        if self.stack.len() == 1 {
            return Err(ParseError::UnexpectedClose {
                delimiter: delimiter.close(),
                position: self.pos.get(),
            });
        }
        let Some(&current) = self.stack.last() else {
            debug_assert!(false, "parser stack unexpectedly empty when closing list");
            return Err(ParseError::UnexpectedClose {
                delimiter: delimiter.close(),
                position: self.pos.get(),
            });
        };
        let expected = self.nodes[current.get()]
            .delimiter
            .map(Delimiter::close)
            .unwrap_or_else(|| {
                debug_assert!(false, "list node missing delimiter");
                delimiter.close()
            });
        if delimiter.close() != expected {
            return Err(ParseError::MismatchedClose {
                found: delimiter.close(),
                expected,
                position: self.pos.get(),
            });
        }
        let Some(id) = self.stack.pop() else {
            debug_assert!(false, "parser stack unexpectedly empty while popping list");
            return Err(ParseError::UnexpectedClose {
                delimiter: delimiter.close(),
                position: self.pos.get(),
            });
        };
        self.nodes[id.get()].span = ByteSpan::new(
            self.nodes[id.get()].span.start(),
            ByteOffset::new(self.pos.get() + 1),
        );
        self.nodes[id.get()].close = Some(self.pos);
        self.advance();
        Ok(())
    }

    fn atom_string_with_prefixes(
        &mut self,
        prefixes: Vec<PrefixToken>,
    ) -> std::result::Result<(), ParseError> {
        let start = self.pos;
        self.advance();
        let mut escaped = false;
        while self.pos.get() < self.bytes.len() {
            let byte = self.current_byte();
            self.advance();
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                self.push_atom(prefixes, start, self.pos);
                return Ok(());
            }
        }
        Err(ParseError::UnterminatedString(start.get()))
    }

    fn atom_with_prefixes(
        &mut self,
        prefixes: Vec<PrefixToken>,
    ) -> std::result::Result<(), ParseError> {
        let start = self.pos;
        if self.policy.is_legacy()
            && matches!(
                self.policy
                    .classify_reader_macro(self.bytes, self.pos.get()),
                Some(ReaderMacro::MultiDatum { .. })
            )
        {
            self.advance_by(2);
            self.push_atom(prefixes, start, self.pos);
            return Ok(());
        }
        self.consume_atom_body()?;
        self.push_atom(prefixes, start, self.pos);
        Ok(())
    }

    fn consume_atom_body(&mut self) -> std::result::Result<(), ParseError> {
        self.consume_character_literal();
        while self.pos.get() < self.bytes.len() {
            let byte = self.current_byte();
            if self.policy.supports_single_escape() && byte == b'\\' {
                self.consume_single_escape()?;
                continue;
            }
            if self.policy.supports_bar_quoted_symbols() && byte == b'|' {
                self.consume_multiple_escape()?;
                continue;
            }
            if self.policy.is_atom_boundary(self.bytes, self.pos.get()) {
                break;
            }
            self.advance();
        }
        Ok(())
    }

    fn consume_character_literal(&mut self) {
        let Some(prefix_width) = self
            .policy
            .character_literal_prefix_width(self.bytes, self.pos.get())
        else {
            return;
        };
        self.advance_by(prefix_width);
        let Some(character) = self.input[self.pos.get()..].chars().next() else {
            return;
        };
        self.advance_by(character.len_utf8());
    }

    /// Consumes a Lisp single-escape (`\`) and the following character literally.
    ///
    /// This keeps character literals such as `#\[`, `#\)`, and `#\Space`, as well
    /// as escaped symbol constituents like `\(`, from being split at what would
    /// otherwise be a delimiter or whitespace boundary.
    const fn consume_single_escape(&mut self) -> std::result::Result<(), ParseError> {
        let start = self.pos.get();
        self.advance();
        if self.pos.get() >= self.bytes.len() {
            return Err(ParseError::DanglingSingleEscape(start));
        }
        self.advance();
        Ok(())
    }

    /// Consumes a Lisp multiple-escape (`|...|`) region and the following
    /// character literally, per CLHS 2.1.4.2.
    ///
    /// Every character inside the region, including whitespace and delimiters,
    /// is a literal symbol constituent rather than a token boundary, so `|Foo
    /// Bar|` scans as one atom instead of splitting at the space. A nested `\`
    /// still single-escapes the character that follows it.
    fn consume_multiple_escape(&mut self) -> std::result::Result<(), ParseError> {
        let start = self.pos;
        self.advance();
        while self.pos.get() < self.bytes.len() {
            let byte = self.current_byte();
            if byte == b'\\' {
                self.consume_single_escape()?;
                continue;
            }
            if byte == b'|' {
                self.advance();
                return Ok(());
            }
            self.advance();
        }
        Err(ParseError::UnterminatedSymbol(start.get()))
    }

    fn push_atom(&mut self, prefixes: Vec<PrefixToken>, start: ByteOffset, end: ByteOffset) {
        let Some(&parent) = self.stack.last() else {
            debug_assert!(false, "parser stack unexpectedly empty when pushing atom");
            return;
        };
        let id = NodeId::new(self.nodes.len());
        let span_start = prefixes
            .first()
            .map(|prefix| prefix.span.start())
            .unwrap_or(start);
        // `start` is the position after `consume_reader_prefixes` already ran
        // `skip_trivia()`, so this is the true start of the atom's own
        // content even when whitespace or a comment separates a reader
        // prefix from what it prefixes (`#' foo` is valid, if unusual, CL
        // syntax).
        let symbol_offset = start.get() - span_start.get();
        let reader_prefixes = prefixes.iter().map(|prefix| prefix.kind).collect();
        let reader_prefix_spans = prefixes.iter().map(|prefix| prefix.span).collect();
        self.nodes.push(Node {
            kind: NodeKind::Atom,
            delimiter: None,
            reader_prefixes,
            reader_prefix_spans,
            parent: Some(parent),
            children: Vec::new(),
            span: ByteSpan::new(span_start, end),
            open: None,
            close: None,
            symbol_offset,
            opaque_reader_form: false,
        });
        self.nodes[parent.get()].children.push(id);
    }

    fn push_opaque_reader_form(&mut self, start: ByteOffset, end: ByteOffset) {
        let Some(&parent) = self.stack.last() else {
            debug_assert!(
                false,
                "parser stack unexpectedly empty when pushing reader form"
            );
            return;
        };
        let id = NodeId::new(self.nodes.len());
        self.nodes.push(Node {
            kind: NodeKind::Atom,
            delimiter: None,
            reader_prefixes: Vec::new(),
            reader_prefix_spans: Vec::new(),
            parent: Some(parent),
            children: Vec::new(),
            span: ByteSpan::new(start, end),
            open: None,
            close: None,
            symbol_offset: 0,
            opaque_reader_form: true,
        });
        self.nodes[parent.get()].children.push(id);
    }

    fn form(&mut self) -> std::result::Result<(), ParseError> {
        if let Some(ReaderMacro::Discard { width }) = self
            .policy
            .classify_reader_macro(self.bytes, self.pos.get())
        {
            self.skip_reader_comment(width)?;
            return Ok(());
        }
        let mut prefixes = Vec::new();
        loop {
            prefixes.extend(self.consume_reader_prefixes()?);
            if self.pos.get() >= self.bytes.len() {
                break;
            }
            match self
                .policy
                .classify_reader_macro(self.bytes, self.pos.get())
            {
                Some(ReaderMacro::Discard { width }) => {
                    self.skip_reader_comment(width)?;
                    self.skip_trivia()?;
                }
                _ => break,
            }
        }
        if self.pos.get() >= self.bytes.len() {
            let missing_at = prefixes
                .first()
                .map(|prefix| prefix.span.start().get())
                .unwrap_or(self.pos.get());
            return Err(ParseError::MissingReaderForm(missing_at));
        }
        match self
            .policy
            .classify_reader_macro(self.bytes, self.pos.get())
        {
            Some(ReaderMacro::MultiDatum {
                width,
                payload_forms,
            }) if !self.policy.is_legacy() => {
                self.opaque_reader_form_with_prefixes(prefixes, width, payload_forms)?;
                return Ok(());
            }
            Some(ReaderMacro::UnsupportedDispatch { width }) => {
                return Err(self.unsupported_reader_error(width));
            }
            _ => {}
        }
        match self.current_byte() {
            byte if self.policy.delimiter_from_open(byte).is_some() => {
                self.open_list_with_prefixes(prefixes);
            }
            byte if self.policy.delimiter_from_close(byte).is_some() => self.close_list()?,
            byte if DialectReaderPolicy::is_raw_delimiter(byte) => {
                return Err(self.raw_delimiter_error());
            }
            b'"' => self.atom_string_with_prefixes(prefixes)?,
            _ => self.atom_with_prefixes(prefixes)?,
        }
        Ok(())
    }

    fn consume_reader_prefixes(&mut self) -> std::result::Result<Vec<PrefixToken>, ParseError> {
        let mut prefixes = Vec::new();
        while let Some(ReaderMacro::Prefix {
            semantic: kind,
            width,
        }) = self
            .policy
            .classify_reader_macro(self.bytes, self.pos.get())
        {
            let start = self.pos;
            self.advance_by(width);
            prefixes.push(PrefixToken {
                kind,
                span: ByteSpan::new(start, self.pos),
            });
            self.skip_trivia()?;
            if self.pos.get() >= self.bytes.len() {
                break;
            }
        }
        Ok(prefixes)
    }

    fn skip_reader_comment(&mut self, width: usize) -> std::result::Result<(), ParseError> {
        let start = self.pos;
        self.advance_by(width);
        self.suppress_depth += 1;
        let result = self.skip_form(start.get());
        self.suppress_depth -= 1;
        result?;
        // Only the outermost datum comment records; nested `#;`/`#_` are folded in.
        self.record_comment(start, self.pos);
        Ok(())
    }

    fn opaque_reader_form_with_prefixes(
        &mut self,
        prefixes: Vec<PrefixToken>,
        width: usize,
        payload_forms: usize,
    ) -> std::result::Result<(), ParseError> {
        let start = prefixes
            .first()
            .map(|prefix| prefix.span.start())
            .unwrap_or(self.pos);
        let reader_start = self.pos.get();
        self.advance_by(width);
        self.suppress_depth += 1;
        let result = (0..payload_forms).try_for_each(|_| self.skip_form(reader_start));
        self.suppress_depth -= 1;
        result?;
        self.push_opaque_reader_form(start, self.pos);
        Ok(())
    }

    fn skip_form(&mut self, missing_at: usize) -> std::result::Result<(), ParseError> {
        let mut frames = Vec::new();
        Self::push_skip_frame(&mut frames, SkipFrame::Form { missing_at }, self.pos.get())?;
        while let Some(frame) = frames.pop() {
            match frame {
                SkipFrame::Form { missing_at } => {
                    self.skip_trivia()?;
                    if self.pos.get() >= self.bytes.len() {
                        return Err(ParseError::MissingReaderForm(missing_at));
                    }

                    let mut prefix_start = None;
                    let mut additional_discarded_forms = 0;
                    while let Some(ReaderMacro::Prefix { semantic, width }) = self
                        .policy
                        .classify_reader_macro(self.bytes, self.pos.get())
                    {
                        prefix_start.get_or_insert(self.pos.get());
                        additional_discarded_forms +=
                            self.policy.additional_discarded_forms_for_prefix(semantic);
                        self.advance_by(width);
                        self.skip_trivia()?;
                        if self.pos.get() >= self.bytes.len() {
                            return Err(ParseError::MissingReaderForm(
                                prefix_start.unwrap_or(missing_at),
                            ));
                        }
                    }

                    for _ in 0..additional_discarded_forms {
                        Self::push_skip_frame(
                            &mut frames,
                            SkipFrame::Form {
                                missing_at: prefix_start.unwrap_or(missing_at),
                            },
                            self.pos.get(),
                        )?;
                    }

                    match self
                        .policy
                        .classify_reader_macro(self.bytes, self.pos.get())
                    {
                        Some(ReaderMacro::Discard { width }) => {
                            let comment_start = self.pos.get();
                            self.advance_by(width);
                            Self::push_skip_frame(
                                &mut frames,
                                SkipFrame::Form {
                                    missing_at: prefix_start.unwrap_or(missing_at),
                                },
                                comment_start,
                            )?;
                            Self::push_skip_frame(
                                &mut frames,
                                SkipFrame::Form {
                                    missing_at: comment_start,
                                },
                                comment_start,
                            )?;
                            continue;
                        }
                        Some(ReaderMacro::MultiDatum {
                            width,
                            payload_forms,
                        }) => {
                            let dispatch_start = self.pos.get();
                            self.advance_by(width);
                            for _ in 0..payload_forms {
                                Self::push_skip_frame(
                                    &mut frames,
                                    SkipFrame::Form {
                                        missing_at: dispatch_start,
                                    },
                                    dispatch_start,
                                )?;
                            }
                            continue;
                        }
                        Some(ReaderMacro::UnsupportedDispatch { width }) => {
                            return Err(self.unsupported_reader_error(width));
                        }
                        Some(ReaderMacro::Prefix { .. }) | None => {}
                    }

                    match self.current_byte() {
                        byte if self.policy.delimiter_from_open(byte).is_some() => {
                            let open_pos = self.pos;
                            let expected_close = self
                                .policy
                                .delimiter_from_open(byte)
                                .expect("opening delimiter checked above")
                                .close() as u8;
                            self.advance();
                            Self::push_skip_frame(
                                &mut frames,
                                SkipFrame::List {
                                    open_pos,
                                    expected_close,
                                },
                                open_pos.get(),
                            )?;
                        }
                        byte if self.policy.delimiter_from_close(byte).is_some() => {
                            let delimiter = self
                                .policy
                                .delimiter_from_close(byte)
                                .expect("closing delimiter checked above");
                            return Err(ParseError::UnexpectedClose {
                                delimiter: delimiter.close(),
                                position: self.pos.get(),
                            });
                        }
                        byte if DialectReaderPolicy::is_raw_delimiter(byte) => {
                            return Err(self.raw_delimiter_error());
                        }
                        b'"' => self.skip_string()?,
                        _ => self.skip_atom()?,
                    }
                }
                SkipFrame::List {
                    open_pos,
                    expected_close,
                } => {
                    self.skip_trivia()?;
                    if self.pos.get() >= self.bytes.len() {
                        return Err(ParseError::UnclosedList(open_pos.get()));
                    }
                    if self.current_byte() == expected_close {
                        self.advance();
                        continue;
                    }
                    Self::push_skip_frame(
                        &mut frames,
                        SkipFrame::List {
                            open_pos,
                            expected_close,
                        },
                        self.pos.get(),
                    )?;
                    Self::push_skip_frame(
                        &mut frames,
                        SkipFrame::Form {
                            missing_at: self.pos.get(),
                        },
                        self.pos.get(),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn push_skip_frame(
        frames: &mut Vec<SkipFrame>,
        frame: SkipFrame,
        position: usize,
    ) -> std::result::Result<(), ParseError> {
        if frames.len() >= MAX_DISCARDED_FORM_STACK_FRAMES {
            return Err(ParseError::ResourceLimitExceeded {
                position,
                limit: MAX_DISCARDED_FORM_STACK_FRAMES,
            });
        }
        frames
            .try_reserve(1)
            .map_err(|_| ParseError::ResourceLimitExceeded {
                position,
                limit: MAX_DISCARDED_FORM_STACK_FRAMES,
            })?;
        frames.push(frame);
        Ok(())
    }

    fn skip_string(&mut self) -> std::result::Result<(), ParseError> {
        let start = self.pos;
        self.advance();
        let mut escaped = false;
        while self.pos.get() < self.bytes.len() {
            let byte = self.current_byte();
            self.advance();
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                return Ok(());
            }
        }
        Err(ParseError::UnterminatedString(start.get()))
    }

    fn skip_block_comment(&mut self) -> std::result::Result<(), ParseError> {
        let start = self.pos;
        self.advance();
        self.advance();
        let mut depth = 1usize;
        while self.pos.get() < self.bytes.len() {
            if self.current_byte_is_block_comment() {
                depth += 1;
                self.advance();
                self.advance();
                continue;
            }
            if self.current_byte() == b'|' && self.peek_byte() == Some(b'#') {
                depth -= 1;
                self.advance();
                self.advance();
                if depth == 0 {
                    return Ok(());
                }
                continue;
            }
            self.advance();
        }
        Err(ParseError::UnterminatedBlockComment(start.get()))
    }

    fn skip_atom(&mut self) -> std::result::Result<(), ParseError> {
        self.consume_atom_body()
    }

    fn current_byte_is_block_comment(&self) -> bool {
        self.policy.supports_block_comments()
            && self.current_byte() == b'#'
            && self.peek_byte() == Some(b'|')
    }

    fn current_byte(&self) -> u8 {
        self.bytes[self.pos.get()]
    }

    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos.get() + 1).copied()
    }

    const fn advance(&mut self) {
        self.pos = ByteOffset::new(self.pos.get() + 1);
    }

    const fn advance_by(&mut self, width: usize) {
        self.pos = ByteOffset::new(self.pos.get() + width);
    }

    fn unsupported_reader_error(&self, width: usize) -> ParseError {
        let start = self.pos.get();
        let end = start.saturating_add(width).min(self.input.len());
        ParseError::UnsupportedReaderDispatch {
            dispatch: self.input[start..end].to_owned(),
            position: start,
        }
    }

    fn raw_delimiter_error(&self) -> ParseError {
        let start = self.pos.get();
        ParseError::UnexpectedClose {
            delimiter: self.input[start..]
                .chars()
                .next()
                .unwrap_or(char::REPLACEMENT_CHARACTER),
            position: start,
        }
    }
}
