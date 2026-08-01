//! Turning a selector into the forms it names.
//!
//! Every way of pointing at a form funnels through [`resolve`], which returns
//! zero or more [`SelectorTarget`]s in source order. Arity is the caller's
//! policy and is enforced here only because the caller says so: `--all` turns
//! "more than one match" from a refusal into a fan-out, and without it an
//! ambiguous selector fails loudly rather than silently picking the first
//! match, which for a mutating command is the difference between an edit and
//! the wrong edit.

use crate::definition::definition_shape;
use crate::dialect::Dialect;
use crate::sexpr::{ByteSpan, ExpressionKind, ExpressionPath, ExpressionView, SyntaxTree};
use crate::view_query::list_head;

use super::error::{AmbiguousCandidate, SelectorError, SelectorResult};
use super::line_index::{LineIndex, LinePosition};
use super::matcher::{Capture, match_all};
use super::pattern::Pattern;
use super::stable_id::{STABLE_ID_PREFIX, StableSelectorId, resolve_stable_id};

/// One way of naming a form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorTerm {
    /// `--path 0.2.1`
    Path(ExpressionPath),
    /// `--at 120`
    Offset(usize),
    /// `--line-column 12:5`
    Position(LinePosition),
    /// `--name parse-header`
    Name(String),
    /// `--query '(defun ?name ...)'`, optionally narrowed to one capture.
    Query {
        pattern: Pattern,
        source: String,
        capture: Option<String>,
    },
    /// `--id sel:0123456789abcdef`
    Id(StableSelectorId),
}

impl SelectorTerm {
    /// The spellings [`Self::parse_compact`] accepts, for error messages.
    pub const COMPACT_PREFIXES: &'static str =
        "path:, at:, line:, name:, id:, sel:, query:, or a bare path such as 0.2";

    /// Parses the one-string form used by `--from` and `--to`.
    ///
    /// `--from`/`--to` need a whole selector in a single value, which the
    /// per-kind flags cannot express twice in one invocation. A bare `0.2` is
    /// read as a path because that is the selector an agent already has in
    /// hand from `inspect outline`.
    pub fn parse_compact(value: &str, dialect: Dialect) -> SelectorResult<Self> {
        let malformed = |kind: &str, detail: String| SelectorError::MalformedSelector {
            kind: kind.to_owned(),
            value: value.to_owned(),
            detail,
        };

        if let Some(rest) = value.strip_prefix("path:") {
            return rest
                .parse::<ExpressionPath>()
                .map(Self::Path)
                .map_err(|error| malformed("path", error.to_string()));
        }
        if let Some(rest) = value.strip_prefix("at:") {
            return rest
                .trim()
                .parse::<usize>()
                .map(Self::Offset)
                .map_err(|_| malformed("byte offset", "expected a byte offset".to_owned()));
        }
        if let Some(rest) = value.strip_prefix("line:") {
            return rest.parse::<LinePosition>().map(Self::Position);
        }
        if let Some(rest) = value.strip_prefix("name:") {
            return Ok(Self::Name(rest.to_owned()));
        }
        if let Some(rest) = value.strip_prefix("query:") {
            return Ok(Self::Query {
                pattern: Pattern::parse(rest, dialect)?,
                source: rest.to_owned(),
                capture: None,
            });
        }
        if value.starts_with(STABLE_ID_PREFIX) || value.starts_with("id:") {
            let rest = value.strip_prefix("id:").unwrap_or(value);
            return StableSelectorId::parse(rest).map(Self::Id);
        }
        if !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return value
                .parse::<ExpressionPath>()
                .map(Self::Path)
                .map_err(|error| malformed("path", error.to_string()));
        }

