//! `elisp-process-filter-assumes-whole-output`: a process filter that parses
//! its `string` argument as if it were a complete message.
//!
//! A filter is called with whatever bytes Emacs happened to read, not with a
//! message. Measured in GNU Emacs 31.0.91, a subprocess that writes 10000
//! lines in one `write` reaches its filter as:
//!
//! ```text
//! invocations=8  chunk sizes (65536 65536 65536 65536 65536 65536 65536 50142)
//! chunks that split a line mid-record: 7
//! naive per-chunk (split-string string "\n") malformed lines: 7
//! ```
//!
//! `read-process-output-max` is 65536, and that number is exactly why this bug
//! ships. The same probe against a 20009-byte JSON message delivered it in a
//! **single** invocation that parsed cleanly, so a filter written this way
//! passes every test whose output is small and corrupts data the first time a
//! response crosses 64 KiB.
//!
//! What distinguishes a broken filter from a correct one is not the parsing —
//! it is whether the filter *accumulates* first. The Emacs Lisp manual's own
//! prescription (Info node `(elisp) Filter Functions`) is to append the chunk
//! to a buffer or a process property and then extract whole records from the
//! accumulator. So a filter that inserts `string` into a buffer is fine — the
//! buffer *is* the accumulator — and a filter that hands `string` straight to
//! `json-parse-string` or `split-string` is not.
//!
//! This rule reports only that second shape: `string` flowing directly into a
//! whole-message consumer with no accumulation anywhere in the filter body.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ExpressionView, SyntaxTree};

use crate::support::{
    ancestry_at, atom_text, keyword_value, lambda_form, list_head, prefixed_atom_text, subtree_any,
};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-process-filter-assumes-whole-output",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a process filter parsing its string argument as a complete message",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A process filter receives whatever bytes Emacs read, not a message. Output is delivered \
         in chunks of at most `read-process-output-max` (65536 by default), so a record longer \
         than one chunk — or merely one that straddles a chunk boundary — reaches the filter \
         split in two. Handing that argument straight to a parser or a record splitter silently \
         corrupts every record on a boundary. The manual's prescription is to append the chunk to \
         a buffer or a process property first, then extract complete records from the \
         accumulator.",
    )
    .with_example(
        "(set-process-filter p (lambda (_proc string) (handle (json-parse-string string))))",
        "(set-process-filter p (lambda (proc string)\n  (process-put proc :buf (concat (process-get proc :buf) string))\n  (handle-complete-records proc)))",
    )
    .with_caveat(
        "A filter that inserts its argument into a buffer, appends it to a process property, or \
         otherwise accumulates it is not reported: the accumulator is what makes chunking safe, \
         and this rule looks for its absence rather than for the parse.",
    ),
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("set-process-filter"),
    NormalizedHead::new("make-process"),
    NormalizedHead::new("make-network-process"),
];

/// Forms that consume a whole message and therefore cannot take a chunk.
///
/// Each one either parses the argument as a complete syntactic object or
/// splits it into records. `read-from-string` is included with `read` because
/// its second return value — the offset it stopped at — is only useful to code
/// that already holds the whole string.
const WHOLE_MESSAGE_CONSUMERS: [&str; 7] = [
    "json-read-from-string",
    "json-parse-string",
    "read",
    "read-from-string",
    "split-string",
    "string-split",
    "string-to-number",
];

/// Forms whose presence anywhere in the filter body means the filter keeps
/// state between invocations.
///
/// `insert` covers the buffer-as-accumulator idiom the manual recommends;
/// `concat` and `setq` cover the string-accumulator idiom; `process-put`
/// covers storing the tail on the process itself.
///
/// An earlier version asked for the *chunk symbol itself* to be a direct
/// argument of one of these, and the corpus refuted it. `affe.el:77` is a
/// correctly written filter:
///
/// ```text
/// (lambda (_ out)
///   (let ((lines (split-string out "\n")))
///     (if (not (cdr lines))
///         (setq rest (concat rest (car lines)))
///       (setcar lines (concat rest (car lines)))
///       …)))
/// ```
///
/// It stitches the partial tail through `rest`, but what it accumulates is a
/// value *derived* from the chunk, never the chunk itself — so the narrower
/// test reported textbook-correct code. Asking only whether the filter keeps
/// any state at all is the honest question: a filter with nowhere to put a
/// partial record cannot be handling one.
const ACCUMULATORS: [&str; 14] = [
    "insert",
    "insert-before-markers",
    "process-put",
    "concat",
    "push",
    "setq",
    "setq-local",
    "setq-default",
    "setcar",
    "setcdr",
    "setf",
    "puthash",
    "cl-callf",
    "cl-pushnew",
];

