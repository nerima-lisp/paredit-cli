//! Gitignore-compatible glob patterns.
//!
//! One pattern language serves three separate features that would otherwise
//! each grow their own: `.gitignore` files, `.pareditignore` files, and the
//! `--include` / `--exclude-glob` command-line filters. Keeping them on one
//! implementation is not only less code — it means a pattern a user already
//! knows from git behaves identically when typed as a flag.
//!
//! The semantics are those documented in `gitignore(5)`:
//!
//! * a leading `!` negates the pattern;
//! * a trailing `/` restricts the match to directories;
//! * a `/` anywhere except the end anchors the pattern to the base directory,
//!   and otherwise the pattern matches a path component at any depth;
//! * `*` and `?` never cross a `/`, while a `**` that occupies a whole
//!   component matches zero or more components;
//! * `[...]` is a character class, negated with a leading `!` or `^`;
//! * `\` escapes the next character.
//!
//! Matching runs as a forward reachability sweep over path components rather
//! than as backtracking recursion. `a/**/b/**/c/**/d` against a deep path is
//! the case that makes the recursive formulation exponential, and a hostile
//! `.gitignore` in a scanned tree is exactly the kind of input this tool has to
//! survive.

use std::fmt;

/// The longest pattern text that will be compiled.
///
/// Bounded for the same reason every other input here is: a pattern arrives
/// from a file inside the tree being scanned, so it is untrusted.
const MAX_PATTERN_BYTES: usize = 4_096;

/// The most patterns one [`GlobSet`] will hold.
const MAX_PATTERNS: usize = 65_536;

/// What a compiled pattern says about one path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlobDecision {
    /// No pattern in the set matched.
    Unmatched,
    /// The last matching pattern was a plain one.
    Matched,
    /// The last matching pattern was negated with `!`.
    Negated,
}

impl GlobDecision {
    /// Whether the decision selects the path.
    #[must_use]
    pub const fn is_match(self) -> bool {
        matches!(self, Self::Matched)
    }
}

/// Why a pattern could not be compiled.
///
/// Callers surface these with the originating file and line, so the variant
/// carries only what the pattern text itself got wrong.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GlobParseError {
    /// The pattern exceeded `MAX_PATTERN_BYTES`.
    TooLong { actual: usize, maximum: usize },
    /// A `[` was never closed.
    UnterminatedClass,
    /// A trailing `\` had nothing to escape.
    DanglingEscape,
    /// The pattern was empty once its `!`, `/` and whitespace were stripped.
    Empty,
}

impl fmt::Display for GlobParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { actual, maximum } => write!(
                formatter,
                "glob pattern length limit exceeded: {actual} bytes exceeds maximum {maximum}"
            ),
            Self::UnterminatedClass => formatter.write_str("glob pattern has an unterminated ["),
            Self::DanglingEscape => formatter.write_str("glob pattern ends with a trailing \\"),
            Self::Empty => formatter.write_str("glob pattern is empty"),
        }
    }
}

impl std::error::Error for GlobParseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ClassItem {
    Char(char),
    Range(char, char),
}