        Err(SelectorError::UnknownSelectorPrefix {
            value: value.to_owned(),
            expected: Self::COMPACT_PREFIXES.to_owned(),
        })
    }

    /// How this selector is named in a diagnostic.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Path(path) => format!("--path {path}"),
            Self::Offset(offset) => format!("--at {offset}"),
            Self::Position(position) => format!("--line-column {position}"),
            Self::Name(name) => format!("--name {name}"),
            Self::Query {
                source,
                capture: None,
                ..
            } => format!("--query '{source}'"),
            Self::Query {
                source,
                capture: Some(capture),
                ..
            } => format!("--query '{source}' --capture {capture}"),
            Self::Id(id) => format!("--id {id}"),
        }
    }
}

/// A move relative to an already-selected form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativeStep {
    /// One level out.
    Parent,
    /// Into child `N`.
    Child(usize),
    /// `N` siblings forward, or backward when negative.
    Sibling(isize),
}

impl RelativeStep {
    fn describe(self) -> String {
        match self {
            Self::Parent => "--parent".to_owned(),
            Self::Child(index) => format!("--child {index}"),
            Self::Sibling(offset) => format!("--sibling {offset}"),
        }
    }
}

/// A contiguous run of siblings, produced by `--from`/`--to` and by a rest
/// capture that bound more than one form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeExtent {
    /// Path of the list the run sits in; empty for the top level.
    pub parent: ExpressionPath,
    /// Index of the first covered child.
    pub first: usize,
    /// Index of the last covered child.
    pub last: usize,
}

impl RangeExtent {
    /// How many forms the run covers.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.last - self.first + 1
    }

    /// Always false; a range covers at least one form by construction.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

/// One form (or run of forms) a selector named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorTarget {
    /// The selected form, or the first form of a range.
    pub path: ExpressionPath,
    /// Source covered by the selection, including a range's interior.
    pub span: ByteSpan,
    /// Present only when the selection spans more than one sibling.
    pub range: Option<RangeExtent>,
    /// Pattern captures, empty for every selector but `--query`.
    pub captures: Vec<Capture>,
}

impl SelectorTarget {
    /// The number of top-level forms this target covers.
    #[must_use]
    pub fn form_count(&self) -> usize {
        self.range.as_ref().map_or(1, RangeExtent::len)
    }
}

/// A fully specified selection request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorRequest {
    /// The base selector, or the start of a range when `range_end` is set.
    pub term: SelectorTerm,
    /// The end of a `--from`/`--to` range.
    pub range_end: Option<SelectorTerm>,
    /// Relative moves, applied in order to every match.
    pub steps: Vec<RelativeStep>,
    /// Whether more than one match is an answer rather than a refusal.
    pub all: bool,
}

impl SelectorRequest {
    /// A request with no relative steps, no range, and single-match arity.
    #[must_use]
    pub const fn new(term: SelectorTerm) -> Self {
        Self {
            term,
            range_end: None,
            steps: Vec::new(),
            all: false,
        }
    }

    /// How the whole request is named in a diagnostic.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut rendered = self.term.describe();
        if let Some(end) = &self.range_end {
            rendered = format!("--from {rendered} --to {}", end.describe());
        }
        for step in &self.steps {
            rendered.push(' ');
            rendered.push_str(&step.describe());
        }
        rendered
    }
}

/// Resolves `request` against `tree`, in source order.
///
/// Fails when nothing matches, and — unless `all` is set — when more than one
/// form does.
pub fn resolve(
    tree: &SyntaxTree,
    dialect: Dialect,
    request: &SelectorRequest,
) -> SelectorResult<Vec<SelectorTarget>> {
    let targets = match &request.range_end {
        Some(end) => vec![resolve_range(tree, dialect, request, end)?],
        None => {
            let mut targets = resolve_term(tree, dialect, &request.term)?;
            for target in &mut targets {
                apply_steps(tree, target, &request.steps)?;
            }
            targets
        }
    };

    if targets.is_empty() {
        return Err(SelectorError::NoMatch {
            selector: request.describe(),
        });
    }
    if !request.all && targets.len() > 1 {
        return Err(SelectorError::Ambiguous {
            selector: request.describe(),
            count: targets.len(),
            candidates: ambiguous_candidates(tree, &targets),
        });
    }
    Ok(targets)
}

