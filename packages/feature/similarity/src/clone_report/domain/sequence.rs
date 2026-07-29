//! Sub-form clones: duplicated runs of consecutive sibling forms.
//!
//! Every other clone report in this package compares *whole forms*, which means
//! it can only see duplication that someone happened to draw a form boundary
//! around. The common real case is narrower: two definitions that differ
//! overall but contain the same four-line stretch of body. No form matches, so
//! `duplicates` and `similarity` both stay quiet, and the four lines get copied
//! a third time.
//!
//! The unit here is therefore a *run*: a maximal-by-default stretch of adjacent
//! children of one list. A run is not a form and cannot be extracted by moving
//! one node, which is exactly why it is worth reporting separately — it names
//! work that is real and that the form-shaped reports structurally cannot find.
//!
//! Detection is a fingerprint index, then verification. Fingerprints find
//! candidate groups in one pass; every group that survives filtering is then
//! confirmed node by node against its representative with the same
//! classification the rest of the package uses, so a hash collision produces a
//! smaller group rather than a wrong one.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path as FsPath, PathBuf};

use thiserror::Error;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SyntaxTree,
};

use crate::form_similarity::{CloneType, StructuralTree, classify_clone};

use super::class::{ExtractionEstimate, MemberSize};
use super::shared::line_span_of;

/// A run longer than this is almost certainly a whole body, which the
/// form-shaped reports already cover, and enumerating every length up to a
/// file's widest list is what makes naive run detection quadratic.
pub const MAX_SUPPORTED_RUN_LENGTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceMatchMode {
    /// Runs must be identical down to every identifier: Type-1.
    Exact,
    /// Runs may differ in atom text only: Type-1 or Type-2.
    Renamed,
}

impl SequenceMatchMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Renamed => "renamed",
        }
    }

    const fn accepts(self, clone_type: CloneType) -> bool {
        match self {
            Self::Exact => matches!(clone_type, CloneType::Type1),
            Self::Renamed => matches!(clone_type, CloneType::Type1 | CloneType::Type2),
        }
    }
}

impl std::str::FromStr for SequenceMatchMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "exact" => Ok(Self::Exact),
            "renamed" => Ok(Self::Renamed),
            _ => Err(format!("unknown sequence match mode: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceOverlapPolicy {
    /// Drop a group whose every occurrence sits inside a longer reported run.
    Maximal,
    /// Report every run length that repeats.
    All,
}

impl SequenceOverlapPolicy {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Maximal => "maximal",
            Self::All => "all",
        }
    }
}

