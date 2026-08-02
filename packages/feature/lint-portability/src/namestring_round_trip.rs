//! `namestring-round-trip-assumption`: a pathname flattened to a string and
//! handed straight back to the filesystem.
//!
//! `(open (namestring path))` takes a structured pathname, renders it — CLHS:
//! "in an *implementation-dependent* canonical form" — and asks the
//! implementation to parse that rendering back into a pathname. Nothing in the
//! standard says the pathname that comes out is the pathname that went in.
//! Components the host's syntax cannot spell, `:wild` and `:unspecific`
//! markers, versions, and the type/name boundary are all free to be lost or
//! re-read differently, and each implementation loses a different set.
//!
//! The round trip is also unnecessary. Every one of these operators takes a
//! *pathname designator*, and a pathname is one. Deleting the conversion is
//! both more portable and less code.
//!
//! The narrower converters are worse, not better: `file-namestring` returns
//! "just the name, type, and version components" and `directory-namestring`
//! "the directory name portion", so feeding either to `open` does not merely
//! risk losing a component — it drops the rest of the path outright, and then
//! the result is resolved against `*default-pathname-defaults*` instead.
//!
//! # Boundary with `unportable-pathname`
//!
//! [`crate::unportable_pathname`] anchors on the same filesystem operators, and
//! the two triggers are disjoint by construction: that rule fires only when the
//! designator is a **string literal** spelling one host's path syntax, and this
//! one only when the designator is a **call** to a namestring converter. A
//! literal is never a call, so no form can earn both findings. The advice
//! converges — "hand the operator a pathname" — because the two rules are the
//! literal-typed and the object-flattened halves of the same mistake.
//!
//! # Limits, deliberately
//!
//! - **Only a directly nested conversion.** Binding the namestring to a
//!   variable and opening it later is the same round trip and is not reported;
//!   finding it would mean correlating a conversion with a later call across a
//!   whole file, which is a per-invocation whole-file scan and is what this
//!   package will not pay.
//! - **`(format nil "~a" path)` is not reported**, even though it is the other
//!   common way to spell the flattening. `~a` of an object that is *already* a
//!   string is a no-op, and nothing here can tell a pathname argument from a
//!   string one — reporting it would fire on correct code.
//!
//! Report-only: whether the fix is to drop the conversion, to keep a pathname
//! in the variable that fed it, or to `parse-namestring` deliberately against
//! known defaults, depends on where the value came from.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, unqualified};

use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "namestring-round-trip-assumption",
    RuleCategory::Portability,
    Severity::Warning,
    "a pathname rendered to a string and handed straight to a filesystem call, which assumes the \
     rendering parses back unchanged",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`namestring` returns the pathname in an implementation-dependent canonical form, and the \
         standard nowhere guarantees that parsing that form yields the same pathname. Since every \
         filesystem operator already accepts a pathname designator, the conversion buys nothing \
         and risks the components the host's syntax cannot spell.",
    )
    .with_example(
        "(with-open-file (s (namestring path)) …)",
        "(with-open-file (s path) …)",
    )
    .with_caveat(
        "Only a conversion written directly in the designator position is reported. A namestring \
         bound to a variable and opened later is the same round trip and is deliberately not \
         reported, and `(format nil \"~a\" path)` is not either — it cannot be told from \
         stringifying a value that is already a string.",
    ),
);

/// Filesystem operators, paired with the argument holding the file designator.
///
/// Kept in step with [`crate::unportable_pathname`]'s table on purpose: the two
/// rules are about the same argument of the same operators, and disagreeing
/// about which operators those are would make one of them silently narrower.
const FILE_OPERATORS: [(&str, usize); 11] = [
    ("open", 1),
    ("load", 1),
    ("probe-file", 1),
    ("truename", 1),
    ("delete-file", 1),
    ("directory", 1),
    ("compile-file", 1),
    ("ensure-directories-exist", 1),
    ("rename-file", 1),
    ("file-write-date", 1),
    ("with-open-file", 1),
];

const HEADS: [NormalizedHead; 11] = [
    NormalizedHead::new("open"),
    NormalizedHead::new("load"),
    NormalizedHead::new("probe-file"),
    NormalizedHead::new("truename"),
    NormalizedHead::new("delete-file"),
    NormalizedHead::new("directory"),
    NormalizedHead::new("compile-file"),
    NormalizedHead::new("ensure-directories-exist"),
    NormalizedHead::new("rename-file"),
    NormalizedHead::new("file-write-date"),
    NormalizedHead::new("with-open-file"),
];

