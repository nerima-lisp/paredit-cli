use std::cmp::Ordering;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Condvar, Mutex};

use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, ReaderPrefix};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralTree {
    /// Order-sensitive digest of `labels`. Placed first so the derived
    /// `PartialEq` rejects unequal trees after one integer comparison instead
    /// of walking both label vectors.
    tree_hash: u64,
    labels: Vec<NodeLabel>,
    /// FNV-1a digest of each label, parallel to `labels`. Lets the edit
    /// distance inner loop and the multiset intersection compare labels
    /// without touching atom text.
    label_hashes: Vec<u64>,
    /// `label_hashes` sorted, for merge-based multiset intersection in
    /// `similarity_upper_bound`.
    sorted_label_hashes: Vec<u64>,
    leaf_count: usize,
    leftmost: Vec<usize>,
    keyroots: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NodeLabel {
    Root(Vec<ReaderPrefix>),
    List(Option<Delimiter>, Vec<ReaderPrefix>),
    Atom(String, Vec<ReaderPrefix>),
}

const EDIT_COST_SCALE: usize = 10;
const ATOM_RENAME_COST: usize = 3;
const MAX_DISTANCE_MATRIX_CELLS: usize = 4 * 1024 * 1024;
const MAX_TREE_SIMILARITY_WORKSPACE_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_TREE_SIMILARITY_WORKSPACE_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_TREE_SIMILARITY_WORKSPACES: usize =
    MAX_TOTAL_TREE_SIMILARITY_WORKSPACE_BYTES / MAX_TREE_SIMILARITY_WORKSPACE_BYTES;
const MAX_TREE_EDIT_OPERATIONS: usize = 64 * 1024 * 1024;
pub const MAX_REPORT_TREE_EDIT_OPERATIONS: usize = MAX_TREE_EDIT_OPERATIONS;

/// Every count this module converts to `f64` is exact, and this is the proof.
///
/// Section 9.4 lists the `usize as f64` casts here as a correctness problem, on
/// the grounds that the conversion is lossy. It is not, for these inputs. An
/// `f64` represents every integer up to 2^53 exactly, and every count reaching
/// a cast below is bounded by one of the limits above - node counts and leaf
/// counts by the 64 MiB input limit (a node costs at least one byte), edit
/// operations by `MAX_TREE_EDIT_OPERATIONS`, matrix cells by
/// `MAX_DISTANCE_MATRIX_CELLS`. All are 2^26 or smaller.
///
/// The alternative - a checked `TryFrom` on each cast - would put a branch in
/// the inner loop of the distance computation to guard a case the limiter has
/// already made unreachable. Proving the bound once is both cheaper and more
/// honest: if someone raises a limit past 2^53, this assertion fails at compile
/// time rather than the ratios quietly losing precision.
const _: () = {
    const F64_EXACT_INTEGER_LIMIT: usize = 1 << 53;
    assert!(MAX_TREE_SIMILARITY_WORKSPACE_BYTES < F64_EXACT_INTEGER_LIMIT);
    assert!(MAX_TOTAL_TREE_SIMILARITY_WORKSPACE_BYTES < F64_EXACT_INTEGER_LIMIT);
    assert!(MAX_TREE_EDIT_OPERATIONS < F64_EXACT_INTEGER_LIMIT);
    assert!(MAX_DISTANCE_MATRIX_CELLS < F64_EXACT_INTEGER_LIMIT);
    assert!(EDIT_COST_SCALE * MAX_TREE_EDIT_OPERATIONS < F64_EXACT_INTEGER_LIMIT);
    assert!(ATOM_RENAME_COST * MAX_TREE_EDIT_OPERATIONS < F64_EXACT_INTEGER_LIMIT);
};

#[derive(Debug)]
struct TreeSimilarityWorkspaceLimiter {
    active: Mutex<usize>,
    available: Condvar,
    limit: usize,
}

impl TreeSimilarityWorkspaceLimiter {
    const fn new(limit: usize) -> Self {
        assert!(limit > 0);
        Self {
            active: Mutex::new(0),
            available: Condvar::new(),
            limit,
        }
    }

    fn acquire(&self, requested: usize) -> TreeSimilarityWorkspaceReservation<'_> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *active >= self.limit {
            active = self
                .available
                .wait(active)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        let count = requested.max(1).min(self.limit.saturating_sub(*active));
        *active = active.saturating_add(count);
        TreeSimilarityWorkspaceReservation {
            limiter: self,
            count,
        }
    }
}

// Public since the extraction: it was crate-internal, a visibility that
// cannot cross a crate boundary, so the lint applies for the first time.
#[derive(Debug)]
pub struct TreeSimilarityWorkspaceReservation<'a> {
    limiter: &'a TreeSimilarityWorkspaceLimiter,
    count: usize,
}

impl TreeSimilarityWorkspaceReservation<'_> {
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }
}

impl Drop for TreeSimilarityWorkspaceReservation<'_> {
    fn drop(&mut self) {
        let mut active = self
            .limiter
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *active = active.saturating_sub(self.count);
        self.limiter.available.notify_all();
    }
}

static TREE_SIMILARITY_WORKSPACE_LIMITER: TreeSimilarityWorkspaceLimiter =
    TreeSimilarityWorkspaceLimiter::new(MAX_TREE_SIMILARITY_WORKSPACES);

pub fn reserve_tree_similarity_workspaces(
    requested: usize,
) -> TreeSimilarityWorkspaceReservation<'static> {
    TREE_SIMILARITY_WORKSPACE_LIMITER.acquire(requested)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeSimilarityError {
    MatrixTooLarge { cells: usize, bytes: usize },
    AllocationFailed { cells: usize, bytes: usize },
    OperationBudgetExceeded { operations: usize, limit: usize },
}

impl fmt::Display for TreeSimilarityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MatrixTooLarge { cells, bytes } => write!(
                formatter,
                "tree similarity matrix exceeds resource budget ({cells} cells, {bytes} bytes)"
            ),
            Self::AllocationFailed { cells, bytes } => write!(
                formatter,
                "tree similarity matrix allocation failed ({cells} cells, {bytes} bytes)"
            ),
            Self::OperationBudgetExceeded { operations, limit } => write!(
                formatter,
                "tree similarity operation budget exceeded ({operations} operations, limit {limit})"
            ),
        }
    }
}

impl Error for TreeSimilarityError {}

#[derive(Debug, Default)]
pub struct TreeSimilarityWorkspace {
    tree_distances: Vec<usize>,
    forest_distances: Vec<usize>,
}

#[derive(Debug)]
pub struct TreeSimilarityOperationBudget {
    operations: AtomicUsize,
    exhausted: AtomicBool,
    limit: usize,
}

impl TreeSimilarityOperationBudget {
    #[must_use]
    pub const fn new(limit: usize) -> Self {
        Self {
            operations: AtomicUsize::new(0),
            exhausted: AtomicBool::new(false),
            limit,
        }
    }