impl ClassItem {
    const fn contains(&self, candidate: char) -> bool {
        match *self {
            Self::Char(character) => character == candidate,
            Self::Range(low, high) => low <= candidate && candidate <= high,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Literal(char),
    AnyChar,
    AnyRun,
    Class {
        negated: bool,
        items: Vec<ClassItem>,
    },
}

impl Token {
    fn matches(&self, candidate: char) -> bool {
        match self {
            Self::Literal(character) => *character == candidate,
            Self::AnyChar => true,
            // Only reached through the sweep in `match_component`, which
            // handles `*` before consulting this.
            Self::AnyRun => true,
            Self::Class { negated, items } => {
                items.iter().any(|item| item.contains(candidate)) != *negated
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Segment {
    /// A `**` component: zero or more path components.
    AnyDepth,
    /// One path component, matched token by token.
    Component(Vec<Token>),
}

/// One compiled `.gitignore`-style pattern.
#[derive(Clone, Debug)]
pub struct GlobPattern {
    segments: Vec<Segment>,
    anchored: bool,
    directory_only: bool,
    negated: bool,
    source: String,
}

impl GlobPattern {
    /// Compiles one pattern.
    ///
    /// Returns `Ok(None)` for a line that carries no pattern at all — blank, or
    /// a `#` comment. Those are not errors: an ignore file is mostly comments,
    /// and refusing to read one because it has a blank line would be absurd.
    pub fn parse(line: &str) -> Result<Option<Self>, GlobParseError> {
        if line.len() > MAX_PATTERN_BYTES {
            return Err(GlobParseError::TooLong {
                actual: line.len(),
                maximum: MAX_PATTERN_BYTES,
            });
        }

        let source = line.to_owned();
        let trimmed = strip_trailing_unescaped_whitespace(line.trim_start_matches(' '));
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return Ok(None);
        }

        let (negated, rest) = match trimmed.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, trimmed),
        };

        // `gitignore(5)`: a trailing separator restricts the pattern to
        // directories and is not itself a separator for the anchoring rule.
        let (directory_only, rest) = match rest.strip_suffix('/') {
            Some(stripped) => (true, stripped),
            None => (false, rest),
        };

        let anchored = rest.contains('/');
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if rest.is_empty() {
            return Err(GlobParseError::Empty);
        }

        let mut segments = Vec::new();
        for (index, component) in rest.split('/').enumerate() {
            let is_last = index + 1 == rest.split('/').count();
            if component == "**" {
                segments.push(Segment::AnyDepth);
                // A trailing `/**` "matches everything inside", so it must
                // consume at least one component: `a/**` covers `a/b` but not
                // `a` itself, which the bare pattern `a` already covers.
                if is_last && index > 0 {
                    segments.push(Segment::Component(vec![Token::AnyRun]));
                }
                continue;
            }
            segments.push(Segment::Component(parse_component(component)?));
        }

        Ok(Some(Self {
            segments,
            anchored,
            directory_only,
            negated,
            source,
        }))
    }

    /// The pattern text exactly as it was written.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Whether this pattern was written with a leading `!`.
    #[must_use]
    pub const fn is_negated(&self) -> bool {
        self.negated
    }

    /// Whether this pattern only applies to directories.
    #[must_use]
    pub const fn is_directory_only(&self) -> bool {
        self.directory_only
    }

    /// Tests `relative` — a `/`-separated path with no leading separator —
    /// against this pattern.
    ///
    /// `is_directory` decides whether a `dir/` pattern applies. A caller that
    /// does not know passes `false`, which is the conservative answer: a
    /// directory-only pattern then does not match, and nothing is skipped by
    /// accident.
    #[must_use]
    pub fn matches(&self, relative: &str, is_directory: bool) -> bool {
        if self.directory_only && !is_directory {
            return false;
        }

        let components = relative
            .split('/')
            .filter(|component| !component.is_empty())
            .collect::<Vec<_>>();
        if components.is_empty() {
            return false;
        }

        if self.anchored {
            return matches_segments(&self.segments, &components);
        }

        // An unanchored pattern matches at any depth, which is exactly what an
        // implicit leading `**/` means. Rather than allocate a modified segment
        // list per pattern, try every suffix: the segment lists here are short
        // and the component count is bounded by the traversal depth.
        (0..components.len()).any(|start| matches_segments(&self.segments, &components[start..]))
    }
}

/// An ordered set of patterns, evaluated last-match-wins.
///
/// The ordering rule is git's: within one ignore file the last pattern that
/// matches decides, so a `!keep.lisp` written after `*.lisp` re-includes the
/// file. Callers that stack several files evaluate them outermost first and let
/// the innermost speak last.
#[derive(Clone, Debug, Default)]
pub struct GlobSet {
    patterns: Vec<GlobPattern>,
}

impl GlobSet {
    /// An empty set, which never matches.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Compiles one pattern per line, skipping blanks and `#` comments.
    pub fn parse_lines<'a>(
        lines: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, GlobParseError> {
        let mut set = Self::new();
        for line in lines {
            set.push_line(line)?;
        }
        Ok(set)
    }

    /// Compiles the contents of one ignore file.
    pub fn parse_file(contents: &str) -> Result<Self, GlobParseError> {
        Self::parse_lines(contents.lines())
    }

    /// Adds one pattern line, ignoring blanks and comments.
    pub fn push_line(&mut self, line: &str) -> Result<(), GlobParseError> {
        if self.patterns.len() >= MAX_PATTERNS {
            return Err(GlobParseError::TooLong {
                actual: self.patterns.len(),
                maximum: MAX_PATTERNS,
            });
        }
        if let Some(pattern) = GlobPattern::parse(line)? {
            self.patterns.push(pattern);
        }
        Ok(())
    }

    /// Whether the set holds no patterns at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// How many patterns the set holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// The compiled patterns, in the order they were written.
    #[must_use]
    pub fn patterns(&self) -> &[GlobPattern] {
        &self.patterns
    }

    /// Evaluates `relative` against every pattern, last match winning.
    #[must_use]
    pub fn decide(&self, relative: &str, is_directory: bool) -> GlobDecision {
        let mut decision = GlobDecision::Unmatched;
        for pattern in &self.patterns {
            if pattern.matches(relative, is_directory) {
                decision = if pattern.negated {
                    GlobDecision::Negated
                } else {
                    GlobDecision::Matched
                };
            }
        }
        decision
    }

    /// Whether any pattern selects `relative`, after negations.
    #[must_use]
    pub fn is_match(&self, relative: &str, is_directory: bool) -> bool {
        self.decide(relative, is_directory).is_match()
    }

    /// Whether any pattern could still match something *below* `relative`.
    ///
    /// Directory pruning needs this: `src/**/*.lisp` does not match `src`, but
    /// pruning `src` because of that would lose every file the pattern exists
    /// to find. A conservative `true` only costs a directory visit.
    #[must_use]
    pub fn could_match_descendant(&self, relative: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern_could_match_descendant(pattern, relative))
    }
}

fn pattern_could_match_descendant(pattern: &GlobPattern, relative: &str) -> bool {
    if !pattern.anchored {
        // An unanchored pattern can match a basename at any depth.
        return true;
    }

    let components = relative
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    // `relative` is a prefix of some deeper path, so ask whether the pattern's
    // segments can consume exactly these components with segments left over.
    let mut reachable = vec![false; components.len() + 1];
    reachable[0] = true;
    for segment in &pattern.segments {
        if reachable[components.len()] {
            // The pattern has already consumed the whole prefix and still has
            // segments to spend, which is what a descendant match needs.
            return true;
        }
        reachable = advance(segment, &reachable, &components);
        if !reachable.iter().any(|value| *value) {
            return false;
        }
    }
    false
}

fn matches_segments(segments: &[Segment], components: &[&str]) -> bool {
    let mut reachable = vec![false; components.len() + 1];
    reachable[0] = true;
    for segment in segments {
        reachable = advance(segment, &reachable, components);
        if !reachable.iter().any(|value| *value) {
            return false;
        }
    }
    reachable[components.len()]
}

fn advance(segment: &Segment, reachable: &[bool], components: &[&str]) -> Vec<bool> {
    let mut next = vec![false; components.len() + 1];
    match segment {
        Segment::AnyDepth => {
            let mut carried = false;
            for (index, slot) in next.iter_mut().enumerate() {
                carried |= reachable[index];
                *slot = carried;
            }
        }
        Segment::Component(tokens) => {
            for index in 0..components.len() {
                if reachable[index] && match_component(tokens, components[index]) {
                    next[index + 1] = true;
                }
            }
        }
    }
    next
}

/// Matches one path component against a token list.
///
/// The single-`*` backtrack point is the standard linear-space wildcard sweep:
/// remember where the last `*` was, and on a mismatch let it consume one more
/// character instead of recursing.
fn match_component(tokens: &[Token], component: &str) -> bool {
    let characters = component.chars().collect::<Vec<_>>();
    let mut token_index = 0;
    let mut char_index = 0;
    let mut star_token = None;
    let mut star_char = 0;

    while char_index < characters.len() {
        match tokens.get(token_index) {
            Some(Token::AnyRun) => {
                star_token = Some(token_index);
                star_char = char_index;
                token_index += 1;
            }
            Some(token) if token.matches(characters[char_index]) => {
                token_index += 1;
                char_index += 1;
            }
            _ => match star_token {
                Some(index) => {
                    token_index = index + 1;
                    star_char += 1;
                    char_index = star_char;
                }
                None => return false,
            },
        }
    }

    while matches!(tokens.get(token_index), Some(Token::AnyRun)) {
        token_index += 1;
    }
    token_index == tokens.len()
}

fn parse_component(component: &str) -> Result<Vec<Token>, GlobParseError> {
    let mut tokens = Vec::new();
    let mut characters = component.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                let escaped = characters.next().ok_or(GlobParseError::DanglingEscape)?;
                tokens.push(Token::Literal(escaped));
            }
            '?' => tokens.push(Token::AnyChar),
            '*' => {
                // `**` inside a component (`a**b`) is documented as having no
                // special meaning, so collapsing a run of stars is correct.
                while characters.peek() == Some(&'*') {
                    characters.next();
                }
                tokens.push(Token::AnyRun);
            }
            '[' => tokens.push(parse_class(&mut characters)?),
            other => tokens.push(Token::Literal(other)),
        }
    }

    Ok(tokens)
}

