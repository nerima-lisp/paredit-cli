use std::collections::{HashMap, HashSet};
use std::path::Path as FsPath;
use std::sync::Arc;

use anyhow::Error as AnyhowError;
use thiserror::Error;

use crate::domain::common_lisp::normalize_common_lisp_operator_head;
use crate::domain::dialect::Dialect;
use crate::domain::form_similarity::StructuralTree;
use crate::domain::sexpr::{ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SyntaxTree};

use super::SimilarityReportOptionsError;
use super::types::{
    ComparisonHead, FormHead, SimilarityCandidate, SimilarityFormReport, SimilarityFormScope,
    SimilarityReportOptions,
};

const MAX_COLLECTED_CANDIDATE_NODES: usize = 1_000_000;
const MAX_COLLECTED_CANDIDATE_TEXT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum SimilarityCandidateCollectionError {
    #[error(transparent)]
    InvalidOptions(#[from] SimilarityReportOptionsError),
    #[error("failed to select an expression while collecting similarity candidates: {0}")]
    Selection(#[from] AnyhowError),
}

pub fn collect_similarity_candidates(
    tree: &SyntaxTree,
    input: &str,
    file: &FsPath,
    dialect: Dialect,
    options: &SimilarityReportOptions,
    candidates: &mut Vec<SimilarityCandidate>,
) -> std::result::Result<usize, SimilarityCandidateCollectionError> {
    collect_similarity_candidates_with_budgets(
        tree,
        input,
        file,
        dialect,
        options,
        candidates,
        MAX_COLLECTED_CANDIDATE_NODES,
        MAX_COLLECTED_CANDIDATE_TEXT_BYTES,
    )
    .map(|outcome| outcome.0)
}

#[allow(clippy::too_many_arguments)]
fn collect_similarity_candidates_with_budgets(
    tree: &SyntaxTree,
    input: &str,
    file: &FsPath,
    dialect: Dialect,
    options: &SimilarityReportOptions,
    candidates: &mut Vec<SimilarityCandidate>,
    max_collected_nodes: usize,
    max_collected_text_bytes: usize,
) -> std::result::Result<(usize, bool), SimilarityCandidateCollectionError> {
    options.validate()?;
    let line_index = (options.min_line_span() > 1).then_some(LineIndex::new(input));
    let collected_nodes = candidates
        .iter()
        .map(|candidate| candidate.form().node_count())
        .fold(0, usize::saturating_add);
    let mut retained_sources = HashSet::new();
    let collected_text_bytes = candidates
        .iter()
        .filter_map(|candidate| {
            retained_sources
                .insert(candidate.form().text().source_identity())
                .then_some(candidate.form().text().source_len())
        })
        .fold(0, usize::saturating_add);
    let mut collection = CandidateCollection {
        input,
        source: None,
        line_index: line_index.as_ref(),
        file,
        dialect,
        options,
        candidates,
        omitted_candidates: 0,
        collected_nodes,
        collected_text_bytes,
        source_counted: false,
        max_collected_nodes,
        max_collected_text_bytes,
    };
    let mut path_stack = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&Path::root_child(index))?.view();
        path_stack.push(index);
        collection.collect_from_view(&view, &mut path_stack);
        path_stack.pop();
    }
    Ok((collection.omitted_candidates, collection.source.is_some()))
}

struct CandidateCollection<'a> {
    input: &'a str,
    source: Option<Arc<str>>,
    line_index: Option<&'a LineIndex>,
    file: &'a FsPath,
    dialect: Dialect,
    options: &'a SimilarityReportOptions,
    candidates: &'a mut Vec<SimilarityCandidate>,
    omitted_candidates: usize,
    collected_nodes: usize,
    collected_text_bytes: usize,
    source_counted: bool,
    max_collected_nodes: usize,
    max_collected_text_bytes: usize,
}

