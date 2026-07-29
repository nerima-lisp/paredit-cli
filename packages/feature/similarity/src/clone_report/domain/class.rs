//! Clone classes: grouping, taxonomy labelling, and extraction ranking.
//!
//! `similarity` reports pairs. Pairs are the wrong unit for acting on: five
//! copies of the same helper produce ten pairs, and reading ten pairs to
//! discover there is one thing to extract is work a report should have done.
//!
//! A class here is a connected component of the similar-pair graph. That is a
//! deliberately loose grouping — similarity is not transitive, so a component
//! can chain A~B~C with A and C not themselves similar — but it is the grouping
//! that matches the decision being made. You do not extract A~B and separately
//! extract B~C; you look at the whole chain and decide what the common shape is.
//! The reported `min_similarity` is what tells you how loose a given chain got.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::SimilarityAnalysisResult;
use crate::form_similarity::{CloneClassification, CloneType, classify_clone};
use crate::similarity_report::domain::{
    SimilarityCandidate, SimilarityFormReport, SimilarityReportOptions, SimilarityReportSummary,
    build_similarity_pairs_with_omissions, structural_tree_of,
};

use super::shared::{FormKey, line_span_of};

/// Lines a helper costs that its body does not: the `(defun name (args)` line.
///
/// One line is the honest floor. Anything larger would be guessing at a naming
/// and formatting convention the report cannot see, and the estimate is already
/// labelled an estimate.
pub const DEFAULT_HELPER_OVERHEAD_LINES: usize = 1;

/// Whether a class wholly nested inside a bigger one is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassOverlapPolicy {
    /// Drop a class whose every member sits strictly inside a member of a
    /// higher-ranked class.
    Maximal,
    /// Report every class.
    All,
}

impl ClassOverlapPolicy {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Maximal => "maximal",
            Self::All => "all",
        }
    }
}

impl std::str::FromStr for ClassOverlapPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "maximal" => Ok(Self::Maximal),
            "all" => Ok(Self::All),
            _ => Err(format!("unknown overlap policy: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloneClassOptions {
    pub overlap_policy: ClassOverlapPolicy,
    pub min_members: usize,
    pub max_classes: Option<usize>,
    pub clone_type: Option<CloneType>,
    pub helper_overhead_lines: usize,
}

impl Default for CloneClassOptions {
    fn default() -> Self {
        Self {
            overlap_policy: ClassOverlapPolicy::Maximal,
            min_members: 2,
            max_classes: None,
            clone_type: None,
            helper_overhead_lines: DEFAULT_HELPER_OVERHEAD_LINES,
        }
    }
}

/// What extracting one class would cost and save, in lines and in nodes.
///
/// Every field is reported rather than just the bottom line, because the bottom
/// line is an estimate built on an assumption — one call site is one line — that
/// the reader may want to overrule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractionEstimate {
    pub member_count: usize,
    pub file_count: usize,
    pub total_lines: usize,
    pub total_nodes: usize,
    /// The largest member, which is the one the helper's body has to cover.
    pub representative_lines: usize,
    pub representative_nodes: usize,
    pub helper_overhead_lines: usize,
    /// One call per site, one line each.
    pub call_site_lines: usize,
    pub retained_lines: usize,
    pub saved_lines: usize,
    pub saved_nodes: usize,
}

/// One class member reduced to the three numbers the estimate needs.
///
/// Lets the same arithmetic serve whole-form classes and sibling-run groups,
/// which have no report type in common.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberSize<'a> {
    pub path: &'a std::path::Path,
    pub lines: usize,
    pub nodes: usize,
}

impl ExtractionEstimate {
    fn new(members: &[Arc<SimilarityFormReport>], helper_overhead_lines: usize) -> Self {
        let sizes = members
            .iter()
            .map(|member| MemberSize {
                path: member.path(),
                lines: line_span_of(member.text().as_ref()),
                nodes: member.node_count(),
            })
            .collect::<Vec<_>>();
        Self::from_sizes(&sizes, helper_overhead_lines)
    }