fn resolve_range(
    tree: &SyntaxTree,
    dialect: Dialect,
    request: &SelectorRequest,
    end: &SelectorTerm,
) -> SelectorResult<SelectorTarget> {
    let mut start = exactly_one(tree, dialect, &request.term)?;
    let mut finish = exactly_one(tree, dialect, end)?;
    apply_steps(tree, &mut start, &request.steps)?;
    apply_steps(tree, &mut finish, &request.steps)?;

    let (start_parent, start_index) = split_path(&start.path)?;
    let (end_parent, end_index) = split_path(&finish.path)?;
    if start_parent != end_parent {
        return Err(SelectorError::RangeParentMismatch);
    }
    if start_index > end_index {
        return Err(SelectorError::RangeReversed);
    }

    let span = ByteSpan::try_new(start.span.start(), finish.span.end())
        .ok_or(SelectorError::RangeReversed)?;
    Ok(SelectorTarget {
        path: start.path,
        span,
        range: (start_index != end_index).then_some(RangeExtent {
            parent: start_parent,
            first: start_index,
            last: end_index,
        }),
        captures: Vec::new(),
    })
}

fn exactly_one(
    tree: &SyntaxTree,
    dialect: Dialect,
    term: &SelectorTerm,
) -> SelectorResult<SelectorTarget> {
    let mut targets = resolve_term(tree, dialect, term)?;
    match targets.len() {
        0 => Err(SelectorError::NoMatch {
            selector: term.describe(),
        }),
        1 => Ok(targets.remove(0)),
        count => Err(SelectorError::Ambiguous {
            selector: term.describe(),
            count,
            candidates: ambiguous_candidates(tree, &targets),
        }),
    }
}

fn split_path(path: &ExpressionPath) -> SelectorResult<(ExpressionPath, usize)> {
    let indexes = path.to_raw_indexes();
    let Some((last, head)) = indexes.split_last() else {
        return Err(SelectorError::RangeParentMismatch);
    };
    Ok((ExpressionPath::from_indexes(head.to_vec()), *last))
}

fn resolve_term(
    tree: &SyntaxTree,
    dialect: Dialect,
    term: &SelectorTerm,
) -> SelectorResult<Vec<SelectorTarget>> {
    match term {
        SelectorTerm::Path(path) => {
            let selection = tree.select_path(path)?;
            Ok(vec![SelectorTarget {
                path: path.clone(),
                span: selection.span(),
                range: None,
                captures: Vec::new(),
            }])
        }
        SelectorTerm::Offset(offset) => target_at_offset(tree, *offset),
        SelectorTerm::Position(position) => {
            let source = tree.source();
            let offset = LineIndex::new(source).offset_of(source, *position)?;
            target_at_offset(tree, offset)
        }
        SelectorTerm::Name(name) => Ok(definitions_named(tree, dialect, name)),
        SelectorTerm::Id(id) => {
            let path = resolve_stable_id(tree, dialect, id)?;
            let selection = tree.select_path(&path)?;
            Ok(vec![SelectorTarget {
                path,
                span: selection.span(),
                range: None,
                captures: Vec::new(),
            }])
        }
        SelectorTerm::Query {
            pattern, capture, ..
        } => query_targets(tree, dialect, pattern, capture.as_deref()),
    }
}

fn target_at_offset(tree: &SyntaxTree, offset: usize) -> SelectorResult<Vec<SelectorTarget>> {
    let selection = tree.select_at(offset)?;
    let path = path_of_span(tree, selection.span()).ok_or(SelectorError::NoMatch {
        selector: format!("--at {offset}"),
    })?;
    Ok(vec![SelectorTarget {
        path,
        span: selection.span(),
        range: None,
        captures: Vec::new(),
    }])
}