/// The filter function this call installs, if it installs one literally.
///
/// A filter named by a symbol (`#'my-filter`) is not reachable from here: this
/// crate cannot follow the name to its definition, and guessing would report
/// code it has not read.
fn installed_filter(view: &ExpressionView) -> Option<&ExpressionView> {
    match list_head(view)? {
        "set-process-filter" => view.children.get(2),
        "make-process" | "make-network-process" => keyword_value(view, ":filter"),
        _ => None,
    }
}

/// The name of a filter lambda's second parameter — the chunk.
///
/// A filter is called with `(process string)`.
///
/// An `_`-prefixed name was excluded here at first, on the theory that a
/// parameter the author spelled `_string` is one they declared unused.
/// Mutation-testing killed nothing, and chasing why showed the guard was
/// simply wrong: a body that reads `_string` and parses it has the defect this
/// rule is about, and the underscore is then a lie rather than a waiver. A
/// filter that genuinely ignores its chunk consumes nothing and is excluded by
/// the consumer test anyway, so the guard bought nothing and suppressed a real
/// shape.
fn chunk_parameter(lambda: &ExpressionView) -> Option<&str> {
    let arglist = lambda.children.get(1)?;
    arglist.children.get(1).and_then(atom_text)
}

/// Whether `symbol` appears as a direct argument of a call to one of `heads`.
///
/// Direct is the whole point: `(json-parse-string (concat tail string))` reads
/// the chunk through an accumulator and is correct, so only an argument
/// position of the consumer itself counts.
fn passed_directly_to(body: &ExpressionView, heads: &[&str], symbol: &str) -> bool {
    let mut matched = |node: &ExpressionView| {
        list_head(node).is_some_and(|head| heads.contains(&head))
            && node.children.get(1..).is_some_and(|arguments| {
                arguments
                    .iter()
                    .any(|argument| prefixed_atom_text(argument) == Some(symbol))
            })
    };
    subtree_any(body, &mut matched)
}

/// Whether the filter body keeps state anywhere.
///
/// Deliberately not tied to the chunk symbol — see [`ACCUMULATORS`] for the
/// corpus finding that settled that.
fn accumulates(body: &ExpressionView) -> bool {
    let mut matched =
        |node: &ExpressionView| list_head(node).is_some_and(|head| ACCUMULATORS.contains(&head));
    subtree_any(body, &mut matched)
}

/// The chunk-consuming call this filter body opens itself up to, if any.
fn unbuffered_consumer<'a>(lambda: &'a ExpressionView, symbol: &str) -> Option<&'a ExpressionView> {
    let body = lambda.children.get(2..)?;
    let consumes = body
        .iter()
        .any(|form| passed_directly_to(form, &WHOLE_MESSAGE_CONSUMERS, symbol));
    if !consumes {
        return None;
    }
    if body.iter().any(accumulates) {
        return None;
    }
    body.iter()
        .find(|form| passed_directly_to(form, &WHOLE_MESSAGE_CONSUMERS, symbol))
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl Rule {
    fn report_for(
        &self,
        tree: &SyntaxTree,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(filter) = installed_filter(view) else {
            return Ok(());
        };
        let Some(lambda) = lambda_form(filter) else {
            return Ok(());
        };
        let Some(symbol) = chunk_parameter(lambda) else {
            return Ok(());
        };
        let Some(consumer) = unbuffered_consumer(lambda, symbol) else {
            return Ok(());
        };
        // Only now is the finding otherwise ready, so only now is reaching the
        // root worth its cost.
        let root = tree.root_view();
        if ancestry_at(&root, view.span).is_data {
            return Ok(());
        }
        sink.report(
            consumer.span,
            format!(
                "process output arrives in chunks of at most read-process-output-max \
                 bytes, so `{symbol}` is not a whole message; accumulate it in a buffer \
                 or a process property before parsing"
            ),
        );
        Ok(())
    }
}

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::EMACS_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // The cheap domain check, before anything reaches the root.
        //
        // This kills no mutation, because `HeadFilter::Heads` has already
        // filtered on exactly these three names. It stays for the reason the
        // sibling `lint-elisp-idiom` rules keep theirs: the head index is
        // documented as a pre-filter that may be *wider* than a rule's own
        // notion of its operator, so a rule that leaned on it would be correct
        // only by accident of the dialect it happens to run on.
        if !matches!(
            list_head(view),
            Some("set-process-filter" | "make-process" | "make-network-process")
        ) {
            return Ok(());
        }
        self.report_for(context.tree(), view, sink)
    }
}
