//! Duplicate-form analysis and replacement planning rules.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path as FsPath, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::form_shape::{FormShape, duplicate_shape};
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SyntaxTree,
};

#[derive(Debug, Clone)]
pub struct DuplicateFormReport {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub form_path: Path,
    pub span: ByteSpan,
    pub node_count: usize,
    pub head: Option<String>,
    pub text: String,
}

#[derive(Debug)]
pub struct DuplicateShapeReport {
    pub shape: FormShape,
    pub count: usize,
    pub forms: Vec<DuplicateFormReport>,
}

#[derive(Debug)]
pub struct ReplacementPlanBatch {
    pub file: PathBuf,
    pub dialect: Dialect,
    pub shape: FormShape,
    pub replacement: String,
    pub keep_first: bool,
    pub forms: Vec<DuplicateFormReport>,
}

pub type DuplicateCandidateGroups = BTreeMap<FormShape, Vec<DuplicateFormReport>>;

#[derive(Debug)]
pub struct DuplicateCandidateAccumulator {
    min_node_count: usize,
    sources: Vec<DuplicateSource>,
    candidates: HashMap<u64, Vec<CandidateLocator>>,
}

#[derive(Debug)]
struct DuplicateSource {
    tree: SyntaxTree,
    path: PathBuf,
    dialect: Dialect,
}

#[derive(Debug, Clone)]
struct CandidateLocator {
    source_index: usize,
    path: Path,
    span: ByteSpan,
    node_count: usize,
}

impl DuplicateCandidateAccumulator {
    #[must_use]
    pub fn new(min_node_count: usize) -> Self {
        Self {
            min_node_count,
            sources: Vec::new(),
            candidates: HashMap::new(),
        }
    }

    pub fn add_source(&mut self, tree: SyntaxTree, path: PathBuf, dialect: Dialect) -> Result<()> {
        let source_index = self.sources.len();
        for index in 0..tree.root_children().len() {
            let root_path = Path::root_child(index);
            let view = tree.select_path(&root_path)?.view();
            let metrics = subtree_metrics(&view);
            collect_candidate_locators(
                &view,
                &root_path,
                self.min_node_count,
                source_index,
                &metrics,
                &mut self.candidates,
            );
        }
        self.sources.push(DuplicateSource {
            tree,
            path,
            dialect,
        });
        Ok(())
    }

    pub fn finish(self, min_group_size: usize) -> Result<DuplicateCandidateGroups> {
        let mut grouped = DuplicateCandidateGroups::new();
        for bucket in self.candidates.into_values() {
            let mut partition_by_shape = HashMap::<FormShape, usize>::new();
            let mut partitions = Vec::<(FormShape, Vec<CandidateLocator>)>::new();
            for candidate in bucket {
                let shape = duplicate_shape(&locator_view(&self.sources, &candidate)?, true);
                if let Some(&index) = partition_by_shape.get(&shape) {
                    partitions[index].1.push(candidate);
                } else {
                    let index = partitions.len();
                    partition_by_shape.insert(shape.clone(), index);
                    partitions.push((shape, vec![candidate]));
                }
            }

            for (shape, partition) in partitions {
                if partition.len() < min_group_size {
                    continue;
                }
                let forms = partition
                    .into_iter()
                    .map(|candidate| materialize_candidate(&self.sources, candidate))
                    .collect::<Result<Vec<_>>>()?;
                grouped.insert(shape, forms);
            }
        }
        Ok(grouped)
    }
}