/// The path of the node whose span is exactly `span`.
///
/// `select_at` returns a `Selection`, which deliberately does not expose its
/// path — the tree walks by node id internally. Every selector but `--path`
/// still has to report one, so it is recovered by matching the span during a
/// walk. Spans are unique per node, so the match is exact.
fn path_of_span(tree: &SyntaxTree, span: ByteSpan) -> Option<ExpressionPath> {
    let root = tree.root_view();
    let mut pending: Vec<(&ExpressionView, ExpressionPath)> = root
        .children
        .iter()
        .enumerate()
        .rev()
        .map(|(index, child)| (child, ExpressionPath::root_child(index)))
        .collect();

    while let Some((view, path)) = pending.pop() {
        if view.span == span {
            return Some(path);
        }
        if view.span.contains_span(span) {
            pending.extend(
                view.children
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, child)| (child, path.child(index))),
            );
        }
    }
    None
}

/// Every definition form in the file whose name is `name`.
///
/// The whole tree is searched, not just the top level: a `defun` inside
/// `eval-when` or a Clojure `comment` block is still the definition of that
/// name, and an agent asking for it by name does not know where it sits.
fn definitions_named(tree: &SyntaxTree, dialect: Dialect, name: &str) -> Vec<SelectorTarget> {
    let root = tree.root_view();
    let mut targets = Vec::new();
    let mut pending: Vec<(&ExpressionView, ExpressionPath)> = root
        .children
        .iter()
        .enumerate()
        .rev()
        .map(|(index, child)| (child, ExpressionPath::root_child(index)))
        .collect();

    while let Some((view, path)) = pending.pop() {
        // Written as nested `if let`s rather than a `let` chain: the MSRV is
        // 1.85, where edition 2024 makes chains *look* available and only the
        // pinned-toolchain check rejects them.
        if view.kind == ExpressionKind::List {
            if let Some(shape) =
                list_head(view).and_then(|head| definition_shape(dialect, view, head))
            {
                if shape
                    .name(view)
                    .is_some_and(|found| symbol_names(dialect, found, name))
                {
                    targets.push(SelectorTarget {
                        path: path.clone(),
                        span: view.span,
                        range: None,
                        captures: Vec::new(),
                    });
                }
            }
        }
        pending.extend(
            view.children
                .iter()
                .enumerate()
                .rev()
                .map(|(index, child)| (child, path.child(index))),
        );
    }

    targets.sort_by_key(|target| (target.span.start(), target.span.end()));
    targets
}

/// Whether a definition's name is the one `--name` asked for.
///
/// Only Common Lisp folds case and package qualifiers, for the same reason a
/// literal pattern atom does: its reader upcases, and no other supported
/// dialect's does.
fn symbol_names(dialect: Dialect, found: &str, wanted: &str) -> bool {
    if dialect == Dialect::CommonLisp {
        crate::common_lisp::common_lisp_symbol_reference_eq(found, wanted)
    } else {
        found == wanted
    }
}

fn query_targets(
    tree: &SyntaxTree,
    dialect: Dialect,
    pattern: &Pattern,
    capture: Option<&str>,
) -> SelectorResult<Vec<SelectorTarget>> {
    let matches = match_all(tree, pattern, dialect);
    let Some(capture) = capture else {
        return Ok(matches
            .into_iter()
            .map(|found| SelectorTarget {
                path: found.path,
                span: found.span,
                range: None,
                captures: found.captures,
            })
            .collect());
    };

    let bound = pattern.capture_names();
    if !bound.iter().any(|name| name == capture) {
        return Err(SelectorError::UnknownCapture {
            name: capture.to_owned(),
            bound: if bound.is_empty() {
                "nothing".to_owned()
            } else {
                bound.join(", ")
            },
        });
    }

    let mut targets = Vec::new();
    for found in matches {
        let Some(binding) = found.capture(capture) else {
            continue;
        };
        let (Some(span), Some(first)) = (binding.span, binding.paths.first()) else {
            // A rest that bound nothing has no source to select.
            continue;
        };
        let range = (binding.paths.len() > 1)
            .then(|| range_of(&binding.paths))
            .flatten();
        targets.push(SelectorTarget {
            path: first.clone(),
            span,
            range,
            captures: found.captures.clone(),
        });
    }
    Ok(targets)
}