impl std::str::FromStr for SequenceOverlapPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "maximal" => Ok(Self::Maximal),
            "all" => Ok(Self::All),
            _ => Err(format!("unknown sequence overlap policy: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloneSequenceOptions {
    pub min_run_length: usize,
    pub max_run_length: usize,
    pub min_occurrences: usize,
    pub min_run_nodes: usize,
    pub match_mode: SequenceMatchMode,
    pub overlap_policy: SequenceOverlapPolicy,
    pub max_groups: Option<usize>,
    /// Report runs whose enclosing forms are themselves clones of each other.
    ///
    /// Off by default, and that default is what makes this report worth
    /// running. If every occurrence of a run sits in a different form and all
    /// those forms have the same shape, then the *forms* are the clone —
    /// something `clone-classes` already reports, ranks, and classifies better,
    /// because it compares them as wholes. Leaving these in would bury the
    /// partial duplication this report exists to find under duplication another
    /// report already found.
    ///
    /// Repetition *within* one form is never suppressed by this: there is only
    /// one enclosing form, so there is nothing for it to be a clone of.
    pub include_parent_clones: bool,
    pub helper_overhead_lines: usize,
}

impl Default for CloneSequenceOptions {
    fn default() -> Self {
        Self {
            min_run_length: 3,
            max_run_length: 16,
            min_occurrences: 2,
            min_run_nodes: 8,
            match_mode: SequenceMatchMode::Renamed,
            overlap_policy: SequenceOverlapPolicy::Maximal,
            max_groups: None,
            include_parent_clones: false,
            helper_overhead_lines: super::class::DEFAULT_HELPER_OVERHEAD_LINES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CloneSequenceOptionsError {
    #[error("--min-run-length must be at least 2")]
    MinRunLengthTooSmall,
    #[error("--max-run-length must not be below --min-run-length")]
    MaxRunLengthBelowMin,
    #[error("--max-run-length must not exceed {MAX_SUPPORTED_RUN_LENGTH}")]
    MaxRunLengthTooLarge,
    #[error("--min-occurrences must be at least 2")]
    MinOccurrencesTooSmall,
    #[error("--max-groups must be at least 1")]
    MaxGroupsTooSmall,
}

impl CloneSequenceOptions {
    pub const fn validate(&self) -> Result<(), CloneSequenceOptionsError> {
        if self.min_run_length < 2 {
            return Err(CloneSequenceOptionsError::MinRunLengthTooSmall);
        }
        if self.max_run_length < self.min_run_length {
            return Err(CloneSequenceOptionsError::MaxRunLengthBelowMin);
        }
        if self.max_run_length > MAX_SUPPORTED_RUN_LENGTH {
            return Err(CloneSequenceOptionsError::MaxRunLengthTooLarge);
        }
        if self.min_occurrences < 2 {
            return Err(CloneSequenceOptionsError::MinOccurrencesTooSmall);
        }
        if matches!(self.max_groups, Some(0)) {
            return Err(CloneSequenceOptionsError::MaxGroupsTooSmall);
        }
        Ok(())
    }
}

/// One file's parsed state, borrowed for the length of the scan.
#[derive(Debug, Clone, Copy)]
pub struct SequenceSource<'a> {
    pub path: &'a FsPath,
    pub dialect: Dialect,
    pub tree: &'a SyntaxTree,
    pub text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneSequenceOccurrence {
    pub path: PathBuf,
    pub dialect: Dialect,
    /// Expression path of the list whose children the run is taken from.
    /// Empty for a run of top-level forms.
    pub parent_path: Path,
    pub parent_head: Option<String>,
    pub first_child_index: usize,
    pub run_length: usize,
    /// First child's start to last child's end. Not a form: slicing it out
    /// yields a sequence, which is why it is reported as a span rather than as
    /// something `--path` could select.
    pub span: ByteSpan,
    pub node_count: usize,
    pub line_span: usize,
    pub text: String,
}

impl CloneSequenceOccurrence {
    fn overlaps(&self, other: &Self) -> bool {
        self.path == other.path
            && self.parent_path == other.parent_path
            && self.first_child_index < other.first_child_index + other.run_length
            && other.first_child_index < self.first_child_index + self.run_length
    }

    fn contained_by(&self, other: &Self) -> bool {
        self.path == other.path
            && self.parent_path == other.parent_path
            && other.first_child_index <= self.first_child_index
            && self.first_child_index + self.run_length
                <= other.first_child_index + other.run_length
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneSequenceGroup {
    pub rank: usize,
    pub run_length: usize,
    pub clone_type: CloneType,
    pub consistent_renaming: bool,
    pub occurrences: Vec<CloneSequenceOccurrence>,
    pub extraction: ExtractionEstimate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneSequenceReport {
    pub groups: Vec<CloneSequenceGroup>,
    pub scanned_files: usize,
    pub scanned_parents: usize,
    pub candidate_runs: usize,
    pub total_groups: usize,
    /// Groups dropped because their enclosing forms are themselves clones.
    /// Reported rather than dropped silently: without it a caller cannot tell
    /// "no partial duplication" from "all of it was whole-form duplication".
    pub suppressed_parent_clone_groups: usize,
    pub suppressed_overlapping_groups: usize,
    pub truncated_groups: usize,
}

impl CloneSequenceReport {
    #[must_use]
    pub fn saved_lines(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.extraction.saved_lines)
            .fold(0, usize::saturating_add)
    }

    #[must_use]
    pub fn occurrence_count(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.occurrences.len())
            .fold(0, usize::saturating_add)
    }
}

pub fn build_clone_sequence_report(
    sources: &[SequenceSource<'_>],
    options: &CloneSequenceOptions,
) -> Result<CloneSequenceReport, CloneSequenceOptionsError> {
    options.validate()?;

    let mut index = HashMap::<RunKey, Vec<Located>>::new();
    let mut scanned_parents = 0;
    let mut candidate_runs = 0;
    for (source_index, source) in sources.iter().enumerate() {
        scan_source(
            source_index,
            source,
            options,
            &mut index,
            &mut scanned_parents,
            &mut candidate_runs,
        );
    }

    let mut groups = Vec::new();
    let mut suppressed_parent_clone_groups = 0;
    for (key, located) in index {
        if located.len() < options.min_occurrences {
            continue;
        }
        let (verified, parent_clones) = verify_group(&key, located, sources, options);
        suppressed_parent_clone_groups += parent_clones;
        groups.extend(verified);
    }
    let total_groups = groups.len();

    groups.sort_by(compare_groups);
    let suppressed_overlapping_groups = match options.overlap_policy {
        SequenceOverlapPolicy::All => 0,
        SequenceOverlapPolicy::Maximal => {
            let before = groups.len();
            retain_maximal_groups(&mut groups);
            before - groups.len()
        }
    };

    let truncated_groups = options
        .max_groups
        .map_or(0, |limit| groups.len().saturating_sub(limit));
    if let Some(limit) = options.max_groups {
        groups.truncate(limit);
    }
    for (index, group) in groups.iter_mut().enumerate() {
        group.rank = index + 1;
    }

    Ok(CloneSequenceReport {
        groups,
        scanned_files: sources.len(),
        scanned_parents,
        candidate_runs,
        total_groups,
        suppressed_parent_clone_groups,
        suppressed_overlapping_groups,
        truncated_groups,
    })
}

/// Longest and most-repeated first; ties broken by location so the ranking is
/// reproducible.
fn compare_groups(left: &CloneSequenceGroup, right: &CloneSequenceGroup) -> std::cmp::Ordering {
    right
        .extraction
        .saved_lines
        .cmp(&left.extraction.saved_lines)
        .then_with(|| right.run_length.cmp(&left.run_length))
        .then_with(|| right.occurrences.len().cmp(&left.occurrences.len()))
        .then_with(|| left.clone_type.cmp(&right.clone_type))
        .then_with(|| {
            left.occurrences
                .first()
                .map(occurrence_order_key)
                .cmp(&right.occurrences.first().map(occurrence_order_key))
        })
}

fn occurrence_order_key(occurrence: &CloneSequenceOccurrence) -> (PathBuf, usize, usize) {
    (
        occurrence.path.clone(),
        occurrence.span.start().get(),
        occurrence.span.end().get(),
    )
}

/// Drops a group whose every occurrence sits inside an occurrence of a group
/// already retained.
///
/// Groups arrive ranked, so "already retained" means "at least as valuable",
/// and a shorter run that only ever appears inside a longer reported one is
/// noise: extracting the longer run removes it too.
fn retain_maximal_groups(groups: &mut Vec<CloneSequenceGroup>) {
    let mut retained: Vec<CloneSequenceGroup> = Vec::with_capacity(groups.len());
    for group in groups.drain(..) {
        let covered = group.occurrences.iter().all(|occurrence| {
            retained.iter().any(|kept| {
                kept.run_length > occurrence.run_length
                    && kept
                        .occurrences
                        .iter()
                        .any(|outer| occurrence.contained_by(outer))
            })
        });
        if !covered {
            retained.push(group);
        }
    }
    *groups = retained;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RunKey {
    /// Per-child digests rather than one rolled-up hash: a collision then needs
    /// two children to collide in the same slot, and verification catches what
    /// is left.
    children: Vec<u64>,
}

#[derive(Debug, Clone)]
struct Located {
    source_index: usize,
    /// The enclosing form's own digest, so a group can tell whether its
    /// occurrences merely restate a whole-form clone.
    parent_fingerprint: u64,
    parent_path: Path,
    parent_head: Option<String>,
    first_child_index: usize,
    span: ByteSpan,
    node_count: usize,
}

fn scan_source(
    source_index: usize,
    source: &SequenceSource<'_>,
    options: &CloneSequenceOptions,
    index: &mut HashMap<RunKey, Vec<Located>>,
    scanned_parents: &mut usize,
    candidate_runs: &mut usize,
) {
    let root = source.tree.root_view();
    let metrics = subtree_metrics(&root, options.match_mode);

    let mut path_stack: Vec<usize> = Vec::new();
    let mut pending = vec![Visit::Enter {
        view: &root,
        index: None,
    }];
    while let Some(visit) = pending.pop() {
        let Visit::Enter {
            view,
            index: child_index,
        } = visit
        else {
            path_stack.pop();
            continue;
        };
        if let Some(child_index) = child_index {
            path_stack.push(child_index);
            pending.push(Visit::Leave);
        }

        if view.children.len() >= options.min_run_length {
            *scanned_parents += 1;
            record_runs(
                source_index,
                view,
                &path_stack,
                &metrics,
                options,
                index,
                candidate_runs,
            );
        }

        for (offset, child) in view.children.iter().enumerate().rev() {
            pending.push(Visit::Enter {
                view: child,
                index: Some(offset),
            });
        }
    }
}

enum Visit<'a> {
    Enter {
        view: &'a ExpressionView,
        index: Option<usize>,
    },
    Leave,
}

#[allow(clippy::too_many_arguments)]
fn record_runs(
    source_index: usize,
    parent: &ExpressionView,
    path_stack: &[usize],
    metrics: &HashMap<ByteSpan, ChildMetrics>,
    options: &CloneSequenceOptions,
    index: &mut HashMap<RunKey, Vec<Located>>,
    candidate_runs: &mut usize,
) {
    let parent_head = parent
        .children
        .first()
        .and_then(atom_text)
        .map(ToOwned::to_owned);
    let parent_fingerprint = metrics[&parent.span].fingerprint;
    let child_count = parent.children.len();

    for start in 0..child_count {
        let mut children = Vec::with_capacity(options.max_run_length);
        let mut node_count = 0usize;
        for offset in 0..options.max_run_length {
            let Some(child) = parent.children.get(start + offset) else {
                break;
            };
            let child_metrics = metrics[&child.span];
            children.push(child_metrics.fingerprint);
            node_count = node_count.saturating_add(child_metrics.node_count);

            let run_length = offset + 1;
            if run_length < options.min_run_length || node_count < options.min_run_nodes {
                continue;
            }
            let span = ByteSpan::new(
                parent.children[start].span.start(),
                parent.children[start + offset].span.end(),
            );
            *candidate_runs += 1;
            index
                .entry(RunKey {
                    children: children.clone(),
                })
                .or_default()
                .push(Located {
                    source_index,
                    parent_fingerprint,
                    parent_path: Path::from_indexes(path_stack.to_vec()),
                    parent_head: parent_head.clone(),
                    first_child_index: start,
                    span,
                    node_count,
                });
        }
    }
}

/// Confirms a fingerprint bucket structurally and splits it if it disagrees.
///
/// The representative is the first occurrence in source order. Everything that
/// classifies against it within the requested match mode joins its group;
/// everything else is set aside and re-verified against its own representative,
/// so a collision costs an extra group rather than a wrong one.
fn verify_group(
    key: &RunKey,
    mut located: Vec<Located>,
    sources: &[SequenceSource<'_>],
    options: &CloneSequenceOptions,
) -> (Vec<CloneSequenceGroup>, usize) {
    located.sort_by(|left, right| {
        sources[left.source_index]
            .path
            .cmp(sources[right.source_index].path)
            .then_with(|| left.span.start().get().cmp(&right.span.start().get()))
            .then_with(|| right.span.end().get().cmp(&left.span.end().get()))
    });

    let run_length = key.children.len();
    let mut groups = Vec::new();
    let mut parent_clone_groups = 0;
    let mut remaining = located;
    while remaining.len() >= options.min_occurrences {
        let representative = remaining.remove(0);
        let Some(representative_views) = run_views(&representative, sources, run_length) else {
            continue;
        };

        let mut members = vec![(representative, CloneType::Type1, true)];
        let mut deferred = Vec::new();
        for candidate in remaining {
            let verdict = run_views(&candidate, sources, run_length)
                .and_then(|views| verify_run(&representative_views, &views));
            match verdict {
                Some((clone_type, consistent)) if options.match_mode.accepts(clone_type) => {
                    members.push((candidate, clone_type, consistent));
                }
                _ => deferred.push(candidate),
            }
        }
        remaining = deferred;

        if members.len() < options.min_occurrences {
            continue;
        }
        match assemble_group(members, sources, options, run_length) {
            Assembled::Group(group) => groups.push(*group),
            Assembled::ParentClone => parent_clone_groups += 1,
            Assembled::TooFewOccurrences => {}
        }
    }
    (groups, parent_clone_groups)
}

/// Why a verified bucket did or did not become a reported group.
enum Assembled {
    Group(Box<CloneSequenceGroup>),
    /// The enclosing forms are clones of each other, so `clone-classes` owns it.
    ParentClone,
    /// Dropping overlapping occurrences left too few independent copies.
    TooFewOccurrences,
}

fn assemble_group(
    members: Vec<(Located, CloneType, bool)>,
    sources: &[SequenceSource<'_>],
    options: &CloneSequenceOptions,
    run_length: usize,
) -> Assembled {
    let clone_type = members
        .iter()
        .map(|(_, clone_type, _)| *clone_type)
        .max()
        .unwrap_or(CloneType::Type1);
    let consistent_renaming = members.iter().all(|(_, _, consistent)| *consistent);

    let mut occurrences = Vec::with_capacity(members.len());
    let mut parent_identities = Vec::with_capacity(members.len());
    for (located, _, _) in members {
        let source = &sources[located.source_index];
        parent_identities.push((
            source.path.to_path_buf(),
            located.parent_path.clone(),
            located.parent_fingerprint,
        ));
        let text = located.span.slice(source.text).to_owned();
        occurrences.push(CloneSequenceOccurrence {
            path: source.path.to_path_buf(),
            dialect: source.dialect,
            parent_path: located.parent_path,
            parent_head: located.parent_head,
            first_child_index: located.first_child_index,
            run_length,
            span: located.span,
            node_count: located.node_count,
            line_span: line_span_of(&text),
            text,
        });
    }

    drop_overlapping(&mut occurrences);
    if occurrences.len() < options.min_occurrences {
        return Assembled::TooFewOccurrences;
    }
    if !options.include_parent_clones && parents_are_clones(&parent_identities) {
        return Assembled::ParentClone;
    }

    let sizes = occurrences
        .iter()
        .map(|occurrence| MemberSize {
            path: occurrence.path.as_path(),
            lines: occurrence.line_span,
            nodes: occurrence.node_count,
        })
        .collect::<Vec<_>>();
    let extraction = ExtractionEstimate::from_sizes(&sizes, options.helper_overhead_lines);

    Assembled::Group(Box::new(CloneSequenceGroup {
        rank: 0,
        run_length,
        clone_type,
        consistent_renaming,
        occurrences,
        extraction,
    }))
}

/// Whether a group merely restates a whole-form clone.
///
/// True when the occurrences sit in two or more distinct enclosing forms and
/// every one of those forms has the same digest — that is, the enclosing forms
/// are clones, and the run inside them is not independent news.
fn parents_are_clones(parents: &[(PathBuf, Path, u64)]) -> bool {
    let mut distinct = parents
        .iter()
        .map(|(path, parent_path, fingerprint)| (path, parent_path, fingerprint))
        .collect::<Vec<_>>();
    distinct.sort_unstable_by(|left, right| left.0.cmp(right.0).then_with(|| left.1.cmp(right.1)));
    distinct.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    distinct.len() >= 2 && distinct.windows(2).all(|window| window[0].2 == window[1].2)
}

/// Keeps a leftmost non-overlapping subset within one group.
///
/// `(a a a a)` with a run length of two matches at offsets 0, 1 and 2, but
/// there are only two independent copies there. Counting three would overstate
/// both the duplication and what removing it saves.
fn drop_overlapping(occurrences: &mut Vec<CloneSequenceOccurrence>) {
    occurrences.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.span.start().get().cmp(&right.span.start().get()))
    });
    let mut kept: Vec<CloneSequenceOccurrence> = Vec::with_capacity(occurrences.len());
    for occurrence in occurrences.drain(..) {
        if !kept.iter().any(|existing| existing.overlaps(&occurrence)) {
            kept.push(occurrence);
        }
    }
    *occurrences = kept;
}

/// The forms a run covers, as views, for verification.
fn run_views(
    located: &Located,
    sources: &[SequenceSource<'_>],
    run_length: usize,
) -> Option<Vec<ExpressionView>> {
    let source = &sources[located.source_index];
    let parent_indexes = located
        .parent_path
        .indexes()
        .iter()
        .map(|index| index.get())
        .collect::<Vec<_>>();
    (0..run_length)
        .map(|offset| {
            let mut indexes = parent_indexes.clone();
            indexes.push(located.first_child_index + offset);
            let selection = source.tree.select_path(&Path::from_indexes(indexes)).ok()?;
            Some(selection.view())
        })
        .collect()
}

/// Verifies one run against the representative, form by form.
///
/// Returns `None` when the run is not a clone of the representative at all —
/// either the taxonomy puts a pair at Type-3, or two corresponding forms
/// disagree on a head symbol. The head check is what the fingerprint cannot do
/// on its own, since two heads that collide in the digest still have to be
/// compared.
fn verify_run(left: &[ExpressionView], right: &[ExpressionView]) -> Option<(CloneType, bool)> {
    let mut clone_type = CloneType::Type1;
    let mut consistent = true;
    for (left, right) in left.iter().zip(right) {
        if !same_head_shape(left, right) {
            return None;
        }
        let classification = classify_clone(
            &StructuralTree::from_view(left),
            &StructuralTree::from_view(right),
        );
        if classification.clone_type == CloneType::Type3 {
            return None;
        }
        clone_type = clone_type.max(classification.clone_type);
        consistent &= classification.consistent_renaming;
    }
    Some((clone_type, consistent))
}

/// Structural equality with every list's head symbol pinned.
///
/// Non-head atoms are free, so a renamed variable still matches; a different
/// operator does not.
fn same_head_shape(left: &ExpressionView, right: &ExpressionView) -> bool {
    let mut pending = vec![(left, right)];
    while let Some((left, right)) = pending.pop() {
        if left.kind != right.kind
            || left.delimiter != right.delimiter
            || left.reader_prefixes != right.reader_prefixes
            || left.children.len() != right.children.len()
        {
            return false;
        }
        for (index, (left_child, right_child)) in
            left.children.iter().zip(&right.children).enumerate()
        {
            if index == 0 && atom_text(left_child) != atom_text(right_child) {
                return false;
            }
            pending.push((left_child, right_child));
        }
    }
    true
}

#[derive(Debug, Clone, Copy)]
struct ChildMetrics {
    fingerprint: u64,
    node_count: usize,
}

/// One postorder pass giving every node its digest and its size.
///
/// Building a `StructuralTree` per child would be the obvious implementation
/// and is quadratic in depth; this is linear, and the trees are only built
/// later, for the handful of runs that reach verification.
fn subtree_metrics(
    root: &ExpressionView,
    match_mode: SequenceMatchMode,
) -> HashMap<ByteSpan, ChildMetrics> {
    enum Frame<'a> {
        Enter(&'a ExpressionView),
        Leave(&'a ExpressionView),
    }

    let mut metrics: HashMap<ByteSpan, ChildMetrics> = HashMap::new();
    let mut pending = vec![Frame::Enter(root)];
    while let Some(frame) = pending.pop() {
        match frame {
            Frame::Enter(view) => {
                pending.push(Frame::Leave(view));
                pending.extend(view.children.iter().rev().map(Frame::Enter));
            }
            Frame::Leave(view) => {
                let mut hasher = DefaultHasher::new();
                kind_tag(view.kind).hash(&mut hasher);
                delimiter_tag(view.delimiter).hash(&mut hasher);
                for prefix in &view.reader_prefixes {
                    (*prefix as u8).hash(&mut hasher);
                }
                if match_mode == SequenceMatchMode::Exact && view.kind == ExpressionKind::Atom {
                    view.text.as_deref().unwrap_or_default().hash(&mut hasher);
                }
                view.children.len().hash(&mut hasher);
                let mut node_count = 1usize;
                for child in &view.children {
                    let child_metrics = metrics[&child.span];
                    child_metrics.fingerprint.hash(&mut hasher);
                    node_count = node_count.saturating_add(child_metrics.node_count);
                }
                metrics.insert(
                    view.span,
                    ChildMetrics {
                        fingerprint: hasher.finish(),
                        node_count,
                    },
                );
            }
        }
    }
    metrics
}

const fn kind_tag(kind: ExpressionKind) -> u8 {
    match kind {
        ExpressionKind::Root => 0,
        ExpressionKind::Atom => 1,
        ExpressionKind::List => 2,
    }
}

const fn delimiter_tag(delimiter: Option<Delimiter>) -> u8 {
    match delimiter {
        None => 0,
        Some(Delimiter::Paren) => 1,
        Some(Delimiter::Bracket) => 2,
        Some(Delimiter::Brace) => 3,
    }
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
        .filter(|text| !text.is_empty())
}