impl CandidateCollection<'_> {
    fn collect_from_view(&mut self, view: &ExpressionView, path_stack: &mut Vec<usize>) {
        let subtree_node_counts = subtree_node_counts(view);

        enum Frame<'a> {
            Enter {
                view: &'a ExpressionView,
                index: Option<usize>,
            },
            Leave,
        }

        let mut pending = vec![Frame::Enter { view, index: None }];
        while let Some(frame) = pending.pop() {
            let Frame::Enter { view, index } = frame else {
                path_stack.pop();
                continue;
            };
            if let Some(index) = index {
                path_stack.push(index);
            }

            let is_top_level = path_stack.len() == 1;
            self.collect_candidate(
                view,
                path_stack,
                is_top_level,
                subtree_node_counts[&view.span],
            );

            if index.is_some() {
                pending.push(Frame::Leave);
            }
            if self.options.form_scope() == SimilarityFormScope::All {
                pending.extend(view.children.iter().enumerate().rev().map(|(index, view)| {
                    Frame::Enter {
                        view,
                        index: Some(index),
                    }
                }));
            }
        }
    }

    fn collect_candidate(
        &mut self,
        view: &ExpressionView,
        path_stack: &[usize],
        is_top_level: bool,
        node_count: usize,
    ) {
        if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
            return;
        }
        let line_span = self
            .line_index
            .map_or(1, |line_index| line_index.line_span(view.span));
        let in_scope = self.options.form_scope() == SimilarityFormScope::All || is_top_level;
        if !in_scope
            || line_span < self.options.min_line_span()
            || node_count < self.options.min_node_count()
        {
            return;
        }
        let source_bytes = if self.source_counted {
            0
        } else {
            self.input.len()
        };
        let over_limit = self
            .options
            .max_candidates()
            .is_some_and(|limit| self.candidates.len() >= limit)
            || self.collected_nodes.saturating_add(node_count) > self.max_collected_nodes
            || self.collected_text_bytes.saturating_add(source_bytes)
                > self.max_collected_text_bytes;
        if over_limit {
            self.omitted_candidates = self.omitted_candidates.saturating_add(1);
            return;
        }

        self.collected_nodes = self.collected_nodes.saturating_add(node_count);
        self.collected_text_bytes = self.collected_text_bytes.saturating_add(source_bytes);
        self.source_counted = true;
        let source = self.source.get_or_insert_with(|| Arc::from(self.input));
        let tree = StructuralTree::from_view_with_count(view).0;
        let head = view.children.first().and_then(atom_text);
        let form = SimilarityFormReport::new_shared(
            self.file.to_path_buf(),
            self.dialect,
            Path::from_indexes(path_stack.to_vec()),
            view.span,
            node_count,
            head.map(FormHead::from),
            Arc::clone(source),
        );
        let comparison_head = head.map(|head| {
            if self.dialect == Dialect::CommonLisp {
                ComparisonHead::from(normalize_common_lisp_operator_head(head).to_ascii_lowercase())
            } else {
                ComparisonHead::from(head.to_ascii_lowercase())
            }
        });
        self.candidates
            .push(SimilarityCandidate::new(form, tree, comparison_head));
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_similarity_candidates_with_budgets_for_test(
    tree: &SyntaxTree,
    input: &str,
    file: &FsPath,
    dialect: Dialect,
    options: &SimilarityReportOptions,
    candidates: &mut Vec<SimilarityCandidate>,
    max_collected_nodes: usize,
    max_collected_text_bytes: usize,
) -> std::result::Result<usize, SimilarityCandidateCollectionError> {
    collect_similarity_candidates_with_budgets(
        tree,
        input,
        file,
        dialect,
        options,
        candidates,
        max_collected_nodes,
        max_collected_text_bytes,
    )
    .map(|outcome| outcome.0)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_similarity_candidates_materialization_for_test(
    tree: &SyntaxTree,
    input: &str,
    file: &FsPath,
    dialect: Dialect,
    options: &SimilarityReportOptions,
    candidates: &mut Vec<SimilarityCandidate>,
    max_collected_nodes: usize,
    max_collected_text_bytes: usize,
) -> std::result::Result<(usize, bool), SimilarityCandidateCollectionError> {
    collect_similarity_candidates_with_budgets(
        tree,
        input,
        file,
        dialect,
        options,
        candidates,
        max_collected_nodes,
        max_collected_text_bytes,
    )
}

fn subtree_node_counts(root: &ExpressionView) -> HashMap<ByteSpan, usize> {
    enum Frame<'a> {
        Enter(&'a ExpressionView),
        Leave(&'a ExpressionView),
    }

    let mut counts = HashMap::new();
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
                    .map(|child| counts[&child.span])
                    .fold(0usize, usize::saturating_add);
                counts.insert(view.span, child_nodes.saturating_add(1));
            }
        }
    }
    counts
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

#[derive(Debug)]
struct LineIndex {
    newline_offsets: Vec<usize>,
}

impl LineIndex {
    fn new(input: &str) -> Self {
        let newline_offsets = input
            .bytes()
            .enumerate()
            .filter_map(|(index, byte)| (byte == b'\n').then_some(index))
            .collect();
        Self { newline_offsets }
    }

    fn line_span(&self, span: ByteSpan) -> usize {
        let start = span.start().get();
        let end = span.end().get();
        let start_index = self
            .newline_offsets
            .partition_point(|&offset| offset < start);
        let end_index = self.newline_offsets.partition_point(|&offset| offset < end);
        end_index.saturating_sub(start_index) + 1
    }
}