    #[inline]
    fn consume_many(&self, count: usize) -> Result<(), TreeSimilarityError> {
        if count == 0 {
            return Ok(());
        }

        if self.exhausted.load(AtomicOrdering::Acquire) {
            return Err(TreeSimilarityError::OperationBudgetExceeded {
                operations: self.operations.load(AtomicOrdering::Acquire),
                limit: self.limit,
            });
        }

        match self.operations.fetch_update(
            AtomicOrdering::AcqRel,
            AtomicOrdering::Acquire,
            |operations| {
                operations
                    .checked_add(count)
                    .filter(|&next| next <= self.limit)
            },
        ) {
            Ok(_) => Ok(()),
            Err(operations) => {
                let attempted = operations
                    .saturating_add(count)
                    .max(self.limit.saturating_add(1));
                self.operations.fetch_max(attempted, AtomicOrdering::AcqRel);
                self.exhausted.store(true, AtomicOrdering::Release);
                Err(TreeSimilarityError::OperationBudgetExceeded {
                    operations: attempted,
                    limit: self.limit,
                })
            }
        }
    }

    pub fn exhausted(&self) -> bool {
        self.exhausted.load(AtomicOrdering::Acquire)
    }

    pub fn operations(&self) -> usize {
        self.operations.load(AtomicOrdering::Acquire)
    }

    pub const fn limit(&self) -> usize {
        self.limit
    }
}

impl TreeSimilarityWorkspace {
    fn try_reset(&mut self, len: usize, bytes: usize) -> Result<(), TreeSimilarityError> {
        if self.tree_distances.capacity() >= len && self.forest_distances.capacity() >= len {
            self.tree_distances.resize(len, 0);
            self.forest_distances.resize(len, 0);
            self.tree_distances.fill(0);
            self.forest_distances.fill(0);
            return Ok(());
        }

        // Allocate transactionally so a failure for the second matrix does not
        // leave the reusable workspace retaining the first large allocation.
        let mut tree_distances = Vec::new();
        tree_distances
            .try_reserve_exact(len)
            .map_err(|_| TreeSimilarityError::AllocationFailed { cells: len, bytes })?;
        let mut forest_distances = Vec::new();
        forest_distances
            .try_reserve_exact(len)
            .map_err(|_| TreeSimilarityError::AllocationFailed { cells: len, bytes })?;
        tree_distances.resize(len, 0);
        forest_distances.resize(len, 0);
        self.tree_distances = tree_distances;
        self.forest_distances = forest_distances;
        Ok(())
    }
}

impl StructuralTree {
    #[must_use]
    pub fn from_view(view: &ExpressionView) -> Self {
        Self::from_view_with_count(view).0
    }

    pub fn from_view_with_count(view: &ExpressionView) -> (Self, usize) {
        fn label(view: &ExpressionView) -> NodeLabel {
            match view.kind {
                ExpressionKind::Root => NodeLabel::Root(view.reader_prefixes.clone()),
                ExpressionKind::List => {
                    NodeLabel::List(view.delimiter, view.reader_prefixes.clone())
                }
                ExpressionKind::Atom => NodeLabel::Atom(
                    view.text.clone().unwrap_or_default(),
                    view.reader_prefixes.clone(),
                ),
            }
        }

        enum Visit<'a> {
            Enter(&'a ExpressionView),
            Exit {
                view: &'a ExpressionView,
                descendant_start: usize,
            },
        }

        let mut labels = Vec::new();
        let mut leaf_count = 0;
        let mut leftmost = Vec::new();
        let mut pending = vec![Visit::Enter(view)];
        while let Some(frame) = pending.pop() {
            match frame {
                Visit::Enter(view) => {
                    pending.push(Visit::Exit {
                        view,
                        descendant_start: labels.len(),
                    });
                    pending.extend(view.children.iter().rev().map(Visit::Enter));
                }
                Visit::Exit {
                    view,
                    descendant_start,
                } => {
                    if view.children.is_empty() {
                        leaf_count += 1;
                    }
                    let index = labels.len() + 1;
                    let leaf = leftmost.get(descendant_start).copied().unwrap_or(index);
                    labels.push(label(view));
                    leftmost.push(leaf);
                }
            }
        }
        let node_count = labels.len();
        let mut keyroots = vec![0; node_count + 1];
        for (offset, leaf) in leftmost.iter().copied().enumerate() {
            keyroots[leaf] = offset + 1;
        }
        let mut keyroots = keyroots
            .into_iter()
            .skip(1)
            .filter(|&index| index != 0)
            .collect::<Vec<_>>();
        keyroots.sort_unstable();

        let label_hashes: Vec<u64> = labels.iter().map(hash_label).collect();
        let mut sorted_label_hashes = label_hashes.clone();
        sorted_label_hashes.sort_unstable();
        let tree_hash = label_hashes
            .iter()
            .fold(FNV_OFFSET, |hash, &label| fnv_u64(hash, label));

        (
            Self {
                tree_hash,
                labels,
                label_hashes,
                sorted_label_hashes,
                leaf_count,
                leftmost,
                keyroots,
            },
            node_count,
        )
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.labels.len()
    }

    fn exact_same_topology_distance_scaled(&self, other: &Self) -> Option<usize> {
        if self.leftmost != other.leftmost {
            return None;
        }

        // Equal leftmost encodings uniquely determine the ordered topology. Any
        // non-full mapping pays at least one delete plus one insert, so an
        // identity rename at or below that boundary is exact.
        let distance_limit = 2 * EDIT_COST_SCALE;
        let mut distance = 0;
        for node in 0..self.labels.len() {
            let cost = rename_cost_scaled(self, node, other, node);
            if cost > distance_limit - distance {
                return None;
            }
            distance += cost;
        }
        Some(distance)
    }

    /// Order-sensitive digest of the whole tree, atom text included.
    ///
    /// Two trees with the same fingerprint are Type-1 clones up to a hash
    /// collision; two with different fingerprints are certainly not.
    #[must_use]
    pub const fn exact_fingerprint(&self) -> u64 {
        self.tree_hash
    }

    /// Order-sensitive digest of the tree with every atom's text erased.
    ///
    /// This is the Type-2 key: it survives renaming identifiers and literals
    /// but not reshaping the tree, changing a delimiter, or changing a reader
    /// prefix. Computed on demand rather than cached, because only the
    /// sequence-clone detector asks for it and it asks once per candidate.
    #[must_use]
    pub fn generic_fingerprint(&self) -> u64 {
        self.labels
            .iter()
            .map(erased_label_hash)
            .fold(FNV_OFFSET, fnv_u64)
    }
}

/// Where a pair of forms sits on the standard clone taxonomy.
///
/// The names are Roy and Cordy's, and so are the boundaries: Type-1 is a copy,
/// Type-2 is a copy with the names changed, Type-3 is a copy someone then
/// edited. Layout and comments never enter into it here because the structural
/// tree has already dropped both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CloneType {
    /// Identical structure and identical atom text.
    Type1,
    /// Identical structure; every difference is one atom's text.
    Type2,
    /// The structures themselves differ.
    Type3,
}

impl CloneType {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Type1 => "type-1",
            Self::Type2 => "type-2",
            Self::Type3 => "type-3",
        }
    }

    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Type1 => 1,
            Self::Type2 => 2,
            Self::Type3 => 3,
        }
    }
}

impl fmt::Display for CloneType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A clone type plus the evidence that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloneClassification {
    pub clone_type: CloneType,
    /// Atoms whose text differs between the two trees. Zero for Type-1, and
    /// meaningless for Type-3, where the trees do not correspond node for node.
    pub renamed_atoms: usize,
    /// Whether the Type-2 substitution is a bijection — every `x` became the
    /// same `y` and nothing else became that `y`.
    ///
    /// An inconsistent renaming is the interesting case: it is the shape a
    /// copy-paste bug takes when one occurrence of a variable was missed. True
    /// for Type-1 (the empty substitution is a bijection) and false for Type-3,
    /// where there is no substitution to be consistent about.
    pub consistent_renaming: bool,
}