fn collect_candidate_locators(
    view: &ExpressionView,
    root_path: &Path,
    min_node_count: usize,
    source_index: usize,
    metrics: &HashMap<ByteSpan, SubtreeMetrics>,
    candidates: &mut HashMap<u64, Vec<CandidateLocator>>,
) {
    struct Frame<'a> {
        view: &'a ExpressionView,
        next_child: usize,
    }

    let mut path = root_path
        .indexes()
        .iter()
        .map(|index| index.get())
        .collect::<Vec<_>>();
    let mut frames = vec![Frame {
        view,
        next_child: 0,
    }];

    while let Some(frame) = frames.last_mut() {
        if frame.next_child == 0 {
            let view_metrics = metrics[&frame.view.span];
            if frame.view.kind == ExpressionKind::List
                && frame.view.delimiter == Some(Delimiter::Paren)
                && view_metrics.node_count >= min_node_count
            {
                candidates
                    .entry(view_metrics.candidate_fingerprint)
                    .or_default()
                    .push(CandidateLocator {
                        source_index,
                        path: Path::from_indexes(path.clone()),
                        span: frame.view.span,
                        node_count: view_metrics.node_count,
                    });
            }
        }

        if let Some(child) = frame.view.children.get(frame.next_child) {
            let child_index = frame.next_child;
            frame.next_child += 1;
            path.push(child_index);
            frames.push(Frame {
                view: child,
                next_child: 0,
            });
        } else {
            frames.pop();
            if !frames.is_empty() {
                path.pop();
            }
        }
    }
}

fn locator_view(
    sources: &[DuplicateSource],
    candidate: &CandidateLocator,
) -> Result<ExpressionView> {
    Ok(sources[candidate.source_index]
        .tree
        .select_path(&candidate.path)?
        .view())
}

fn materialize_candidate(
    sources: &[DuplicateSource],
    candidate: CandidateLocator,
) -> Result<DuplicateFormReport> {
    let source = &sources[candidate.source_index];
    let selection = source.tree.select_path(&candidate.path)?;
    let view = selection.view();
    Ok(DuplicateFormReport {
        path: source.path.clone(),
        dialect: source.dialect,
        form_path: candidate.path,
        span: candidate.span,
        node_count: candidate.node_count,
        head: view
            .children
            .first()
            .and_then(atom_text)
            .map(ToOwned::to_owned),
        text: selection.text().to_owned(),
    })
}

pub fn collect_duplicate_candidates(
    tree: &SyntaxTree,
    input: &str,
    file: &FsPath,
    dialect: Dialect,
    min_node_count: usize,
    grouped: &mut DuplicateCandidateGroups,
) -> Result<()> {
    let mut path_stack = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&Path::root_child(index))?.view();
        let metrics = subtree_metrics(&view);
        let repeated_fingerprints =
            repeated_candidate_fingerprints(&view, min_node_count, &metrics);
        path_stack.push(index);
        CandidateTraversal {
            input,
            file,
            dialect,
            min_node_count,
            metrics: &metrics,
            path_stack: &mut path_stack,
            grouped,
            repeated_fingerprints: &repeated_fingerprints,
            shape_cache: HashMap::new(),
        }
        .walk(&view);
        path_stack.pop();
    }

    Ok(())
}

// Candidate traversal remains pre-order so forms retain their existing deterministic order.
struct CandidateTraversal<'a> {
    input: &'a str,
    file: &'a FsPath,
    dialect: Dialect,
    min_node_count: usize,
    metrics: &'a HashMap<ByteSpan, SubtreeMetrics>,
    path_stack: &'a mut Vec<usize>,
    grouped: &'a mut DuplicateCandidateGroups,
    repeated_fingerprints: &'a HashSet<u64>,
    shape_cache: HashMap<u64, Vec<CachedShape<'a>>>,
}

struct CachedShape<'a> {
    representative: &'a ExpressionView,
    shape: FormShape,
}