fn parse_class(
    characters: &mut std::iter::Peekable<std::str::Chars<'_>>,
) -> Result<Token, GlobParseError> {
    let negated = matches!(characters.peek(), Some('!' | '^'));
    if negated {
        characters.next();
    }

    let mut items = Vec::new();
    let mut first = true;
    loop {
        let character = characters.next().ok_or(GlobParseError::UnterminatedClass)?;
        // A `]` in the first position is a literal, as in POSIX bracket
        // expressions; anywhere else it closes the class.
        if character == ']' && !first {
            break;
        }
        first = false;

        let low = if character == '\\' {
            characters.next().ok_or(GlobParseError::DanglingEscape)?
        } else {
            character
        };

        if characters.peek() == Some(&'-') {
            let mut lookahead = characters.clone();
            lookahead.next();
            match lookahead.peek() {
                // `a-]` is a literal `a`, a literal `-`, then the terminator.
                Some(&']') | None => items.push(ClassItem::Char(low)),
                Some(_) => {
                    characters.next();
                    let high = characters.next().ok_or(GlobParseError::UnterminatedClass)?;
                    let high = if high == '\\' {
                        characters.next().ok_or(GlobParseError::DanglingEscape)?
                    } else {
                        high
                    };
                    items.push(ClassItem::Range(low, high));
                }
            }
            continue;
        }

        items.push(ClassItem::Char(low));
    }

    Ok(Token::Class { negated, items })
}