/// Classifies a pair of structural trees on the clone taxonomy.
///
/// Cheap by construction: Type-1 is one integer comparison via the derived
/// `PartialEq`, Type-3 is one `Vec` comparison of the postorder leftmost
/// encodings, and only a same-topology pair pays a walk over the labels. No
/// tree edit distance is computed, so a caller that has already decided a pair
/// is similar can label it for free.
///
/// The head of a form is an atom like any other, so `(alpha x)` and `(beta x)`
/// are Type-2. That follows the taxonomy — a function name is an identifier —
/// but it does mean a Type-2 label is not on its own a claim that the two forms
/// compute the same thing.
#[must_use]
pub fn classify_clone(left: &StructuralTree, right: &StructuralTree) -> CloneClassification {
    if left == right {
        return CloneClassification {
            clone_type: CloneType::Type1,
            renamed_atoms: 0,
            consistent_renaming: true,
        };
    }

    // Equal leftmost encodings uniquely determine the ordered topology, so an
    // inequality here is conclusive: no node-for-node correspondence exists.
    if left.leftmost != right.leftmost {
        return CloneClassification {
            clone_type: CloneType::Type3,
            renamed_atoms: 0,
            consistent_renaming: false,
        };
    }

    let mut renamed_atoms = 0;
    let mut forward = HashMap::<&str, &str>::new();
    let mut backward = HashMap::<&str, &str>::new();
    let mut consistent_renaming = true;
    let type_3 = CloneClassification {
        clone_type: CloneType::Type3,
        renamed_atoms: 0,
        consistent_renaming: false,
    };
    for (left_label, right_label) in left.labels.iter().zip(&right.labels) {
        match (left_label, right_label) {
            (
                NodeLabel::Atom(left_text, left_prefixes),
                NodeLabel::Atom(right_text, right_prefixes),
            ) => {
                // A differing reader prefix is not a rename: `'x` and `x` read
                // differently however the symbol is spelled.
                if left_prefixes != right_prefixes {
                    return type_3;
                }
                if left_text != right_text {
                    renamed_atoms += 1;
                }
                // Every atom pair enters the substitution, including the ones
                // that did not change. An atom left alone is the identity
                // mapping `x -> x`, and it is precisely the conflict between
                // that and a `x -> y` elsewhere that exposes a missed rename.
                consistent_renaming &= forward
                    .insert(left_text, right_text)
                    .is_none_or(|previous| previous == right_text);
                consistent_renaming &= backward
                    .insert(right_text, left_text)
                    .is_none_or(|previous| previous == left_text);
            }
            _ if left_label == right_label => {}
            _ => return type_3,
        }
    }

    CloneClassification {
        clone_type: CloneType::Type2,
        renamed_atoms,
        consistent_renaming,
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[inline]
fn fnv_byte(hash: u64, byte: u8) -> u64 {
    (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
}

#[inline]
fn fnv_u64(hash: u64, value: u64) -> u64 {
    value
        .to_le_bytes()
        .iter()
        .fold(hash, |hash, &byte| fnv_byte(hash, byte))
}

fn hash_label(label: &NodeLabel) -> u64 {
    fn hash_prefixes(mut hash: u64, prefixes: &[ReaderPrefix]) -> u64 {
        for prefix in prefixes {
            hash = fnv_byte(hash, *prefix as u8 + 1);
        }
        hash
    }

    match label {
        NodeLabel::Root(prefixes) => hash_prefixes(fnv_byte(FNV_OFFSET, 0), prefixes),
        NodeLabel::List(delimiter, prefixes) => {
            let delimiter_byte = match delimiter {
                None => 0,
                Some(Delimiter::Paren) => 1,
                Some(Delimiter::Bracket) => 2,
                Some(Delimiter::Brace) => 3,
            };
            hash_prefixes(fnv_byte(fnv_byte(FNV_OFFSET, 1), delimiter_byte), prefixes)
        }
        NodeLabel::Atom(text, prefixes) => {
            let mut hash = fnv_byte(FNV_OFFSET, 2);
            for byte in text.bytes() {
                hash = fnv_byte(hash, byte);
            }
            // Terminator keeps `("ab", [Quote])` and `("ab\x01", [])`-style
            // field boundaries from colliding.
            hash_prefixes(fnv_byte(hash, 0xff), prefixes)
        }
    }
}

/// Hashes a label the way `hash_label` does, minus an atom's text.
///
/// Keeping the reader prefixes and the delimiter means the digest still
/// separates `'x` from `x` and `[a]` from `(a)`; only the identifier itself is
/// erased, which is exactly the Type-2 equivalence.
fn erased_label_hash(label: &NodeLabel) -> u64 {
    match label {
        NodeLabel::Atom(_, prefixes) => {
            let mut hash = fnv_byte(FNV_OFFSET, 2);
            for prefix in prefixes {
                hash = fnv_byte(hash, *prefix as u8 + 1);
            }
            hash
        }
        other => hash_label(other),
    }
}

/// Counts the multiset intersection of two sorted hash sequences by merging.
fn sorted_intersection_count(left: &[u64], right: &[u64]) -> usize {
    let mut shared = 0;
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
            Ordering::Less => left_index += 1,
            Ordering::Greater => right_index += 1,
            Ordering::Equal => {
                shared += 1;
                left_index += 1;
                right_index += 1;
            }
        }
    }
    shared
}

/// Cheap upper bound on `tree_similarity` derived from label multisets and leaf counts.
///
/// The label bound accounts for unmatched nodes and renamed matched nodes. The
/// leaf bound follows because a rename preserves the number of leaves and each
/// insertion or deletion changes it by at most one. Taking the maximum of these
/// independent distance lower bounds keeps the resulting similarity bound sound.
/// Label hash collisions can only loosen the label bound.
#[must_use]
pub fn similarity_upper_bound(left: &StructuralTree, right: &StructuralTree) -> f64 {
    let left_count = left.labels.len();
    let right_count = right.labels.len();
    let matched = left_count.min(right_count);
    let shared = sorted_intersection_count(&left.sorted_label_hashes, &right.sorted_label_hashes);
    let label_lower_bound_scaled = EDIT_COST_SCALE as f64 * left_count.abs_diff(right_count) as f64
        + ATOM_RENAME_COST as f64 * matched.saturating_sub(shared) as f64;
    let leaf_lower_bound_scaled =
        EDIT_COST_SCALE as f64 * left.leaf_count.abs_diff(right.leaf_count) as f64;
    let lower_bound_scaled = label_lower_bound_scaled.max(leaf_lower_bound_scaled);
    1.0 - lower_bound_scaled / (EDIT_COST_SCALE as f64 * left_count.max(right_count) as f64)
}

pub fn tree_similarity(
    left: &StructuralTree,
    right: &StructuralTree,
) -> Result<f64, TreeSimilarityError> {
    let _workspace_reservation = reserve_tree_similarity_workspaces(1);
    let mut workspace = TreeSimilarityWorkspace::default();
    tree_similarity_with_workspace(left, right, &mut workspace)
}

pub fn tree_similarity_with_workspace(
    left: &StructuralTree,
    right: &StructuralTree,
    workspace: &mut TreeSimilarityWorkspace,
) -> Result<f64, TreeSimilarityError> {
    tree_similarity_with_workspace_and_budget(left, right, workspace, None)
}

pub fn tree_similarity_with_workspace_and_budget(
    left: &StructuralTree,
    right: &StructuralTree,
    workspace: &mut TreeSimilarityWorkspace,
    operation_budget: Option<&TreeSimilarityOperationBudget>,
) -> Result<f64, TreeSimilarityError> {
    if left == right {
        return Ok(1.0);
    }
    let denominator = left.node_count().max(right.node_count()) as f64;
    let distance =
        tree_edit_distance_with_workspace_and_budget(left, right, workspace, operation_budget)?;
    Ok((1.0 - distance / denominator).max(0.0))
}

#[cfg(test)]
fn tree_edit_distance(left: &StructuralTree, right: &StructuralTree) -> f64 {
    let mut workspace = TreeSimilarityWorkspace::default();
    tree_edit_distance_with_workspace(left, right, &mut workspace)
        .expect("test tree edit-distance workspace should be allocatable")
}

#[cfg(test)]
fn tree_edit_distance_with_workspace(
    left: &StructuralTree,
    right: &StructuralTree,
    workspace: &mut TreeSimilarityWorkspace,
) -> Result<f64, TreeSimilarityError> {
    tree_edit_distance_with_workspace_and_budget(left, right, workspace, None)
}

fn tree_edit_distance_with_workspace_and_budget(
    left: &StructuralTree,
    right: &StructuralTree,
    workspace: &mut TreeSimilarityWorkspace,
    shared_operation_budget: Option<&TreeSimilarityOperationBudget>,
) -> Result<f64, TreeSimilarityError> {
    tree_edit_distance_scaled_with_workspace(left, right, workspace, shared_operation_budget)
        .map(|distance| distance as f64 / EDIT_COST_SCALE as f64)
}

fn tree_edit_distance_scaled_with_workspace(
    left: &StructuralTree,
    right: &StructuralTree,
    workspace: &mut TreeSimilarityWorkspace,
    shared_operation_budget: Option<&TreeSimilarityOperationBudget>,
) -> Result<usize, TreeSimilarityError> {
    if let Some(operation_budget) = shared_operation_budget {
        if operation_budget.exhausted() {
            return Err(TreeSimilarityError::OperationBudgetExceeded {
                operations: operation_budget.operations(),
                limit: operation_budget.limit(),
            });
        }
    }

    if let Some(distance) = left.exact_same_topology_distance_scaled(right) {
        return Ok(distance);
    }

    let left_len = left.labels.len();
    let right_len = right.labels.len();
    let (width, len, bytes) = distance_matrix_dimensions(left_len, right_len)?;
    workspace.try_reset(len, bytes)?;
    let mut local_operation_budget = TreeEditOperationBudget::new(MAX_TREE_EDIT_OPERATIONS);
    let mut operation_budgets = TreeEditOperationBudgets {
        local: &mut local_operation_budget,
        shared: shared_operation_budget,
    };

    for &left_root in &left.keyroots {
        for &right_root in &right.keyroots {
            forest_distance(
                left,
                right,
                &mut workspace.tree_distances,
                &mut workspace.forest_distances,
                width,
                ForestRoots {
                    left: left_root,
                    right: right_root,
                },
                &mut operation_budgets,
            )?;
        }
    }

    Ok(workspace.tree_distances[index(left_len, right_len, width)])
}

fn distance_matrix_dimensions(
    left_len: usize,
    right_len: usize,
) -> Result<(usize, usize, usize), TreeSimilarityError> {
    let exceeded = || TreeSimilarityError::MatrixTooLarge {
        cells: usize::MAX,
        bytes: usize::MAX,
    };
    let height = left_len.checked_add(1).ok_or_else(exceeded)?;
    let width = right_len.checked_add(1).ok_or_else(exceeded)?;
    let len = height.checked_mul(width).ok_or_else(exceeded)?;
    let bytes = len
        .checked_mul(std::mem::size_of::<usize>())
        .and_then(|one_matrix| one_matrix.checked_mul(2))
        .ok_or_else(exceeded)?;
    if len > MAX_DISTANCE_MATRIX_CELLS || bytes > MAX_TREE_SIMILARITY_WORKSPACE_BYTES {
        return Err(TreeSimilarityError::MatrixTooLarge { cells: len, bytes });
    }
    Ok((width, len, bytes))
}

fn forest_distance(
    left: &StructuralTree,
    right: &StructuralTree,
    tree_distances: &mut [usize],
    forest_distances: &mut [usize],
    width: usize,
    roots: ForestRoots,
    operation_budgets: &mut TreeEditOperationBudgets<'_>,
) -> Result<(), TreeSimilarityError> {
    let left_root = roots.left;
    let right_root = roots.right;
    let left_start = left.leftmost[left_root - 1];
    let right_start = right.leftmost[right_root - 1];
    let row_count = left_root - left_start + 2;
    let column_count = right_root - right_start + 2;

    for row in 1..row_count {
        forest_distances[index(row, 0, width)] =
            forest_distances[index(row - 1, 0, width)] + EDIT_COST_SCALE;
    }
    for column in 1..column_count {
        forest_distances[index(0, column, width)] =
            forest_distances[index(0, column - 1, width)] + EDIT_COST_SCALE;
    }

    for row in 1..row_count {
        operation_budgets.consume_many(column_count - 1)?;
        let left_node = left_start + row - 1;
        for column in 1..column_count {
            let right_node = right_start + column - 1;
            let delete = forest_distances[index(row - 1, column, width)] + EDIT_COST_SCALE;
            let insert = forest_distances[index(row, column - 1, width)] + EDIT_COST_SCALE;

            if left.leftmost[left_node - 1] == left_start
                && right.leftmost[right_node - 1] == right_start
            {
                let rename = forest_distances[index(row - 1, column - 1, width)]
                    + rename_cost_scaled(left, left_node - 1, right, right_node - 1);
                let distance = delete.min(insert).min(rename);
                forest_distances[index(row, column, width)] = distance;
                tree_distances[index(left_node, right_node, width)] = distance;
            } else {
                let left_prefix = left.leftmost[left_node - 1] - left_start;
                let right_prefix = right.leftmost[right_node - 1] - right_start;
                let replace = forest_distances[index(left_prefix, right_prefix, width)]
                    + tree_distances[index(left_node, right_node, width)];
                forest_distances[index(row, column, width)] = delete.min(insert).min(replace);
            }
        }
    }

    Ok(())
}

#[derive(Clone, Copy)]
struct ForestRoots {
    left: usize,
    right: usize,
}

struct TreeEditOperationBudget {
    operations: usize,
    limit: usize,
}

struct TreeEditOperationBudgets<'a> {
    local: &'a mut TreeEditOperationBudget,
    shared: Option<&'a TreeSimilarityOperationBudget>,
}