impl<'a> CandidateTraversal<'a> {
    fn walk(&mut self, view: &'a ExpressionView) {
        if view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Paren) {
            let metrics = self.metrics[&view.span];
            let node_count = metrics.node_count;
            if node_count >= self.min_node_count {
                let report = DuplicateFormReport {
                    path: self.file.to_path_buf(),
                    dialect: self.dialect,
                    form_path: Path::from_indexes(self.path_stack.clone()),
                    span: view.span,
                    node_count,
                    head: view
                        .children
                        .first()
                        .and_then(atom_text)
                        .map(ToOwned::to_owned),
                    text: view.span.slice(self.input).to_owned(),
                };
                if self
                    .repeated_fingerprints
                    .contains(&metrics.candidate_fingerprint)
                {
                    if let Some(cached) = self
                        .shape_cache
                        .get(&metrics.candidate_fingerprint)
                        .and_then(|bucket| {
                            bucket
                                .iter()
                                .find(|cached| same_duplicate_shape(cached.representative, view))
                        })
                    {
                        self.grouped
                            .get_mut(&cached.shape)
                            .expect("cached shape must remain in candidate groups")
                            .push(report);
                    } else {
                        let shape = duplicate_shape(view, true);
                        self.grouped.entry(shape.clone()).or_default().push(report);
                        self.shape_cache
                            .entry(metrics.candidate_fingerprint)
                            .or_default()
                            .push(CachedShape {
                                representative: view,
                                shape,
                            });
                    }
                } else {
                    let shape = duplicate_shape(view, true);
                    self.grouped.entry(shape).or_default().push(report);
                }
            }
        }

        for (index, child) in view.children.iter().enumerate() {
            self.path_stack.push(index);
            self.walk(child);
            self.path_stack.pop();
        }
    }
}

fn repeated_candidate_fingerprints(
    root: &ExpressionView,
    min_node_count: usize,
    metrics: &HashMap<ByteSpan, SubtreeMetrics>,
) -> HashSet<u64> {
    let mut counts = HashMap::<u64, usize>::new();
    let mut pending = vec![root];
    while let Some(view) = pending.pop() {
        let view_metrics = metrics[&view.span];
        if view.kind == ExpressionKind::List
            && view.delimiter == Some(Delimiter::Paren)
            && view_metrics.node_count >= min_node_count
        {
            counts
                .entry(view_metrics.candidate_fingerprint)
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }
        pending.extend(view.children.iter());
    }
    counts
        .into_iter()
        .filter_map(|(fingerprint, count)| (count > 1).then_some(fingerprint))
        .collect()
}

fn same_duplicate_shape(left: &ExpressionView, right: &ExpressionView) -> bool {
    let mut pending = vec![(left, right, true)];
    while let Some((left, right, preserve_head)) = pending.pop() {
        if left.kind != right.kind
            || left.delimiter != right.delimiter
            || left.children.len() != right.children.len()
        {
            return false;
        }
        for (index, (left_child, right_child)) in
            left.children.iter().zip(&right.children).enumerate()
        {
            if preserve_head && index == 0 {
                match (atom_text(left_child), atom_text(right_child)) {
                    (Some(left), Some(right)) if left == right => {}
                    (Some(_), Some(_)) | (Some(_), None) | (None, Some(_)) => return false,
                    (None, None) => pending.push((left_child, right_child, false)),
                }
            } else {
                pending.push((left_child, right_child, false));
            }
        }
    }
    true
}

#[must_use]
pub fn collect_replacement_plan_batches(
    grouped: DuplicateCandidateGroups,
    min_group_size: usize,
    replacement: String,
    keep_first: bool,
) -> Vec<ReplacementPlanBatch> {
    let mut batches = Vec::new();

    for (shape, forms) in grouped {
        let mut by_file = BTreeMap::<PathBuf, Vec<DuplicateFormReport>>::new();
        for form in forms {
            by_file.entry(form.path.clone()).or_default().push(form);
        }

        for (file, mut file_forms) in by_file {
            if file_forms.len() < min_group_size {
                continue;
            }

            file_forms.sort_by_key(|form| form.span.start().get());
            let Some(first_form) = file_forms.first() else {
                continue;
            };

            batches.push(ReplacementPlanBatch {
                file,
                dialect: first_form.dialect,
                shape: shape.clone(),
                replacement: replacement.clone(),
                keep_first,
                forms: file_forms,
            });
        }
    }

    batches
}