    #[must_use]
    pub fn from_sizes(members: &[MemberSize<'_>], helper_overhead_lines: usize) -> Self {
        let member_count = members.len();
        let mut files = members
            .iter()
            .map(|member| member.path.to_path_buf())
            .collect::<Vec<_>>();
        files.sort_unstable();
        files.dedup();

        let total_lines = members
            .iter()
            .map(|member| member.lines)
            .fold(0, usize::saturating_add);
        let representative_lines = members.iter().map(|member| member.lines).max().unwrap_or(0);
        let total_nodes = members
            .iter()
            .map(|member| member.nodes)
            .fold(0, usize::saturating_add);
        let representative_nodes = members.iter().map(|member| member.nodes).max().unwrap_or(0);

        let retained_lines = representative_lines
            .saturating_add(helper_overhead_lines)
            .saturating_add(member_count);

        Self {
            member_count,
            file_count: files.len(),
            total_lines,
            total_nodes,
            representative_lines,
            representative_nodes,
            helper_overhead_lines,
            call_site_lines: member_count,
            retained_lines,
            saved_lines: total_lines.saturating_sub(retained_lines),
            // A call site is one node in the sense that matters here: the whole
            // form collapses to `(helper ...)`, whose own arguments are not
            // counted because the report cannot know how many there will be.
            saved_nodes: total_nodes
                .saturating_sub(representative_nodes)
                .saturating_sub(member_count),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneClass {
    /// 1-based rank in the report, so a caller can cite "class 3" stably.
    pub rank: usize,
    pub clone_type: CloneType,
    /// True only when every edge in the class is a bijective substitution.
    /// Meaningless, and reported false, once the class is Type-3.
    pub consistent_renaming: bool,
    pub renamed_atoms: usize,
    pub members: Vec<Arc<SimilarityFormReport>>,
    pub edge_count: usize,
    pub unclassified_edges: usize,
    pub min_similarity: f64,
    pub max_similarity: f64,
    pub mean_similarity: f64,
    pub extraction: ExtractionEstimate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloneClassReport {
    pub summary: SimilarityReportSummary,
    pub classes: Vec<CloneClass>,
    /// Classes dropped by `--min-members` or `--clone-type`.
    pub filtered_classes: usize,
    /// Classes dropped for sitting wholly inside a higher-ranked class.
    pub nested_classes: usize,
    pub truncated_classes: usize,
    pub total_classes: usize,
    pub unclassified_edges: usize,
}

impl CloneClassReport {
    #[must_use]
    pub fn saved_lines(&self) -> usize {
        self.classes
            .iter()
            .map(|class| class.extraction.saved_lines)
            .fold(0, usize::saturating_add)
    }
}

pub fn build_clone_class_report(
    candidates: Vec<SimilarityCandidate>,
    omitted_candidates: usize,
    options: &SimilarityReportOptions,
    class_options: &CloneClassOptions,
) -> SimilarityAnalysisResult<CloneClassReport> {
    let report = build_similarity_pairs_with_omissions(candidates, omitted_candidates, options)?;

    let mut forms = FormIndex::default();
    let mut edges = Vec::with_capacity(report.pairs.len());
    for pair in &report.pairs {
        let left = forms.intern(pair.left_shared());
        let right = forms.intern(pair.right_shared());
        edges.push(Edge {
            left,
            right,
            similarity: pair.similarity().as_f64(),
        });
    }

    let mut components = DisjointSet::new(forms.forms.len());
    for edge in &edges {
        components.union(edge.left, edge.right);
    }

    let mut trees = TreeCache::default();
    let mut grouped = HashMap::<usize, ClassAccumulator>::new();
    let mut unclassified_edges = 0;
    for edge in &edges {
        let root = components.find(edge.left);
        let accumulator = grouped.entry(root).or_default();
        accumulator.edge_count += 1;
        accumulator.similarity_total += edge.similarity;
        accumulator.min_similarity = accumulator.min_similarity.min(edge.similarity);
        accumulator.max_similarity = accumulator.max_similarity.max(edge.similarity);

        match trees.classify(&forms.forms[edge.left], &forms.forms[edge.right]) {
            Some(classification) => {
                accumulator.clone_type = accumulator.clone_type.max(classification.clone_type);
                accumulator.consistent_renaming &= classification.consistent_renaming;
                accumulator.renamed_atoms =
                    accumulator.renamed_atoms.max(classification.renamed_atoms);
            }
            None => {
                accumulator.unclassified_edges += 1;
                unclassified_edges += 1;
                // An edge nobody could classify cannot be claimed to be a
                // rename, so the class falls back to the loosest label.
                accumulator.clone_type = CloneType::Type3;
            }
        }
    }

    for (index, form) in forms.forms.iter().enumerate() {
        let root = components.find(index);
        if let Some(accumulator) = grouped.get_mut(&root) {
            accumulator.members.push(Arc::clone(form));
        }
    }

    let total_classes = grouped.len();
    let mut classes = grouped
        .into_values()
        .map(|accumulator| accumulator.finish(class_options.helper_overhead_lines))
        .filter(|class| {
            class.members.len() >= class_options.min_members
                && class_options
                    .clone_type
                    .is_none_or(|wanted| class.clone_type == wanted)
        })
        .collect::<Vec<_>>();
    let filtered_classes = total_classes - classes.len();

    classes.sort_by(compare_classes);
    let nested_classes = match class_options.overlap_policy {
        ClassOverlapPolicy::All => 0,
        ClassOverlapPolicy::Maximal => {
            let before = classes.len();
            retain_maximal_classes(&mut classes);
            before - classes.len()
        }
    };
    let truncated_classes = class_options
        .max_classes
        .map_or(0, |limit| classes.len().saturating_sub(limit));
    if let Some(limit) = class_options.max_classes {
        classes.truncate(limit);
    }
    for (index, class) in classes.iter_mut().enumerate() {
        class.rank = index + 1;
    }

    Ok(CloneClassReport {
        summary: report.summary,
        classes,
        filtered_classes,
        nested_classes,
        truncated_classes,
        total_classes,
        unclassified_edges,
    })
}

/// Drops a class whose every member sits strictly inside a member of a class
/// already retained.
///
/// A duplicated `defun` contains a duplicated `let` containing a duplicated
/// `dolist`, and reporting all three states one fact three times — the two
/// inner classes are echoes that extracting the outer one removes.
///
/// Doing this on classes rather than on pairs is both cheaper and a better fit.
/// The pair-level `maximal` policy compares every retained pair against every
/// other as it goes, which is quadratic in the *matched pair* count and becomes
/// the dominant cost on exactly the repetitive input this report exists for.
/// Classes number in the hundreds where pairs number in the tens of thousands,
/// and "is this class an echo" is the question actually being asked.
fn retain_maximal_classes(classes: &mut Vec<CloneClass>) {
    let mut retained: Vec<CloneClass> = Vec::with_capacity(classes.len());
    for class in classes.drain(..) {
        let nested = class.members.iter().all(|member| {
            retained.iter().any(|outer| {
                outer
                    .members
                    .iter()
                    .any(|candidate| candidate.strictly_contains_span(member))
            })
        });
        if !nested {
            retained.push(class);
        }
    }
    *classes = retained;
}

/// Biggest win first, and every tie broken by something stable.
///
/// The final key is the first member's location, which is total and
/// deterministic, so two runs over the same tree emit the same ranking.
fn compare_classes(left: &CloneClass, right: &CloneClass) -> std::cmp::Ordering {
    right
        .extraction
        .saved_lines
        .cmp(&left.extraction.saved_lines)
        .then_with(|| right.members.len().cmp(&left.members.len()))
        .then_with(|| left.clone_type.cmp(&right.clone_type))
        .then_with(|| {
            right
                .max_similarity
                .partial_cmp(&left.max_similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            left.members
                .first()
                .map(|form| FormKey::of(form))
                .cmp(&right.members.first().map(|form| FormKey::of(form)))
        })
}

#[derive(Debug)]
struct Edge {
    left: usize,
    right: usize,
    similarity: f64,
}

#[derive(Debug, Default)]
struct FormIndex {
    forms: Vec<Arc<SimilarityFormReport>>,
    by_key: HashMap<FormKey, usize>,
}

impl FormIndex {
    fn intern(&mut self, form: &Arc<SimilarityFormReport>) -> usize {
        let key = FormKey::of(form);
        if let Some(&index) = self.by_key.get(&key) {
            return index;
        }
        let index = self.forms.len();
        self.forms.push(Arc::clone(form));
        self.by_key.insert(key, index);
        index
    }
}

/// Reparses each reported form at most once.
#[derive(Debug, Default)]
struct TreeCache {
    trees: HashMap<FormKey, Option<crate::form_similarity::StructuralTree>>,
}

impl TreeCache {
    fn get(
        &mut self,
        form: &SimilarityFormReport,
    ) -> Option<&crate::form_similarity::StructuralTree> {
        self.trees
            .entry(FormKey::of(form))
            .or_insert_with(|| structural_tree_of(form))
            .as_ref()
    }

    fn classify(
        &mut self,
        left: &SimilarityFormReport,
        right: &SimilarityFormReport,
    ) -> Option<CloneClassification> {
        // Two lookups rather than one because the cache hands out a borrow and
        // both sides have to be live at the same time. Cloning the smaller tree
        // would be the alternative, and a clone is more expensive than a hash.
        let left_tree = self.get(left)?.clone();
        let right_tree = self.get(right)?;
        Some(classify_clone(&left_tree, right_tree))
    }
}

#[derive(Debug)]
struct ClassAccumulator {
    members: Vec<Arc<SimilarityFormReport>>,
    edge_count: usize,
    unclassified_edges: usize,
    clone_type: CloneType,
    consistent_renaming: bool,
    renamed_atoms: usize,
    similarity_total: f64,
    min_similarity: f64,
    max_similarity: f64,
}

impl Default for ClassAccumulator {
    fn default() -> Self {
        Self {
            members: Vec::new(),
            edge_count: 0,
            unclassified_edges: 0,
            clone_type: CloneType::Type1,
            consistent_renaming: true,
            renamed_atoms: 0,
            similarity_total: 0.0,
            min_similarity: f64::INFINITY,
            max_similarity: f64::NEG_INFINITY,
        }
    }
}

impl ClassAccumulator {
    fn finish(mut self, helper_overhead_lines: usize) -> CloneClass {
        self.members.sort_by_key(|form| FormKey::of(form));
        let extraction = ExtractionEstimate::new(&self.members, helper_overhead_lines);
        let edge_count = self.edge_count;
        let mean_similarity = if edge_count == 0 {
            0.0
        } else {
            self.similarity_total / edge_count as f64
        };

        CloneClass {
            rank: 0,
            consistent_renaming: self.clone_type != CloneType::Type3 && self.consistent_renaming,
            clone_type: self.clone_type,
            renamed_atoms: self.renamed_atoms,
            members: self.members,
            edge_count,
            unclassified_edges: self.unclassified_edges,
            min_similarity: if edge_count == 0 {
                0.0
            } else {
                self.min_similarity
            },
            max_similarity: if edge_count == 0 {
                0.0
            } else {
                self.max_similarity
            },
            mean_similarity,
            extraction,
        }
    }
}

/// Union-find with path halving and union by size.
#[derive(Debug)]
struct DisjointSet {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl DisjointSet {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
            size: vec![1; len],
        }
    }

    fn find(&mut self, mut node: usize) -> usize {
        while self.parent[node] != node {
            self.parent[node] = self.parent[self.parent[node]];
            node = self.parent[node];
        }
        node
    }

    fn union(&mut self, left: usize, right: usize) {
        let (mut left, mut right) = (self.find(left), self.find(right));
        if left == right {
            return;
        }
        if self.size[left] < self.size[right] {
            std::mem::swap(&mut left, &mut right);
        }
        self.parent[right] = left;
        self.size[left] += self.size[right];
    }
}