impl TreeEditOperationBudgets<'_> {
    #[inline]
    fn consume_many(&mut self, count: usize) -> Result<(), TreeSimilarityError> {
        if let Some(shared) = self.shared {
            shared.consume_many(count)?;
        }
        self.local.consume_many(count)
    }
}

impl TreeEditOperationBudget {
    const fn new(limit: usize) -> Self {
        Self {
            operations: 0,
            limit,
        }
    }

    #[inline]
    fn consume_many(&mut self, count: usize) -> Result<(), TreeSimilarityError> {
        let next = self.operations.checked_add(count).ok_or(
            TreeSimilarityError::OperationBudgetExceeded {
                operations: usize::MAX,
                limit: self.limit,
            },
        )?;
        if next > self.limit {
            return Err(TreeSimilarityError::OperationBudgetExceeded {
                operations: next,
                limit: self.limit,
            });
        }
        self.operations = next;
        Ok(())
    }
}

#[inline]
const fn index(row: usize, column: usize, width: usize) -> usize {
    row * width + column
}

fn rename_cost_scaled(
    left: &StructuralTree,
    left_node: usize,
    right: &StructuralTree,
    right_node: usize,
) -> usize {
    // Hash check first: unequal labels (the common case in this hot loop)
    // are rejected without comparing atom text; the full comparison then
    // guards against hash collisions.
    if left.label_hashes[left_node] == right.label_hashes[right_node]
        && left.labels[left_node] == right.labels[right_node]
    {
        0
    } else if matches!(
        (&left.labels[left_node], &right.labels[right_node]),
        (NodeLabel::Atom(_, _), NodeLabel::Atom(_, _))
    ) {
        ATOM_RENAME_COST
    } else {
        EDIT_COST_SCALE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, Path, SyntaxTree};

    fn form(input: &str) -> StructuralTree {
        let tree = SyntaxTree::parse(input).unwrap();
        StructuralTree::from_view(&tree.select_path(&Path::root_child(0)).unwrap().view())
    }

    fn assert_similarity_contract(left: &StructuralTree, right: &StructuralTree) -> f64 {
        let forward = tree_similarity(left, right).unwrap();
        let backward = tree_similarity(right, left).unwrap();

        assert!(forward.is_finite());
        assert!((0.0..=1.0).contains(&forward));
        assert!((forward - backward).abs() < f64::EPSILON);
        forward
    }

    fn synthetic_tree(leftmost: Vec<usize>, labels: Vec<NodeLabel>) -> StructuralTree {
        assert_eq!(leftmost.len(), labels.len());

        let mut keyroots = vec![0; labels.len() + 1];
        for (offset, leaf) in leftmost.iter().copied().enumerate() {
            keyroots[leaf] = offset + 1;
        }
        let mut keyroots = keyroots
            .into_iter()
            .skip(1)
            .filter(|&node| node != 0)
            .collect::<Vec<_>>();
        keyroots.sort_unstable();

        let leaf_count = leftmost
            .iter()
            .enumerate()
            .filter(|&(offset, leaf)| *leaf == offset + 1)
            .count();
        let label_hashes = labels.iter().map(hash_label).collect::<Vec<_>>();
        let mut sorted_label_hashes = label_hashes.clone();
        sorted_label_hashes.sort_unstable();
        let tree_hash = label_hashes
            .iter()
            .fold(FNV_OFFSET, |hash, &label| fnv_u64(hash, label));

        StructuralTree {
            tree_hash,
            labels,
            label_hashes,
            sorted_label_hashes,
            leaf_count,
            leftmost,
            keyroots,
        }
    }

    fn tree_edit_distance_scaled_without_topology_fastpath(
        left: &StructuralTree,
        right: &StructuralTree,
        workspace: &mut TreeSimilarityWorkspace,
    ) -> Result<usize, TreeSimilarityError> {
        let left_len = left.labels.len();
        let right_len = right.labels.len();
        let (width, len, bytes) = distance_matrix_dimensions(left_len, right_len)?;
        workspace.try_reset(len, bytes)?;
        let mut local_operation_budget = TreeEditOperationBudget::new(MAX_TREE_EDIT_OPERATIONS);
        let mut operation_budgets = TreeEditOperationBudgets {
            local: &mut local_operation_budget,
            shared: None,
        };

        for &left_root in &left.keyroots {
            for &right_root in &right.keyroots {
                forest_distance(
                    left,
                    right,
                    &mut workspace.tree_distances,
                    &mut workspace.forest_distances,
                    width,
                    ForestRoots {
                        left: left_root,
                        right: right_root,
                    },
                    &mut operation_budgets,
                )?;
            }
        }

        Ok(workspace.tree_distances[index(left_len, right_len, width)])
    }

    fn renamed_atom_chain(count: usize) -> (StructuralTree, StructuralTree) {
        let left = (0..count)
            .map(|index| NodeLabel::Atom(format!("left-{index}"), Vec::new()))
            .collect();
        let right = (0..count)
            .map(|index| NodeLabel::Atom(format!("right-{index}"), Vec::new()))
            .collect();

        (
            synthetic_tree(vec![1; count], left),
            synthetic_tree(vec![1; count], right),
        )
    }

    fn assert_same_topology_distance_bidirectional(
        left: &StructuralTree,
        right: &StructuralTree,
        expected_fastpath: Option<usize>,
        expected_distance: usize,
    ) {
        for (first, second) in [(left, right), (right, left)] {
            assert_eq!(
                first.exact_same_topology_distance_scaled(second),
                expected_fastpath
            );

            let mut fast_workspace = TreeSimilarityWorkspace::default();
            let fast =
                tree_edit_distance_scaled_with_workspace(first, second, &mut fast_workspace, None);
            let mut full_workspace = TreeSimilarityWorkspace::default();
            let full = tree_edit_distance_scaled_without_topology_fastpath(
                first,
                second,
                &mut full_workspace,
            );

            assert_eq!(full, Ok(expected_distance));
            assert_eq!(fast, full);
        }
    }

    #[test]
    fn same_topology_fastpath_matches_full_dp() {
        let (left, right) = renamed_atom_chain(6);
        let expected_distance = 6 * ATOM_RENAME_COST;

        assert_same_topology_distance_bidirectional(
            &left,
            &right,
            Some(expected_distance),
            expected_distance,
        );
    }

    #[test]
    fn same_topology_fastpath_accepts_delete_insert_boundary() {
        let exact_boundary_left = synthetic_tree(
            vec![1, 1],
            vec![NodeLabel::Root(Vec::new()), NodeLabel::Root(Vec::new())],
        );
        let exact_boundary_right = synthetic_tree(
            vec![1, 1],
            vec![
                NodeLabel::List(None, Vec::new()),
                NodeLabel::List(None, Vec::new()),
            ],
        );
        assert_same_topology_distance_bidirectional(
            &exact_boundary_left,
            &exact_boundary_right,
            Some(2 * EDIT_COST_SCALE),
            2 * EDIT_COST_SCALE,
        );

        let mixed_left = synthetic_tree(
            vec![1; 5],
            vec![
                NodeLabel::Atom("same".to_owned(), Vec::new()),
                NodeLabel::Root(Vec::new()),
                NodeLabel::Atom("left-1".to_owned(), Vec::new()),
                NodeLabel::Atom("left-2".to_owned(), Vec::new()),
                NodeLabel::Atom("left-3".to_owned(), Vec::new()),
            ],
        );
        let mixed_right = synthetic_tree(
            vec![1; 5],
            vec![
                NodeLabel::Atom("same".to_owned(), Vec::new()),
                NodeLabel::List(None, Vec::new()),
                NodeLabel::Atom("right-1".to_owned(), Vec::new()),
                NodeLabel::Atom("right-2".to_owned(), Vec::new()),
                NodeLabel::Atom("right-3".to_owned(), Vec::new()),
            ],
        );
        let mixed_distance = EDIT_COST_SCALE + 3 * ATOM_RENAME_COST;
        assert_same_topology_distance_bidirectional(
            &mixed_left,
            &mixed_right,
            Some(mixed_distance),
            mixed_distance,
        );
    }

    #[test]
    fn same_topology_fastpath_falls_back_above_boundary_and_for_different_topology() {
        let (atom_left, atom_right) = renamed_atom_chain(7);
        assert_same_topology_distance_bidirectional(
            &atom_left,
            &atom_right,
            None,
            7 * ATOM_RENAME_COST,
        );

        let mixed_left = synthetic_tree(
            vec![1; 6],
            vec![
                NodeLabel::Atom("same".to_owned(), Vec::new()),
                NodeLabel::Root(Vec::new()),
                NodeLabel::Atom("left-1".to_owned(), Vec::new()),
                NodeLabel::Atom("left-2".to_owned(), Vec::new()),
                NodeLabel::Atom("left-3".to_owned(), Vec::new()),
                NodeLabel::Atom("left-4".to_owned(), Vec::new()),
            ],
        );
        let mixed_right = synthetic_tree(
            vec![1; 6],
            vec![
                NodeLabel::Atom("same".to_owned(), Vec::new()),
                NodeLabel::List(None, Vec::new()),
                NodeLabel::Atom("right-1".to_owned(), Vec::new()),
                NodeLabel::Atom("right-2".to_owned(), Vec::new()),
                NodeLabel::Atom("right-3".to_owned(), Vec::new()),
                NodeLabel::Atom("right-4".to_owned(), Vec::new()),
            ],
        );
        assert_same_topology_distance_bidirectional(
            &mixed_left,
            &mixed_right,
            None,
            EDIT_COST_SCALE + 4 * ATOM_RENAME_COST,
        );

        let different_topology =
            synthetic_tree(vec![1, 2, 1, 1, 1, 1, 1], atom_right.labels.clone());
        assert_eq!(
            atom_left.exact_same_topology_distance_scaled(&different_topology),
            None
        );
        assert_eq!(
            different_topology.exact_same_topology_distance_scaled(&atom_left),
            None
        );
    }

    #[test]
    fn same_topology_fastpath_does_not_consume_shared_budget() {
        let left = synthetic_tree(
            vec![1, 1, 1],
            vec![
                NodeLabel::Atom("same".into(), Vec::new()),
                NodeLabel::Atom("left".into(), Vec::new()),
                NodeLabel::Root(Vec::new()),
            ],
        );
        let right = synthetic_tree(
            vec![1, 1, 1],
            vec![
                NodeLabel::Atom("same".into(), Vec::new()),
                NodeLabel::Atom("right".into(), Vec::new()),
                NodeLabel::Root(Vec::new()),
            ],
        );
        let budget = TreeSimilarityOperationBudget::new(0);
        let mut workspace = TreeSimilarityWorkspace::default();

        assert!(tree_similarity_with_workspace_and_budget(
            &left,
            &right,
            &mut workspace,
            Some(&budget),
        )
        .is_ok());
        assert_eq!(budget.operations(), 0);
        assert!(!budget.exhausted());
    }

    #[test]
    fn same_topology_fastpath_rejects_preexhausted_shared_budget() {
        let operation_budget = TreeSimilarityOperationBudget::new(0);
        let different_topology_left = synthetic_tree(
            vec![1, 1, 1],
            vec![
                NodeLabel::Root(Vec::new()),
                NodeLabel::Root(Vec::new()),
                NodeLabel::Root(Vec::new()),
            ],
        );
        let different_topology_right = synthetic_tree(
            vec![1, 2, 1],
            vec![
                NodeLabel::Root(Vec::new()),
                NodeLabel::Root(Vec::new()),
                NodeLabel::Root(Vec::new()),
            ],
        );
        let mut workspace = TreeSimilarityWorkspace::default();
        let initial_error = tree_edit_distance_scaled_with_workspace(
            &different_topology_left,
            &different_topology_right,
            &mut workspace,
            Some(&operation_budget),
        )
        .unwrap_err();
        let exhausted_operations = operation_budget.operations();

        assert!(operation_budget.exhausted());
        assert!(exhausted_operations > operation_budget.limit());
        assert_eq!(
            initial_error,
            TreeSimilarityError::OperationBudgetExceeded {
                operations: exhausted_operations,
                limit: operation_budget.limit(),
            }
        );

        let (same_topology_left, same_topology_right) = renamed_atom_chain(1);
        let mut reused_workspace = TreeSimilarityWorkspace::default();
        assert_eq!(
            tree_edit_distance_scaled_with_workspace(
                &same_topology_left,
                &same_topology_right,
                &mut reused_workspace,
                Some(&operation_budget),
            ),
            Err(initial_error)
        );
        assert_eq!(operation_budget.operations(), exhausted_operations);
    }

    #[test]
    fn workspace_limiter_blocks_until_a_reservation_is_released() {
        let limiter = TreeSimilarityWorkspaceLimiter::new(2);
        let reservation = limiter.acquire(2);
        assert_eq!(reservation.count(), 2);

        std::thread::scope(|scope| {
            let (attempting_tx, attempting_rx) = std::sync::mpsc::channel();
            let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
            let limiter = &limiter;
            scope.spawn(move || {
                attempting_tx.send(()).unwrap();
                let reservation = limiter.acquire(1);
                acquired_tx.send(reservation.count()).unwrap();
            });

            attempting_rx.recv().unwrap();
            assert_eq!(
                acquired_rx.recv_timeout(std::time::Duration::from_millis(50)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            );
            drop(reservation);
            assert_eq!(
                acquired_rx
                    .recv_timeout(std::time::Duration::from_secs(1))
                    .unwrap(),
                1
            );
        });
    }

    #[test]
    fn tree_edit_operation_counter_fails_closed_at_budget() {
        let mut budget = TreeEditOperationBudget {
            operations: MAX_TREE_EDIT_OPERATIONS - 2,
            limit: MAX_TREE_EDIT_OPERATIONS,
        };

        assert_eq!(budget.consume_many(2), Ok(()));
        assert_eq!(budget.operations, MAX_TREE_EDIT_OPERATIONS);
        assert_eq!(
            budget.consume_many(1),
            Err(TreeSimilarityError::OperationBudgetExceeded {
                operations: MAX_TREE_EDIT_OPERATIONS + 1,
                limit: MAX_TREE_EDIT_OPERATIONS,
            })
        );
        assert_eq!(budget.operations, MAX_TREE_EDIT_OPERATIONS);

        let mut overflow_budget = TreeEditOperationBudget {
            operations: usize::MAX - 1,
            limit: usize::MAX,
        };
        assert_eq!(
            overflow_budget.consume_many(2),
            Err(TreeSimilarityError::OperationBudgetExceeded {
                operations: usize::MAX,
                limit: usize::MAX,
            })
        );
        assert_eq!(overflow_budget.operations, usize::MAX - 1);
    }

    #[test]
    fn shared_operation_budget_is_exact_for_equal_label_multisets() {
        let left = form("(foo (bar a) b)");
        let right = form("(foo bar (a b))");
        assert_ne!(left, right);
        assert_ne!(left.leftmost, right.leftmost);
        assert_eq!(left.sorted_label_hashes, right.sorted_label_hashes);
        assert_eq!(similarity_upper_bound(&left, &right), 1.0);

        let mut workspace = TreeSimilarityWorkspace::default();
        let measuring_budget = TreeSimilarityOperationBudget::new(usize::MAX);
        assert!(
            tree_similarity_with_workspace_and_budget(
                &left,
                &right,
                &mut workspace,
                Some(&measuring_budget),
            )
            .is_ok()
        );
        let required_operations = measuring_budget.operations();
        assert!(required_operations > 1);

        let exact_budget = TreeSimilarityOperationBudget::new(required_operations);
        assert!(
            tree_similarity_with_workspace_and_budget(
                &left,
                &right,
                &mut workspace,
                Some(&exact_budget),
            )
            .is_ok()
        );
        assert_eq!(exact_budget.operations(), required_operations);
        assert!(!exact_budget.exhausted());

        let insufficient_budget = TreeSimilarityOperationBudget::new(required_operations - 1);
        assert!(matches!(
            tree_similarity_with_workspace_and_budget(
                &left,
                &right,
                &mut workspace,
                Some(&insufficient_budget),
            ),
            Err(TreeSimilarityError::OperationBudgetExceeded { limit, .. })
                if limit == required_operations - 1
        ));
        assert!(insufficient_budget.operations() > required_operations - 1);
        assert!(insufficient_budget.exhausted());

        let fail_closed_budget = TreeSimilarityOperationBudget::new(3);
        assert_eq!(fail_closed_budget.consume_many(2), Ok(()));
        assert_eq!(
            fail_closed_budget.consume_many(2),
            Err(TreeSimilarityError::OperationBudgetExceeded {
                operations: 4,
                limit: 3,
            })
        );
        assert_eq!(fail_closed_budget.operations(), 4);
        assert!(fail_closed_budget.exhausted());
        assert_eq!(
            fail_closed_budget.consume_many(1),
            Err(TreeSimilarityError::OperationBudgetExceeded {
                operations: 4,
                limit: 3,
            })
        );
        assert_eq!(fail_closed_budget.operations(), 4);

        let overflow_budget = TreeSimilarityOperationBudget::new(usize::MAX);
        assert_eq!(overflow_budget.consume_many(usize::MAX - 1), Ok(()));
        assert_eq!(
            overflow_budget.consume_many(2),
            Err(TreeSimilarityError::OperationBudgetExceeded {
                operations: usize::MAX,
                limit: usize::MAX,
            })
        );
        assert_eq!(overflow_budget.operations(), usize::MAX);
        assert!(overflow_budget.exhausted());

        assert!(tree_similarity(&left, &right).is_ok());
    }

    #[test]
    fn leaf_count_tightens_similarity_upper_bound_soundly() {
        let left = form("((a) (b))");
        let right = form("(() (a b))");
        assert_eq!(left.node_count(), right.node_count());
        assert_eq!(left.sorted_label_hashes, right.sorted_label_hashes);
        assert_eq!((left.leaf_count, right.leaf_count), (2, 3));

        let upper = similarity_upper_bound(&left, &right);
        let reverse_upper = similarity_upper_bound(&right, &left);
        let similarity = tree_similarity(&left, &right).unwrap();
        let reverse_similarity = tree_similarity(&right, &left).unwrap();

        assert!(upper < 1.0);
        assert!((upper - 0.8).abs() < f64::EPSILON);
        assert!(similarity <= upper + f64::EPSILON);
        assert_eq!(upper, reverse_upper);
        assert_eq!(similarity, reverse_similarity);
    }

    #[test]
    fn leaf_count_upper_bound_is_sound_for_small_tree_set() {
        let trees = ["a", "(a)", "((a))", "(a b)", "((a) (b))", "(() (a b))"].map(form);

        for left in &trees {
            for right in &trees {
                let upper = similarity_upper_bound(left, right);
                let similarity = tree_similarity(left, right).unwrap();
                assert!(similarity <= upper + f64::EPSILON);
                assert_eq!(upper, similarity_upper_bound(right, left));
            }
        }
    }

    #[test]
    fn alpha_rename_is_highly_similar() {
        assert!(
            tree_similarity(
                &form("(let ((x 1)) (+ x 2))"),
                &form("(let ((y 1)) (+ y 2))")
            )
            .unwrap()
                > 0.9
        );
    }

    #[test]
    fn structural_difference_lowers_similarity() {
        let renamed = tree_similarity(&form("(foo a b)"), &form("(foo x y)")).unwrap();
        let changed = tree_similarity(&form("(foo a b)"), &form("(foo (bar a) b c)")).unwrap();
        assert!(changed < renamed);
    }

    #[test]
    fn adding_or_removing_one_wrapper_costs_one_edit() {
        let plain = form("(foo a)");
        let wrapped = form("((foo a))");

        assert!((tree_edit_distance(&plain, &wrapped) - 1.0).abs() < f64::EPSILON);
        assert!((tree_edit_distance(&wrapped, &plain) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_is_symmetric_and_bounded() {
        let left = form("'(foo a)");
        let right = form("(bar (a))");
        assert_similarity_contract(&left, &right);
    }

    #[test]
    fn identical_trees_have_maximum_similarity() {
        for input in ["atom", "()", "'(foo [bar] {baz})"] {
            let tree = form(input);
            assert_eq!(assert_similarity_contract(&tree, &tree), 1.0);
        }
    }

    #[test]
    fn reader_prefixes_are_structurally_significant() {
        let variants = [
            form("'value"),
            form("`value"),
            form(",value"),
            form(",@value"),
            form("#'value"),
        ];

        for left in 0..variants.len() {
            for right in (left + 1)..variants.len() {
                assert!(assert_similarity_contract(&variants[left], &variants[right]) < 1.0);
            }
        }
    }

    #[test]
    fn delimiters_are_structurally_significant() {
        let round = form("(value)");
        let square = form("[value]");
        let curly = form("{value}");

        assert!(assert_similarity_contract(&round, &square) < 1.0);
        assert!(assert_similarity_contract(&round, &curly) < 1.0);
        assert!(assert_similarity_contract(&square, &curly) < 1.0);
    }

    #[test]
    fn atom_list_and_empty_list_shapes_are_distinct() {
        let atom = form("value");
        let empty = form("()");
        let populated = form("(value)");

        assert!(assert_similarity_contract(&atom, &empty) < 1.0);
        assert!(assert_similarity_contract(&atom, &populated) < 1.0);
        assert!(assert_similarity_contract(&empty, &populated) < 1.0);
    }

    #[test]
    fn deep_and_wide_trees_preserve_similarity_contracts() {
        let deep_left = form(&format!("{}value{}", "(".repeat(64), ")".repeat(64)));
        let deep_right = form(&format!("{}other{}", "(".repeat(64), ")".repeat(64)));
        let wide_left = form(&format!("({})", vec!["value"; 128].join(" ")));
        let wide_right = form(&format!("({})", vec!["other"; 128].join(" ")));

        let deep_similarity = assert_similarity_contract(&deep_left, &deep_right);
        let wide_similarity = assert_similarity_contract(&wide_left, &wide_right);
        assert!(deep_similarity < 1.0);
        assert!(wide_similarity < 1.0);
    }

    #[test]
    fn structural_tree_conversion_preserves_postorder_metadata() {
        let tree = form("(root (left leaf) right)");

        assert_eq!(tree.labels.len(), 6);
        assert_eq!(tree.leftmost, vec![1, 2, 3, 2, 5, 1]);
        assert_eq!(tree.keyroots, vec![3, 4, 5, 6]);
    }

    #[test]
    fn structural_tree_conversion_handles_deep_views_iteratively() {
        let span = ByteSpan::new(ByteOffset::new(0), ByteOffset::new(0));
        let mut view = ExpressionView {
            kind: ExpressionKind::Atom,
            delimiter: None,
            reader_prefixes: Vec::new(),
            span,
            content_span: span,
            text: Some("leaf".to_string()),
            children: Vec::new(),
            symbol_offset: 0,
        };
        for _ in 0..10_000 {
            view = ExpressionView {
                kind: ExpressionKind::List,
                delimiter: Some(Delimiter::Paren),
                reader_prefixes: Vec::new(),
                span,
                content_span: span,
                text: None,
                children: vec![view],
                symbol_offset: 0,
            };
        }

        let structural = StructuralTree::from_view(&view);
        assert_eq!(structural.node_count(), 10_001);
        assert!(structural.leftmost.iter().all(|&leaf| leaf == 1));

        // This test targets conversion. Dropping a deeply owned ExpressionView
        // is independently recursive in Vec's destructor.
        std::mem::forget(view);
    }

    #[test]
    fn distance_matrix_dimensions_reject_overflow_without_allocating() {
        assert_eq!(
            distance_matrix_dimensions(2, 3),
            Ok((4, 12, 12 * std::mem::size_of::<usize>() * 2))
        );
        assert!(distance_matrix_dimensions(usize::MAX, 0).is_err());
        assert!(distance_matrix_dimensions(0, usize::MAX).is_err());
        assert!(distance_matrix_dimensions(usize::MAX / 2, 2).is_err());
    }

    #[test]
    fn distance_matrix_dimensions_enforce_cell_and_byte_budgets() {
        let error = distance_matrix_dimensions(MAX_DISTANCE_MATRIX_CELLS, 1).unwrap_err();
        assert!(matches!(error, TreeSimilarityError::MatrixTooLarge { .. }));
    }

    #[test]
    fn failed_workspace_growth_preserves_existing_buffers() {
        let mut workspace = TreeSimilarityWorkspace::default();
        workspace.try_reset(4, 64).unwrap();
        let tree_capacity = workspace.tree_distances.capacity();
        let forest_capacity = workspace.forest_distances.capacity();

        assert!(workspace.try_reset(usize::MAX, usize::MAX).is_err());
        assert_eq!(workspace.tree_distances.capacity(), tree_capacity);
        assert_eq!(workspace.forest_distances.capacity(), forest_capacity);
    }

    #[test]
    fn identical_forms_classify_as_type_1() {
        let classification = classify_clone(&form("(+ a b)"), &form("(+ a b)"));

        assert_eq!(classification.clone_type, CloneType::Type1);
        assert_eq!(classification.renamed_atoms, 0);
        assert!(classification.consistent_renaming);
    }

    #[test]
    fn renamed_identifiers_classify_as_type_2() {
        let classification = classify_clone(
            &form("(let ((x 1)) (+ x 2))"),
            &form("(let ((y 1)) (+ y 2))"),
        );

        assert_eq!(classification.clone_type, CloneType::Type2);
        assert_eq!(classification.renamed_atoms, 2);
        assert!(classification.consistent_renaming);
    }

    #[test]
    fn a_missed_occurrence_makes_the_type_2_renaming_inconsistent() {
        // The copy-paste bug: `x` became `y` in one place and stayed `x` in the
        // other. Same shape, same atom count, and the substitution is not a
        // function.
        let classification = classify_clone(&form("(+ x x)"), &form("(+ y x)"));

        assert_eq!(classification.clone_type, CloneType::Type2);
        assert_eq!(classification.renamed_atoms, 1);
        assert!(!classification.consistent_renaming);

        // Collapsing two distinct names onto one is equally inconsistent, and
        // only the backward map catches it.
        let collapsed = classify_clone(&form("(+ x y)"), &form("(+ z z)"));
        assert_eq!(collapsed.clone_type, CloneType::Type2);
        assert!(!collapsed.consistent_renaming);
    }

    #[test]
    fn structural_edits_classify_as_type_3() {
        for (left, right) in [
            ("(+ a b)", "(+ a b c)"),
            ("(+ a b)", "(+ (f a) b)"),
            // Same node count, regrouped: a rename cannot move a node.
            ("((a) b)", "(a (b))"),
        ] {
            assert_eq!(
                classify_clone(&form(left), &form(right)).clone_type,
                CloneType::Type3,
                "{left} vs {right}"
            );
        }
    }

    #[test]
    fn a_rename_that_changes_delimiter_or_prefix_is_not_type_2() {
        // Same topology, but the differing labels are not both atoms.
        assert_eq!(
            classify_clone(&form("(f (a))"), &form("(f [a])")).clone_type,
            CloneType::Type3
        );
        // Same topology and both labels are atoms, but one is quoted.
        assert_eq!(
            classify_clone(&form("(f a)"), &form("(f 'b)")).clone_type,
            CloneType::Type3
        );
    }

    #[test]
    fn classification_is_symmetric() {
        let inputs = [
            "(+ a b)",
            "(+ x y)",
            "(+ a b c)",
            "(let ((x 1)) x)",
            "[a b]",
            "'(a b)",
        ]
        .map(form);

        for left in &inputs {
            for right in &inputs {
                let forward = classify_clone(left, right);
                let backward = classify_clone(right, left);
                assert_eq!(forward.clone_type, backward.clone_type);
                assert_eq!(forward.renamed_atoms, backward.renamed_atoms);
                assert_eq!(forward.consistent_renaming, backward.consistent_renaming);
            }
        }
    }

    #[test]
    fn type_1_implies_equal_exact_fingerprints_and_type_2_implies_equal_generic_ones() {
        let base = form("(defun alpha (x) (+ x 1))");
        let copy = form("(defun alpha (x) (+ x 1))");
        let renamed = form("(defun beta (y) (+ y 2))");
        let reshaped = form("(defun alpha (x) (+ x 1 2))");

        assert_eq!(base.exact_fingerprint(), copy.exact_fingerprint());
        assert_ne!(base.exact_fingerprint(), renamed.exact_fingerprint());

        assert_eq!(base.generic_fingerprint(), renamed.generic_fingerprint());
        assert_ne!(base.generic_fingerprint(), reshaped.generic_fingerprint());

        // The generic digest keeps everything a rename cannot touch.
        assert_ne!(
            form("(f a)").generic_fingerprint(),
            form("(f 'a)").generic_fingerprint()
        );
        assert_ne!(
            form("(f (a))").generic_fingerprint(),
            form("(f [a])").generic_fingerprint()
        );
    }

    #[test]
    fn clone_type_labels_and_numbers_agree() {
        for (clone_type, label, number) in [
            (CloneType::Type1, "type-1", 1),
            (CloneType::Type2, "type-2", 2),
            (CloneType::Type3, "type-3", 3),
        ] {
            assert_eq!(clone_type.label(), label);
            assert_eq!(clone_type.number(), number);
            assert_eq!(clone_type.to_string(), label);
        }
    }

    #[test]
    fn workspace_reservation_failure_is_reported_without_panicking() {
        let mut workspace = TreeSimilarityWorkspace::default();
        assert!(workspace.try_reset(usize::MAX, usize::MAX).is_err());
        assert!(workspace.tree_distances.is_empty());
        assert!(workspace.forest_distances.is_empty());
    }
}