fn range_of(paths: &[ExpressionPath]) -> Option<RangeExtent> {
    let (first, last) = (paths.first()?, paths.last()?);
    let (parent, first_index) = split_path(first).ok()?;
    let (end_parent, last_index) = split_path(last).ok()?;
    (parent == end_parent).then_some(RangeExtent {
        parent,
        first: first_index,
        last: last_index,
    })
}

fn apply_steps(
    tree: &SyntaxTree,
    target: &mut SelectorTarget,
    steps: &[RelativeStep],
) -> SelectorResult<()> {
    if steps.is_empty() {
        return Ok(());
    }

    let mut path = target.path.clone();
    let mut parents_taken = 0usize;
    let requested_parents = steps
        .iter()
        .filter(|step| matches!(step, RelativeStep::Parent))
        .count();

    for step in steps {
        match step {
            RelativeStep::Parent => {
                let parent = path.parent().filter(|parent| !parent.indexes().is_empty());
                path = parent.ok_or(SelectorError::ParentAboveRoot {
                    depth: parents_taken,
                    requested: requested_parents,
                })?;
                parents_taken += 1;
            }
            RelativeStep::Child(index) => {
                let count = tree.select_path(&path)?.view().children.len();
                if *index >= count {
                    return Err(SelectorError::ChildOutOfRange {
                        index: *index,
                        count,
                    });
                }
                path = path.child(*index);
            }
            RelativeStep::Sibling(offset) => {
                let (parent, current) =
                    split_path(&path).map_err(|_| SelectorError::SiblingWithoutParent)?;
                let count = sibling_count(tree, &parent)?;
                let moved = isize::try_from(current)
                    .ok()
                    .and_then(|current| current.checked_add(*offset))
                    .and_then(|moved| usize::try_from(moved).ok())
                    .filter(|moved| *moved < count)
                    .ok_or(SelectorError::SiblingOutOfRange {
                        offset: *offset,
                        count,
                    })?;
                path = parent.child(moved);
            }
        }
    }

    let selection = tree.select_path(&path)?;
    target.span = selection.span();
    target.path = path;
    // A relative move lands on one form, so any range the base selector
    // produced no longer describes the selection.
    target.range = None;
    Ok(())
}

fn sibling_count(tree: &SyntaxTree, parent: &ExpressionPath) -> SelectorResult<usize> {
    if parent.indexes().is_empty() {
        return Ok(tree.root_children().len());
    }
    Ok(tree.select_path(parent)?.view().children.len())
}

/// The span of a target rendered as a source slice.
#[must_use]
pub fn target_text<'a>(tree: &'a SyntaxTree, target: &SelectorTarget) -> &'a str {
    tree.source()
        .get(target.span.as_range())
        .unwrap_or_default()
}

/// How many of an ambiguous selector's matches [`ambiguous_candidates`]
/// describes.
///
/// A selector can match arbitrarily many forms — `--query t` in a large file
/// might match thousands — and `SelectorError::Ambiguous` is a single error
/// value that every JSON envelope carries whole. Bounding it keeps that
/// payload bounded too; `count` on the error still reports the true total,
/// and `--all` (or `inspect resolve`) is still how a caller sees the rest.
pub const MAX_AMBIGUOUS_CANDIDATES: usize = 20;

/// Builds the candidate list [`SelectorError::Ambiguous`] carries: each
/// match's path, line/column extent, and a short preview, so a caller does
/// not need a separate `inspect resolve` call just to see what an ambiguous
/// selector found.
#[must_use]
pub fn ambiguous_candidates(
    tree: &SyntaxTree,
    targets: &[SelectorTarget],
) -> Vec<AmbiguousCandidate> {
    let source = tree.source();
    let index = LineIndex::new(source);
    targets
        .iter()
        .take(MAX_AMBIGUOUS_CANDIDATES)
        .map(|target| AmbiguousCandidate {
            path: target.path.to_string(),
            start: index.position_of(source, target.span.start().get()),
            end: index.position_of(source, target.span.end().get()),
            preview: candidate_preview(source, target.span),
        })
        .collect()
}

