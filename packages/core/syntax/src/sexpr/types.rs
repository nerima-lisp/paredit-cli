use super::error::{PathError, SpanError, SymbolError};
use std::fmt;
use std::ops::Range;
use std::str::FromStr;

/// A byte offset into the original source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(usize);

impl ByteOffset {
    /// Creates an offset from a raw byte index.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the raw byte index.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// A half-open byte range `[start, end)` inside the original source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteSpan {
    start: ByteOffset,
    end: ByteOffset,
}

impl ByteSpan {
    /// Creates a span from byte offsets.
    ///
    /// This constructor preserves the historical unchecked behavior. Use
    /// [`Self::try_new`] when input ordering is not trusted.
    #[must_use]
    pub const fn new(start: ByteOffset, end: ByteOffset) -> Self {
        Self { start, end }
    }

    /// Attempts to create a span from byte offsets.
    #[must_use]
    pub const fn try_new(start: ByteOffset, end: ByteOffset) -> Option<Self> {
        if start.get() <= end.get() {
            Some(Self { start, end })
        } else {
            None
        }
    }

    /// Returns the inclusive start boundary as a byte offset.
    #[must_use]
    pub const fn start(&self) -> ByteOffset {
        self.start
    }

    /// Returns the exclusive end boundary as a byte offset.
    #[must_use]
    pub const fn end(&self) -> ByteOffset {
        self.end
    }

    /// Returns the span length in bytes, saturating at zero for invalid order.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end.get().saturating_sub(self.start.get())
    }

    /// Returns `true` when the span covers no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns `true` when `offset` lies inside the half-open range.
    #[must_use]
    pub const fn contains(&self, offset: ByteOffset) -> bool {
        self.start.get() <= offset.get() && offset.get() < self.end.get()
    }

    /// Returns `true` when `inner` lies entirely inside this span.
    #[must_use]
    pub const fn contains_span(&self, inner: ByteSpan) -> bool {
        self.start.get() <= inner.start.get() && inner.end.get() <= self.end.get()
    }

    /// Exposes the span as a Rust range over byte indexes.
    #[must_use]
    pub const fn as_range(&self) -> Range<usize> {
        self.start.get()..self.end.get()
    }

    /// Validates that this span can safely index `input`.
    pub fn validate_against(&self, input: &str) -> Result<(), SpanError> {
        let start = self.start.get();
        let end = self.end.get();
        if start > end {
            return Err(SpanError::StartExceedsEnd { start, end });
        }
        if end > input.len() {
            return Err(SpanError::EndExceedsInput {
                end,
                length: input.len(),
            });
        }
        if !input.is_char_boundary(start) || !input.is_char_boundary(end) {
            return Err(SpanError::NotCharBoundary);
        }
        Ok(())
    }

    /// Borrows the substring covered by this byte span.
    #[must_use]
    pub fn slice<'a>(&self, input: &'a str) -> &'a str {
        &input[self.as_range()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_new_rejects_reversed_offsets() {
        assert_eq!(
            ByteSpan::try_new(ByteOffset::new(8), ByteOffset::new(3)),
            None
        );
    }

    #[test]
    fn new_preserves_unchecked_compatibility() {
        assert_eq!(
            ByteSpan::new(ByteOffset::new(8), ByteOffset::new(3)),
            ByteSpan::new(ByteOffset::new(8), ByteOffset::new(3))
        );
    }

    #[test]
    fn validate_against_rejects_invalid_ranges() {
        let input = "a\u{00e9}b";
        assert!(
            ByteSpan::new(ByteOffset::new(4), ByteOffset::new(2))
                .validate_against(input)
                .is_err()
        );
        assert!(
            ByteSpan::new(ByteOffset::new(0), ByteOffset::new(99))
                .validate_against(input)
                .is_err()
        );
        assert!(
            ByteSpan::new(ByteOffset::new(2), ByteOffset::new(3))
                .validate_against(input)
                .is_err()
        );
    }

    #[test]
    fn validate_against_accepts_empty_and_unicode_boundaries() {
        let input = "a\u{00e9}b";
        assert!(
            ByteSpan::new(ByteOffset::new(1), ByteOffset::new(3))
                .validate_against(input)
                .is_ok()
        );
        assert!(
            ByteSpan::new(ByteOffset::new(3), ByteOffset::new(3))
                .validate_against(input)
                .is_ok()
        );
    }
}

/// A child position within one list node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChildIndex(usize);

impl ChildIndex {
    /// Creates a child index from its zero-based position.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the zero-based child position.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// A zero-based path from the virtual root to a nested expression.
///
/// # Examples
///
/// ```
/// use std::str::FromStr;
///
/// use paredit_core_syntax::sexpr::ExpressionPath;
///
/// let path = ExpressionPath::from_str("0.2")?;
/// assert_eq!(path.to_raw_indexes(), vec![0, 2]);
/// assert_eq!(path.child(1).to_string(), "0.2.1");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExpressionPath(Vec<ChildIndex>);

#[derive(Debug, Clone, Copy)]
pub struct NonEmptyExpressionPath<'a>(&'a [ChildIndex]);