/// How a pathname was flattened into a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Converter {
    /// `namestring`: the whole pathname, in an implementation-dependent
    /// canonical form.
    Full,
    /// `file-namestring`, `directory-namestring`, `host-namestring`: a
    /// *fragment* of the pathname, so the rest is dropped outright.
    Partial,
    /// `enough-namestring`: correct only when re-merged against the same
    /// defaults it was computed from, which a bare filesystem call does not do.
    Relative,
}

impl Converter {
    /// The converter a symbol names, if it names one.
    ///
    /// Strips the package qualifier once and then compares, rather than calling
    /// `symbol_is` per candidate: `symbol_is` re-splits the qualifier on every
    /// call, and five of those is five `rsplit_once` scans of the same string.
    #[must_use]
    pub fn of(symbol: &str) -> Option<Self> {
        let name = unqualified(symbol);
        if name.eq_ignore_ascii_case("namestring") {
            return Some(Self::Full);
        }
        if name.eq_ignore_ascii_case("file-namestring")
            || name.eq_ignore_ascii_case("directory-namestring")
            || name.eq_ignore_ascii_case("host-namestring")
        {
            return Some(Self::Partial);
        }
        name.eq_ignore_ascii_case("enough-namestring")
            .then_some(Self::Relative)
    }

    /// What is wrong with this particular flattening, for the message.
    #[must_use]
    pub const fn complaint(self) -> &'static str {
        match self {
            Self::Full => {
                "renders the pathname in an implementation-dependent canonical form that is not \
                 guaranteed to parse back to the same pathname"
            }
            Self::Partial => {
                "returns only part of the pathname, so the rest is dropped and what is left is \
                 resolved against *default-pathname-defaults*"
            }
            Self::Relative => {
                "is only meaningful when merged back against the defaults it was computed from, \
                 which this call does not do"
            }
        }
    }
}

/// One pathname round-tripped through a string into a filesystem call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamestringRoundTrip {
    pub span: ByteSpan,
    pub converter: Converter,
    /// The converter as written, for the message.
    pub converter_name: String,
}