/// Strips trailing spaces that were not escaped with `\`.
///
/// `gitignore(5)` keeps a trailing space only when it is escaped, which is the
/// only way to write a pattern for a filename that really does end in one.
fn strip_trailing_unescaped_whitespace(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t') {
        let mut backslashes = 0;
        let mut index = end - 1;
        while index > 0 && bytes[index - 1] == b'\\' {
            backslashes += 1;
            index -= 1;
        }
        if backslashes % 2 == 1 {
            break;
        }
        end -= 1;
    }
    // `\r` from a CRLF ignore file is never part of the pattern.
    let trimmed = &line[..end];
    trimmed.strip_suffix('\r').unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{GlobDecision, GlobPattern, GlobSet};

    fn matches(pattern: &str, path: &str) -> bool {
        GlobPattern::parse(pattern)
            .expect("pattern compiles")
            .expect("pattern is not a comment")
            .matches(path, false)
    }

    fn matches_dir(pattern: &str, path: &str) -> bool {
        GlobPattern::parse(pattern)
            .expect("pattern compiles")
            .expect("pattern is not a comment")
            .matches(path, true)
    }

    #[test]
    fn bare_name_matches_at_any_depth() {
        assert!(matches("target", "target"));
        assert!(matches("target", "a/b/target"));
        assert!(!matches("target", "a/targets"));
    }

    #[test]
    fn a_slash_anywhere_anchors_the_pattern() {
        assert!(matches("src/main.lisp", "src/main.lisp"));
        assert!(!matches("src/main.lisp", "lib/src/main.lisp"));
        assert!(matches("/root.lisp", "root.lisp"));
        assert!(!matches("/root.lisp", "nested/root.lisp"));
    }

    #[test]
    fn star_does_not_cross_a_separator() {
        assert!(matches("*.lisp", "a/b/c.lisp"));
        assert!(matches("src/*.lisp", "src/c.lisp"));
        assert!(!matches("src/*.lisp", "src/nested/c.lisp"));
    }

    #[test]
    fn double_star_component_spans_directories() {
        assert!(matches("src/**/*.lisp", "src/c.lisp"));
        assert!(matches("src/**/*.lisp", "src/a/b/c.lisp"));
        assert!(matches("**/vendor/*.lisp", "a/vendor/c.lisp"));
    }

    #[test]
    fn trailing_double_star_requires_a_descendant() {
        assert!(matches("src/**", "src/a.lisp"));
        assert!(matches("src/**", "src/a/b.lisp"));
        assert!(!matches("src/**", "src"));
    }

    #[test]
    fn a_trailing_slash_restricts_the_match_to_directories() {
        assert!(!matches("build/", "build"));
        assert!(matches_dir("build/", "build"));
        assert!(matches_dir("build/", "a/build"));
    }

    #[test]
    fn character_classes_and_negation() {
        assert!(matches("file[0-9].lisp", "file7.lisp"));
        assert!(!matches("file[0-9].lisp", "filex.lisp"));
        assert!(matches("file[!0-9].lisp", "filex.lisp"));
        assert!(!matches("file[!0-9].lisp", "file7.lisp"));
        assert!(matches("file[]x].lisp", "file].lisp"));
    }

    #[test]
    fn escapes_take_the_next_character_literally() {
        assert!(matches(r"a\*b.lisp", "a*b.lisp"));
        assert!(!matches(r"a\*b.lisp", "axb.lisp"));
        assert!(matches(r"\!literal.lisp", "!literal.lisp"));
    }

    #[test]
    fn comments_and_blank_lines_compile_to_nothing() {
        assert!(GlobPattern::parse("").expect("blank compiles").is_none());
        assert!(GlobPattern::parse("   ").expect("spaces compile").is_none());
        assert!(
            GlobPattern::parse("# comment")
                .expect("comment compiles")
                .is_none()
        );
        assert!(
            GlobPattern::parse(r"\#not-a-comment")
                .expect("escaped hash compiles")
                .is_some()
        );
    }

    #[test]
    fn trailing_spaces_are_stripped_unless_escaped() {
        let pattern = GlobPattern::parse("name.lisp   ")
            .expect("compiles")
            .expect("is a pattern");
        assert!(pattern.matches("name.lisp", false));

        let pattern = GlobPattern::parse(r"name\ ")
            .expect("compiles")
            .expect("is a pattern");
        assert!(pattern.matches("name ", false));
    }

    #[test]
    fn the_last_matching_pattern_decides() {
        let set = GlobSet::parse_file("*.lisp\n!keep.lisp\n").expect("compiles");
        assert_eq!(set.decide("drop.lisp", false), GlobDecision::Matched);
        assert_eq!(set.decide("keep.lisp", false), GlobDecision::Negated);
        assert_eq!(set.decide("other.txt", false), GlobDecision::Unmatched);
    }

    #[test]
    fn descendant_lookahead_keeps_directories_that_still_could_match() {
        let set = GlobSet::parse_file("src/**/*.lisp\n").expect("compiles");
        assert!(!set.is_match("src", true));
        assert!(set.could_match_descendant("src"));
        assert!(!set.could_match_descendant("lib"));
    }

    #[test]
    fn deeply_nested_double_stars_stay_cheap() {
        // The recursive formulation of this match is exponential; the sweep is
        // linear in components times segments.
        let set = GlobSet::parse_file("a/**/b/**/c/**/d/**/e/**/f.lisp\n").expect("compiles");
        let deep = std::iter::repeat_n("x", 200).collect::<Vec<_>>().join("/");
        assert!(!set.is_match(&format!("a/{deep}"), false));
    }

    #[test]
    fn crlf_ignore_files_do_not_leak_a_carriage_return() {
        let set = GlobSet::parse_file("target\r\nbuild\r\n").expect("compiles");
        assert!(set.is_match("target", true));
        assert!(set.is_match("build", true));
    }
}