impl<'a> TryFrom<&'a ExpressionPath> for NonEmptyExpressionPath<'a> {
    type Error = ();

    fn try_from(path: &'a ExpressionPath) -> Result<Self, Self::Error> {
        if path.0.is_empty() {
            Err(())
        } else {
            Ok(Self(&path.0))
        }
    }
}

impl NonEmptyExpressionPath<'_> {
    #[must_use]
    pub fn indexes(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.0.iter().map(|index| index.get())
    }
}

/// Backwards-compatible alias for tree paths used by the CLI and API.
pub type Path = ExpressionPath;

impl ExpressionPath {
    /// Builds a path that points to one root-level child expression.
    #[must_use]
    pub fn root_child(index: usize) -> Self {
        Self::from_indexes(vec![index])
    }

    /// Builds a path from raw zero-based child indexes.
    pub fn from_indexes(indexes: Vec<usize>) -> Self {
        Self(indexes.into_iter().map(ChildIndex::new).collect())
    }

    /// Returns the typed child indexes that form this path.
    #[must_use]
    pub fn indexes(&self) -> &[ChildIndex] {
        &self.0
    }

    /// Clones this path into raw zero-based indexes.
    #[must_use]
    pub fn to_raw_indexes(&self) -> Vec<usize> {
        self.0.iter().map(|index| index.get()).collect()
    }

    /// Returns a new path extended by one child position.
    #[must_use]
    pub fn child(&self, index: usize) -> Self {
        let mut indexes = self.0.clone();
        indexes.push(ChildIndex::new(index));
        Self(indexes)
    }

    /// Returns the parent path, or `None` for the virtual root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let mut indexes = self.0.clone();
        indexes.pop()?;
        Some(Self(indexes))
    }

    /// Returns a new path extended by a fixed list of child positions.
    #[must_use]
    pub fn descendant<const N: usize>(&self, indexes: [usize; N]) -> Self {
        let mut path = self.clone();
        for index in indexes {
            path = path.child(index);
        }
        path
    }
}

impl FromStr for ExpressionPath {
    type Err = PathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            return Ok(Self(Vec::new()));
        }
        let mut indexes = Vec::new();
        for part in s.split('.') {
            indexes.push(ChildIndex::new(part.parse::<usize>().map_err(|_| {
                PathError::InvalidSegment {
                    segment: part.to_owned(),
                }
            })?));
        }
        Ok(Self(indexes))
    }
}

impl fmt::Display for ExpressionPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, index) in self.0.iter().enumerate() {
            if position > 0 {
                write!(f, ".")?;
            }
            write!(f, "{}", index.get())?;
        }
        Ok(())
    }
}

/// A validated Lisp-family symbol name without reader delimiters or whitespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolName(String);

impl SymbolName {
    /// Validates and stores a symbol name for rename and selection APIs.
    pub fn new(value: impl Into<String>) -> Result<Self, SymbolError> {
        let value = value.into();
        if value.is_empty() {
            return Err(SymbolError::Empty);
        }
        if value.bytes().any(is_symbol_boundary) || value.contains('"') {
            return Err(SymbolError::ReaderDelimiterOrWhitespace { value });
        }
        Ok(Self(value))
    }

    /// Returns the original symbol text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for SymbolName {
    type Err = SymbolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl fmt::Display for SymbolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

impl NodeId {
    pub(in crate::sexpr) const ROOT: Self = Self(0);

    pub(in crate::sexpr) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(in crate::sexpr) const fn get(self) -> usize {
        self.0
    }
}

/// The list delimiter used by a parsed expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delimiter {
    Paren,
    Bracket,
    Brace,
}

impl Delimiter {
    pub(in crate::sexpr) const fn from_open(byte: u8) -> Option<Self> {
        match byte {
            b'(' => Some(Self::Paren),
            b'[' => Some(Self::Bracket),
            b'{' => Some(Self::Brace),
            _ => None,
        }
    }

    pub(in crate::sexpr) const fn from_close(byte: u8) -> Option<Self> {
        match byte {
            b')' => Some(Self::Paren),
            b']' => Some(Self::Bracket),
            b'}' => Some(Self::Brace),
            _ => None,
        }
    }

    pub(in crate::sexpr) const fn open(self) -> char {
        match self {
            Self::Paren => '(',
            Self::Bracket => '[',
            Self::Brace => '{',
        }
    }

    pub(in crate::sexpr) const fn close(self) -> char {
        match self {
            Self::Paren => ')',
            Self::Bracket => ']',
            Self::Brace => '}',
        }
    }
}

pub(in crate::sexpr) const fn is_symbol_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'(' | b')' | b'[' | b']' | b'{' | b'}' | b';')
}