/// Reads one filesystem call and reports the conversion in its designator.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<NamestringRoundTrip> {
    // The qualifier is stripped once and the table compared against the result.
    // `symbol_is` per entry would re-split the same head eleven times, and this
    // rule's heads include `with-open-file`, which is dense in ordinary code —
    // measured at 0.16µs/invocation before this change and 0.03µs after.
    let head = unqualified(list_head(view)?);
    let (name, position) = FILE_OPERATORS
        .iter()
        .find(|(operator, _)| head.eq_ignore_ascii_case(operator))
        .copied()?;

    // `with-open-file` puts its designator inside the binding form:
    // `(with-open-file (stream <designator> …) …)`.
    let designator = if name == "with-open-file" {
        view.children.get(1)?.children.get(1)?
    } else {
        view.children.get(position)?
    };

    // A quoted or read-time-evaluated designator is not a plain call, and the
    // rule does not claim to read one. Checked before the head is read so a
    // non-call designator costs nothing further.
    if !designator.reader_prefixes.is_empty() {
        return None;
    }
    let converter_head = list_head(designator)?;
    let converter = Converter::of(converter_head)?;
    Some(NamestringRoundTrip {
        span: designator.span,
        converter,
        converter_name: converter_head.to_owned(),
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(found) = examine(view) else {
            return Ok(());
        };
        // Asked only once a finding already exists, never per visited node.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            found.span,
            format!(
                "{} {}; this operator already accepts a pathname, so pass the pathname itself",
                found.converter_name,
                found.converter.complaint()
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn converter(input: &str) -> Option<Converter> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.converter)
    }

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    /// How many findings the *real* dispatch produces, which is the only thing
    /// that exercises the quote guard, the head filter, and the dialect scope.
    fn reports(input: &str) -> usize {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        collect_lint_outcomes(
            catalog,
            &index,
            std::path::Path::new("t.lisp"),
            Dialect::CommonLisp,
            &tree,
            input,
            RuleSelection::All,
        )
        .expect("lint pass")
        .len()
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_namestring_handed_to_a_filesystem_call() {
        assert_eq!(converter("(open (namestring path))"), Some(Converter::Full));
        assert_eq!(
            converter("(probe-file (namestring (merge-pathnames p base)))"),
            Some(Converter::Full)
        );
        assert_eq!(reports("(open (namestring path))"), 1);
    }

    #[test]
    fn flags_the_fragment_converters() {
        assert_eq!(
            converter("(load (file-namestring path))"),
            Some(Converter::Partial)
        );
        assert_eq!(
            converter("(directory (directory-namestring path))"),
            Some(Converter::Partial)
        );
        assert_eq!(
            converter("(truename (host-namestring path))"),
            Some(Converter::Partial)
        );
    }

    #[test]
    fn flags_enough_namestring() {
        assert_eq!(
            converter("(open (enough-namestring path base))"),
            Some(Converter::Relative)
        );
    }

    #[test]
    fn reads_the_designator_inside_a_with_open_file_binding() {
        assert_eq!(
            converter("(with-open-file (s (namestring path)) (read s))"),
            Some(Converter::Full)
        );
    }

    #[test]
    fn reads_the_heads_case_insensitively_and_past_a_package_prefix() {
        assert_eq!(
            converter("(CL:OPEN (cl:namestring path))"),
            Some(Converter::Full)
        );
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_pathname_passed_directly() {
        assert_eq!(converter("(open path)"), None);
        assert_eq!(
            converter("(open (merge-pathnames #p\"in.txt\" base))"),
            None
        );
        assert_eq!(converter("(with-open-file (s path) (read s))"), None);
    }

    /// The disjointness with `unportable-pathname`, pinned: that rule's subject
    /// is a string literal, which is never a call, so this rule stays silent on
    /// exactly the forms it reports.
    #[test]
    fn does_not_flag_a_string_literal_designator() {
        assert_eq!(converter(r#"(open "/etc/hosts")"#), None);
        assert_eq!(converter(r#"(load "data/in.txt")"#), None);
        assert_eq!(
            converter(r#"(with-open-file (s "data/in.txt") (read s))"#),
            None
        );
    }

    #[test]
    fn does_not_flag_a_namestring_used_for_something_other_than_reopening() {
        // Printing or comparing a namestring is exactly what it is for.
        assert_eq!(converter("(format t \"~a~%\" (namestring path))"), None);
        assert_eq!(converter("(string= (namestring a) (namestring b))"), None);
    }

    #[test]
    fn does_not_flag_a_format_stringification() {
        // Documented non-trigger: `~a` of an already-string value is a no-op
        // and cannot be told apart from flattening a pathname.
        assert_eq!(converter("(open (format nil \"~a\" path))"), None);
    }

    #[test]
    fn does_not_flag_a_similarly_named_call() {
        assert_eq!(converter("(open (my-namestring-cache path))"), None);
        assert_eq!(converter("(open (parse-namestring text))"), None);
    }

    #[test]
    fn does_not_flag_a_malformed_call() {
        assert_eq!(converter("(open)"), None);
        assert_eq!(converter("(with-open-file)"), None);
        assert_eq!(converter("(with-open-file (s))"), None);
    }

    // -- quote-context negative ----------------------------------------------

    #[test]
    fn does_not_flag_a_call_in_quoted_data() {
        assert_eq!(reports("'(open (namestring path))"), 0);
        assert_eq!(reports("(quote (open (namestring path)))"), 0);
        assert_eq!(reports("`(open (namestring path))"), 0);
        assert_eq!(reports("'(a ,(open (namestring path)))"), 0);
        assert_eq!(reports("'(outer (open (namestring path)))"), 0);
    }

    #[test]
    fn flags_a_call_unquoted_back_into_code() {
        assert_eq!(reports("`(a ,(open (namestring path)))"), 1);
    }

    // -- string-literal negative ---------------------------------------------

    #[test]
    fn does_not_flag_a_call_written_inside_a_string() {
        assert_eq!(reports(r#"(format nil "(open (namestring path))")"#), 0);
        assert_eq!(
            reports(r#"(defun f () "calls (open (namestring path))" nil)"#),
            0
        );
    }
}