#[must_use]
pub fn build_duplicate_shape_reports(
    grouped: DuplicateCandidateGroups,
    min_group_size: usize,
) -> Vec<DuplicateShapeReport> {
    let mut reports = grouped
        .into_iter()
        .filter_map(|(shape, forms)| {
            (forms.len() >= min_group_size).then_some(DuplicateShapeReport {
                count: forms.len(),
                shape,
                forms,
            })
        })
        .collect::<Vec<_>>();

    reports.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.shape.cmp(&right.shape))
    });

    reports
}

#[derive(Clone, Copy)]
struct SubtreeMetrics {
    node_count: usize,
    generic_fingerprint: u64,
    candidate_fingerprint: u64,
}

fn subtree_metrics(root: &ExpressionView) -> HashMap<ByteSpan, SubtreeMetrics> {
    enum Frame<'a> {
        Enter(&'a ExpressionView),
        Leave(&'a ExpressionView),
    }

    let mut metrics: HashMap<ByteSpan, SubtreeMetrics> = HashMap::new();
    let mut pending = vec![Frame::Enter(root)];
    while let Some(frame) = pending.pop() {
        match frame {
            Frame::Enter(view) => {
                pending.push(Frame::Leave(view));
                pending.extend(view.children.iter().rev().map(Frame::Enter));
            }
            Frame::Leave(view) => {
                let child_nodes = view
                    .children
                    .iter()
                    .map(|child| metrics[&child.span].node_count)
                    .fold(0usize, usize::saturating_add);
                let generic_fingerprint = shape_fingerprint(view, &metrics, false);
                let candidate_fingerprint = shape_fingerprint(view, &metrics, true);
                metrics.insert(
                    view.span,
                    SubtreeMetrics {
                        node_count: child_nodes.saturating_add(1),
                        generic_fingerprint,
                        candidate_fingerprint,
                    },
                );
            }
        }
    }
    metrics
}

fn shape_fingerprint(
    view: &ExpressionView,
    metrics: &HashMap<ByteSpan, SubtreeMetrics>,
    preserve_head: bool,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    expression_kind_tag(view.kind).hash(&mut hasher);
    delimiter_tag(view.delimiter).hash(&mut hasher);
    view.children.len().hash(&mut hasher);
    for (index, child) in view.children.iter().enumerate() {
        if preserve_head && index == 0 {
            match atom_text(child) {
                Some(head) => {
                    true.hash(&mut hasher);
                    head.hash(&mut hasher);
                }
                None => {
                    false.hash(&mut hasher);
                    metrics[&child.span].generic_fingerprint.hash(&mut hasher);
                }
            }
        } else {
            metrics[&child.span].generic_fingerprint.hash(&mut hasher);
        }
    }
    hasher.finish()
}

const fn expression_kind_tag(kind: ExpressionKind) -> u8 {
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

#[cfg(test)]
fn subtree_node_counts(root: &ExpressionView) -> HashMap<ByteSpan, usize> {
    subtree_metrics(root)
        .into_iter()
        .map(|(span, metrics)| (span, metrics.node_count))
        .collect()
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .and_then(|text| text)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use std::path::{Path as FsPath, PathBuf};

    use proptest::prelude::*;

    use super::*;
    use paredit_core_syntax::sexpr::{ByteOffset, Path as ExpressionPath};

    #[test]
    fn groups_duplicate_forms_by_shape() {
        let input = "(+ a b)\n(+ c d)\n(* a b)\n";
        let tree = SyntaxTree::parse(input).expect("parse input");
        let mut grouped = DuplicateCandidateGroups::new();

        collect_duplicate_candidates(
            &tree,
            input,
            FsPath::new("sample.lisp"),
            Dialect::CommonLisp,
            3,
            &mut grouped,
        )
        .expect("collect candidates");

        let reports = build_duplicate_shape_reports(grouped, 2);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].count, 2);
        assert_eq!(
            reports[0].forms[0].form_path,
            ExpressionPath::from_indexes(vec![0])
        );
        assert_eq!(
            reports[0].forms[1].form_path,
            ExpressionPath::from_indexes(vec![1])
        );
    }

    #[test]
    fn subtree_counts_are_computed_once_for_a_deep_tree() {
        let depth = 256;
        let input = format!("{}leaf{}", "(h ".repeat(depth), ")".repeat(depth));
        let tree = SyntaxTree::parse(&input).expect("parse deep input");
        let view = tree
            .select_path(&ExpressionPath::root_child(0))
            .expect("select root form")
            .view();

        let counts = subtree_node_counts(&view);

        assert_eq!(counts.len(), depth * 2 + 1);
        assert_eq!(counts[&view.span], depth * 2 + 1);
    }

    #[test]
    fn deep_candidate_counts_and_owned_output_are_preserved() {
        let depth = 64;
        let input = format!("{}leaf{}", "(h ".repeat(depth), ")".repeat(depth));
        let tree = SyntaxTree::parse(&input).expect("parse deep input");
        let mut grouped = DuplicateCandidateGroups::new();

        collect_duplicate_candidates(
            &tree,
            &input,
            FsPath::new("deep.lisp"),
            Dialect::CommonLisp,
            1,
            &mut grouped,
        )
        .expect("collect deep candidates");

        let forms = grouped.values().flatten().collect::<Vec<_>>();
        assert_eq!(forms.len(), depth);
        assert!(forms.iter().any(|form| {
            form.form_path == ExpressionPath::root_child(0)
                && form.node_count == depth * 2 + 1
                && form.text == input
        }));
        assert!(forms.iter().any(|form| form.node_count == 3));
    }

    #[test]
    fn fingerprint_collisions_still_require_exact_shape_equality() {
        let input = "(outer (alpha x) (beta x))";
        let tree = SyntaxTree::parse(input).expect("parse");
        let view = tree
            .select_path(&ExpressionPath::root_child(0))
            .expect("select root form")
            .view();
        let mut metrics = subtree_metrics(&view);
        metrics
            .get_mut(&view.children[0].span)
            .expect("alpha metrics")
            .candidate_fingerprint = 7;
        metrics
            .get_mut(&view.children[1].span)
            .expect("beta metrics")
            .candidate_fingerprint = 7;
        let mut path_stack = vec![0];
        let mut grouped = DuplicateCandidateGroups::new();
        let repeated_fingerprints = HashSet::from([7]);

        CandidateTraversal {
            input,
            file: FsPath::new("collision.lisp"),
            dialect: Dialect::CommonLisp,
            min_node_count: 1,
            metrics: &metrics,
            path_stack: &mut path_stack,
            grouped: &mut grouped,
            repeated_fingerprints: &repeated_fingerprints,
            shape_cache: HashMap::new(),
        }
        .walk(&view);

        assert_eq!(grouped.len(), 3);
        assert!(
            grouped
                .keys()
                .any(|shape| shape.as_str().contains("head:alpha"))
        );
        assert!(
            grouped
                .keys()
                .any(|shape| shape.as_str().contains("head:beta"))
        );
    }

    #[test]
    fn deep_unique_output_size_is_quadratic_by_public_contract() {
        fn owned_payload(depth: usize) -> usize {
            let input = format!("{}leaf{}", "(h ".repeat(depth), ")".repeat(depth));
            let tree = SyntaxTree::parse(&input).expect("parse");
            let mut grouped = DuplicateCandidateGroups::new();
            collect_duplicate_candidates(
                &tree,
                &input,
                FsPath::new("deep.lisp"),
                Dialect::CommonLisp,
                1,
                &mut grouped,
            )
            .expect("collect");
            grouped
                .iter()
                .map(|(shape, forms)| {
                    shape.as_str().len()
                        + forms
                            .iter()
                            .map(|form| {
                                form.text.len()
                                    + form.path.as_os_str().len()
                                    + form.form_path.indexes().len()
                            })
                            .sum::<usize>()
                })
                .sum()
        }

        let shallow = owned_payload(32);
        let deep = owned_payload(64);
        assert!(deep > shallow * 3);
    }

    #[test]
    fn lazy_accumulator_materializes_only_cross_source_duplicates() {
        let mut candidates = DuplicateCandidateAccumulator::new(3);
        candidates
            .add_source(
                SyntaxTree::parse("(+ a b)\n(unique a b c)").expect("parse first"),
                PathBuf::from("a.lisp"),
                Dialect::CommonLisp,
            )
            .expect("add first");
        candidates
            .add_source(
                SyntaxTree::parse("(+ c d)").expect("parse second"),
                PathBuf::from("b.lisp"),
                Dialect::CommonLisp,
            )
            .expect("add second");

        let grouped = candidates.finish(2).expect("finish");
        let reports = build_duplicate_shape_reports(grouped, 2);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].count, 2);
        assert_eq!(reports[0].forms[0].path, PathBuf::from("a.lisp"));
        assert_eq!(reports[0].forms[0].text, "(+ a b)");
        assert_eq!(reports[0].forms[1].path, PathBuf::from("b.lisp"));
        assert_eq!(reports[0].forms[1].text, "(+ c d)");
    }

    #[test]
    fn lazy_accumulator_partitions_a_large_identical_bucket() {
        const CANDIDATE_COUNT: usize = 1_000;
        let input = "(+ value offset)\n".repeat(CANDIDATE_COUNT);
        let mut candidates = DuplicateCandidateAccumulator::new(3);
        candidates
            .add_source(
                SyntaxTree::parse(&input).expect("parse"),
                PathBuf::from("large.lisp"),
                Dialect::CommonLisp,
            )
            .expect("add source");

        let grouped = candidates.finish(2).expect("finish");
        let reports = build_duplicate_shape_reports(grouped, 2);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].count, CANDIDATE_COUNT);
        assert_eq!(
            reports[0].forms.first().expect("first form").form_path,
            Path::root_child(0)
        );
        assert_eq!(
            reports[0].forms.last().expect("last form").form_path,
            Path::root_child(CANDIDATE_COUNT - 1)
        );
    }

    #[test]
    #[ignore = "release scaling probe; set DUPLICATE_CANDIDATE_COUNT"]
    fn duplicate_accumulator_scaling_probe() {
        let candidate_count = std::env::var("DUPLICATE_CANDIDATE_COUNT")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1_000);
        let input = "(+ value offset)\n".repeat(candidate_count);
        let started = std::time::Instant::now();
        let mut candidates = DuplicateCandidateAccumulator::new(3);
        candidates
            .add_source(
                SyntaxTree::parse(&input).expect("parse"),
                PathBuf::from("scaling.lisp"),
                Dialect::CommonLisp,
            )
            .expect("add source");
        let grouped = candidates.finish(2).expect("finish");
        let elapsed = started.elapsed();

        assert_eq!(
            grouped.values().map(Vec::len).sum::<usize>(),
            candidate_count
        );
        eprintln!("duplicate candidates={candidate_count} elapsed={elapsed:?}");
    }

    #[test]
    fn lazy_accumulator_materializes_nested_candidates_from_collected_paths() {
        let input = "(wrap (+ left right) (+ first second))";
        let mut candidates = DuplicateCandidateAccumulator::new(3);
        candidates
            .add_source(
                SyntaxTree::parse(input).expect("parse"),
                PathBuf::from("nested.lisp"),
                Dialect::CommonLisp,
            )
            .expect("add source");

        let grouped = candidates.finish(2).expect("finish");
        let reports = build_duplicate_shape_reports(grouped, 2);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].count, 2);
        assert_eq!(
            reports[0].forms[0].form_path,
            Path::from_indexes(vec![0, 1])
        );
        assert_eq!(reports[0].forms[0].text, "(+ left right)");
        assert_eq!(reports[0].forms[0].span.slice(input), "(+ left right)");
        assert_eq!(
            reports[0].forms[1].form_path,
            Path::from_indexes(vec![0, 2])
        );
        assert_eq!(reports[0].forms[1].text, "(+ first second)");
        assert_eq!(reports[0].forms[1].span.slice(input), "(+ first second)");
    }

    #[test]
    fn lazy_accumulator_drops_deep_unique_candidates() {
        let depth = 128;
        let input = format!("{}leaf{}", "(h ".repeat(depth), ")".repeat(depth));
        let mut candidates = DuplicateCandidateAccumulator::new(1);
        candidates
            .add_source(
                SyntaxTree::parse(&input).expect("parse"),
                PathBuf::from("deep.lisp"),
                Dialect::CommonLisp,
            )
            .expect("add");

        assert!(candidates.finish(2).expect("finish").is_empty());
    }

    #[test]
    fn replacement_batches_are_partitioned_per_file() {
        let shape = FormShape::from("(+ _ _)");
        let span = ByteSpan::new(ByteOffset::new(0), ByteOffset::new(7));
        let mut grouped = DuplicateCandidateGroups::new();
        grouped.insert(
            shape.clone(),
            vec![
                DuplicateFormReport {
                    path: PathBuf::from("a.lisp"),
                    dialect: Dialect::CommonLisp,
                    form_path: ExpressionPath::from_indexes(vec![1]),
                    span,
                    node_count: 4,
                    head: Some("+".to_owned()),
                    text: "(+ c d)".to_owned(),
                },
                DuplicateFormReport {
                    path: PathBuf::from("a.lisp"),
                    dialect: Dialect::CommonLisp,
                    form_path: ExpressionPath::from_indexes(vec![0]),
                    span: ByteSpan::new(ByteOffset::new(8), ByteOffset::new(15)),
                    node_count: 4,
                    head: Some("+".to_owned()),
                    text: "(+ a b)".to_owned(),
                },
                DuplicateFormReport {
                    path: PathBuf::from("b.lisp"),
                    dialect: Dialect::CommonLisp,
                    form_path: ExpressionPath::from_indexes(vec![0]),
                    span,
                    node_count: 4,
                    head: Some("+".to_owned()),
                    text: "(+ e f)".to_owned(),
                },
            ],
        );

        let batches =
            collect_replacement_plan_batches(grouped, 2, "(helper _ _)".to_owned(), false);

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].file, PathBuf::from("a.lisp"));
        assert_eq!(batches[0].shape, shape);
        assert!(!batches[0].keep_first);
        assert_eq!(batches[0].forms[0].span.start().get(), 0);
        assert_eq!(batches[0].forms[1].span.start().get(), 8);
    }

    proptest! {
        #[test]
        fn pbt_repeated_binary_calls_are_reported_as_one_duplicate_shape(
            count in 2usize..12,
            head in "[a-z]{1,8}",
            lhs in "[a-z]{1,8}",
            rhs in "[a-z]{1,8}",
        ) {
            let forms = (0..count)
                .map(|index| format!("({head} {lhs}{index} {rhs}{index})"))
                .collect::<Vec<_>>();
            let input = forms.join("\n");
            let tree = SyntaxTree::parse(&input).expect("parse generated input");
            let mut grouped = DuplicateCandidateGroups::new();

            collect_duplicate_candidates(
                &tree,
                &input,
                FsPath::new("generated.lisp"),
                Dialect::CommonLisp,
                3,
                &mut grouped,
            )
            .expect("collect generated candidates");

            let reports = build_duplicate_shape_reports(grouped, count);
            prop_assert_eq!(reports.len(), 1);
            prop_assert_eq!(reports[0].count, count);
            prop_assert!(reports[0]
                .forms
                .iter()
                .all(|form| form.head.as_deref() == Some(head.as_str())));
        }
    }
}