/// A single-line, length-bounded rendering of the matched source.
///
/// Newlines and runs of whitespace collapse so one candidate stays one line.
/// Independent of `resolve_report`'s own preview builder (that crate sits
/// above this one and cannot be depended on from here), but the same shape:
/// short enough that twenty candidates still fit an agent's context.
fn candidate_preview(source: &str, span: ByteSpan) -> String {
    const MAX_PREVIEW_BYTES: usize = 80;

    let text = source.get(span.as_range()).unwrap_or_default();
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() <= MAX_PREVIEW_BYTES {
        return collapsed;
    }
    let mut end = MAX_PREVIEW_BYTES.min(collapsed.len());
    while !collapsed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &collapsed[..end])
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn tree(source: &str) -> SyntaxTree {
        SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).unwrap()
    }

    fn request(term: SelectorTerm) -> SelectorRequest {
        SelectorRequest::new(term)
    }

    fn query(text: &str) -> SelectorTerm {
        SelectorTerm::Query {
            pattern: Pattern::parse(text, Dialect::CommonLisp).unwrap(),
            source: text.to_owned(),
            capture: None,
        }
    }

    fn paths(tree: &SyntaxTree, request: &SelectorRequest) -> Vec<String> {
        resolve(tree, Dialect::CommonLisp, request)
            .unwrap()
            .into_iter()
            .map(|target| target.path.to_string())
            .collect()
    }

    #[test]
    fn a_name_selects_the_definition_that_carries_it() {
        let parsed = tree("(defvar *x* 1)\n(defun parse-header (s) s)");
        let found = paths(
            &parsed,
            &request(SelectorTerm::Name("parse-header".to_owned())),
        );
        assert_eq!(found, vec!["1".to_owned()]);
    }

    #[test]
    fn a_name_finds_a_definition_nested_inside_another_form() {
        let parsed = tree("(eval-when (:compile-toplevel) (defun helper () nil))");
        assert_eq!(
            paths(&parsed, &request(SelectorTerm::Name("helper".to_owned()))),
            vec!["0.2".to_owned()]
        );
    }

    #[test]
    fn a_line_and_column_select_the_smallest_form_there() {
        let parsed = tree("(defun f (x)\n  (cleanup x))\n");
        let position = "2:3".parse::<LinePosition>().unwrap();
        assert_eq!(
            paths(&parsed, &request(SelectorTerm::Position(position))),
            vec!["0.3".to_owned()]
        );
    }

    #[test]
    fn an_ambiguous_selector_refuses_unless_all_is_set() {
        let parsed = tree("(defun a ()) (defun b ())");
        let error = resolve(
            &parsed,
            Dialect::CommonLisp,
            &request(query("(defun ?n ...)")),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "--query '(defun ?n ...)' matches 2 forms; pass --all to act on every match, \
             or narrow the selector (see `paredit inspect resolve`)"
        );

        let mut all = request(query("(defun ?n ...)"));
        all.all = true;
        assert_eq!(paths(&parsed, &all), vec!["0".to_owned(), "1".to_owned()]);
    }

    #[test]
    fn an_ambiguous_error_carries_the_matches_it_found() {
        let parsed = tree("(defun a ()) (defun b ())");
        let error = resolve(
            &parsed,
            Dialect::CommonLisp,
            &request(query("(defun ?n ...)")),
        )
        .unwrap_err();
        let SelectorError::Ambiguous {
            count, candidates, ..
        } = error
        else {
            panic!("expected Ambiguous, got {error:?}");
        };
        assert_eq!(count, 2);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].path, "0");
        assert_eq!(candidates[0].preview, "(defun a ())");
        assert_eq!(candidates[1].path, "1");
        assert_eq!(candidates[1].preview, "(defun b ())");
    }

    #[test]
    fn ambiguous_candidates_are_capped_but_the_count_is_not() {
        let source = (0..30)
            .map(|index| format!("(defun f{index} ())"))
            .collect::<Vec<_>>()
            .join(" ");
        let parsed = tree(&source);
        let error = resolve(
            &parsed,
            Dialect::CommonLisp,
            &request(query("(defun ?n ...)")),
        )
        .unwrap_err();
        let SelectorError::Ambiguous {
            count, candidates, ..
        } = error
        else {
            panic!("expected Ambiguous, got {error:?}");
        };
        assert_eq!(count, 30);
        assert_eq!(candidates.len(), MAX_AMBIGUOUS_CANDIDATES);
    }

    #[test]
    fn a_selector_matching_nothing_names_itself() {
        let parsed = tree("(defun a ())");
        let error = resolve(
            &parsed,
            Dialect::CommonLisp,
            &request(SelectorTerm::Name("missing".to_owned())),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "no form matches --name missing");
    }

    #[test]
    fn a_capture_selects_the_bound_form_rather_than_the_whole_match() {
        let parsed = tree("(defun parse-header (s) s)");
        let mut term = query("(defun ?name ...)");
        if let SelectorTerm::Query { capture, .. } = &mut term {
            *capture = Some("name".to_owned());
        }
        let targets = resolve(&parsed, Dialect::CommonLisp, &request(term)).unwrap();
        assert_eq!(targets[0].path.to_string(), "0.1");
        assert_eq!(target_text(&parsed, &targets[0]), "parse-header");
    }

    #[test]
    fn a_rest_capture_selects_the_run_it_bound() {
        let parsed = tree("(progn a b c)");
        let mut term = query("(progn ?body...)");
        if let SelectorTerm::Query { capture, .. } = &mut term {
            *capture = Some("body".to_owned());
        }
        let targets = resolve(&parsed, Dialect::CommonLisp, &request(term)).unwrap();
        assert_eq!(target_text(&parsed, &targets[0]), "a b c");
        assert_eq!(targets[0].form_count(), 3);
    }

    #[test]
    fn an_unknown_capture_lists_the_ones_the_pattern_binds() {
        let parsed = tree("(defun f ())");
        let mut term = query("(defun ?name ...)");
        if let SelectorTerm::Query { capture, .. } = &mut term {
            *capture = Some("body".to_owned());
        }
        let error = resolve(&parsed, Dialect::CommonLisp, &request(term)).unwrap_err();
        assert_eq!(
            error.to_string(),
            "--capture body is not bound by this pattern; it binds name"
        );
    }

    #[test]
    fn relative_steps_walk_up_sideways_and_down() {
        let parsed = tree("(defun f (x) (a) (b))");
        let mut moved = request(SelectorTerm::Path("0.2".parse().unwrap()));
        moved.steps = vec![RelativeStep::Sibling(1)];
        assert_eq!(paths(&parsed, &moved), vec!["0.3".to_owned()]);

        let mut up = request(SelectorTerm::Path("0.3.0".parse().unwrap()));
        up.steps = vec![RelativeStep::Parent];
        assert_eq!(paths(&parsed, &up), vec!["0.3".to_owned()]);

        let mut down = request(SelectorTerm::Path("0".parse().unwrap()));
        down.steps = vec![RelativeStep::Child(1)];
        assert_eq!(paths(&parsed, &down), vec!["0.1".to_owned()]);
    }

    #[test]
    fn a_step_past_the_edge_of_the_tree_is_refused() {
        let parsed = tree("(defun f (x))");
        let mut up = request(SelectorTerm::Path("0".parse().unwrap()));
        up.steps = vec![RelativeStep::Parent];
        assert_eq!(
            resolve(&parsed, Dialect::CommonLisp, &up)
                .unwrap_err()
                .to_string(),
            "--parent moved above the top level after 0 of 1 steps"
        );

        let mut sideways = request(SelectorTerm::Path("0.2".parse().unwrap()));
        sideways.steps = vec![RelativeStep::Sibling(5)];
        assert_eq!(
            resolve(&parsed, Dialect::CommonLisp, &sideways)
                .unwrap_err()
                .to_string(),
            "--sibling 5 is out of range: the selected form has 3 siblings"
        );

        let mut down = request(SelectorTerm::Path("0".parse().unwrap()));
        down.steps = vec![RelativeStep::Child(9)];
        assert_eq!(
            resolve(&parsed, Dialect::CommonLisp, &down)
                .unwrap_err()
                .to_string(),
            "--child 9 is out of range: the selected form has 3 children"
        );
    }

    #[test]
    fn a_range_covers_every_sibling_between_its_ends() {
        let parsed = tree("(progn (a) (b) (c) (d))");
        let mut ranged = request(SelectorTerm::Path("0.1".parse().unwrap()));
        ranged.range_end = Some(SelectorTerm::Path("0.3".parse().unwrap()));
        let targets = resolve(&parsed, Dialect::CommonLisp, &ranged).unwrap();
        assert_eq!(target_text(&parsed, &targets[0]), "(a) (b) (c)");
        assert_eq!(targets[0].form_count(), 3);
    }

    #[test]
    fn a_range_across_two_lists_is_refused() {
        let parsed = tree("(progn (a)) (progn (b))");
        let mut ranged = request(SelectorTerm::Path("0.1".parse().unwrap()));
        ranged.range_end = Some(SelectorTerm::Path("1.1".parse().unwrap()));
        assert_eq!(
            resolve(&parsed, Dialect::CommonLisp, &ranged)
                .unwrap_err()
                .to_string(),
            "--from and --to must select forms inside the same list"
        );
    }

    #[test]
    fn a_reversed_range_is_refused_rather_than_silently_swapped() {
        let parsed = tree("(progn (a) (b))");
        let mut ranged = request(SelectorTerm::Path("0.2".parse().unwrap()));
        ranged.range_end = Some(SelectorTerm::Path("0.1".parse().unwrap()));
        assert_eq!(
            resolve(&parsed, Dialect::CommonLisp, &ranged)
                .unwrap_err()
                .to_string(),
            "--from must select a form at or before --to"
        );
    }

    #[test]
    fn compact_selectors_cover_every_kind() {
        let dialect = Dialect::CommonLisp;
        assert!(matches!(
            SelectorTerm::parse_compact("0.2", dialect).unwrap(),
            SelectorTerm::Path(_)
        ));
        assert!(matches!(
            SelectorTerm::parse_compact("path:0.2", dialect).unwrap(),
            SelectorTerm::Path(_)
        ));
        assert!(matches!(
            SelectorTerm::parse_compact("at:120", dialect).unwrap(),
            SelectorTerm::Offset(120)
        ));
        assert!(matches!(
            SelectorTerm::parse_compact("line:12:5", dialect).unwrap(),
            SelectorTerm::Position(_)
        ));
        assert!(matches!(
            SelectorTerm::parse_compact("name:foo", dialect).unwrap(),
            SelectorTerm::Name(_)
        ));
        assert!(matches!(
            SelectorTerm::parse_compact("sel:0123456789abcdef", dialect).unwrap(),
            SelectorTerm::Id(_)
        ));
        assert!(matches!(
            SelectorTerm::parse_compact("query:(defun ?n ...)", dialect).unwrap(),
            SelectorTerm::Query { .. }
        ));
        assert_eq!(
            SelectorTerm::parse_compact("nonsense", dialect)
                .unwrap_err()
                .to_string(),
            "unknown selector prefix in `nonsense`: expected one of \
             path:, at:, line:, name:, id:, sel:, query:, or a bare path such as 0.2"
        );
    }
}
