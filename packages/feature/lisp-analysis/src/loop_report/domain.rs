//! The `loop` clause grammar: what each `loop` binds, accumulates, and
//! terminates on.
//!
//! `loop` is the one standard macro with a grammar of its own rather than an
//! S-expression shape. To every tree walker in this tool it is a flat list of
//! symbols, which is why `loop` bindings do not appear in the binding table,
//! why `unused-parameters` cannot see a `for` variable, and why renaming inside
//! a `loop` is the one rename this tool declines.
//!
//! Reading the clauses is what closes that gap. The report answers three
//! questions no other one can:
//!
//! - **What does this `loop` bind?** `for`, `as`, and `with` introduce
//!   variables whose scope is the whole loop, including the clauses written
//!   above them.
//! - **What does it accumulate, and into what?** Mixing `collect` with `sum` in
//!   one loop without `into` is a type error the compiler reports as a
//!   confusing macro-expansion failure.
//! - **Can it terminate?** A `loop` with no `while`, `until`, `repeat`,
//!   `for … from/in/on/across`, `always`, `never`, `thereis`, or explicit
//!   `return` runs forever. That is occasionally intended and usually not.
//!
//! The parse is clause-keyword-driven and deliberately shallow: it reads the
//! keywords and the token positions the grammar fixes, and records anything it
//! does not recognise as an unparsed clause rather than guessing. A `loop` whose
//! clauses come from a macro is reported as unparsed, not as empty.

use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding, line_of};

/// What kind of `loop` this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind {
    /// An extended `loop` with recognised clauses.
    Extended,
    /// A `loop` whose body is forms rather than clauses: `(loop (do-thing))`.
    /// It iterates forever until something throws.
    Simple,
    /// An extended `loop` this parser could not read end to end.
    Unparsed,
}

impl LoopKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Extended => "extended",
            Self::Simple => "simple",
            Self::Unparsed => "unparsed",
        }
    }
}

/// One `loop` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopForm {
    pub kind: LoopKind,
    /// Variables introduced by `for`, `as`, and `with`, in clause order.
    pub bindings: Vec<String>,
    /// Accumulation clauses, as `verb` or `verb->target` when `into` is used.
    pub accumulations: Vec<String>,
    /// Clause keywords that can end the loop.
    pub terminations: Vec<String>,
    /// Every recognised clause keyword, in order.
    pub clauses: Vec<String>,
    /// Whether anything in the loop can stop it.
    pub terminates: bool,
    /// Accumulation verbs that disagree about the result type, when no `into`
    /// separates them. `collect` and `sum` in one loop is a bug.
    pub conflicting_accumulations: Vec<String>,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for LoopForm {
    fn kind(&self) -> &'static str {
        if !self.terminates {
            "unterminated"
        } else if !self.conflicting_accumulations.is_empty() {
            "conflicting-accumulation"
        } else {
            self.kind.label()
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("binds=({})", self.bindings.join(" ")),
            format!("accumulates=({})", self.accumulations.join(" ")),
            format!("terminates={}", self.terminates),
            format!("clauses=({})", self.clauses.join(" ")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("loop_kind", json!(self.kind.label())),
            ("bindings", json!(self.bindings)),
            ("accumulations", json!(self.accumulations)),
            ("terminations", json!(self.terminations)),
            ("clauses", json!(self.clauses)),
            ("terminates", json!(self.terminates)),
            (
                "conflicting_accumulations",
                json!(self.conflicting_accumulations),
            ),
        ]
    }
}

/// Clause keywords that introduce a variable, whose name is the next token.
const BINDING_KEYWORDS: [&str; 3] = ["for", "as", "with"];

/// Accumulation verbs, paired with the result type they build.
///
/// The type is what makes a conflict detectable: two verbs building a list are
/// compatible, a list and a number are not.
const ACCUMULATORS: [(&str, &str); 10] = [
    ("collect", "list"),
    ("collecting", "list"),
    ("append", "list"),
    ("appending", "list"),
    ("nconc", "list"),
    ("nconcing", "list"),
    ("sum", "number"),
    ("summing", "number"),
    ("count", "number"),
    ("counting", "number"),
];

/// Extremum accumulators, kept separate only because they are numeric too.
const EXTREMUM_ACCUMULATORS: [&str; 4] = ["maximize", "maximizing", "minimize", "minimizing"];

/// Clause keywords that can end the loop.
const TERMINATION_KEYWORDS: [&str; 8] = [
    "while",
    "until",
    "repeat",
    "always",
    "never",
    "thereis",
    "return",
    "return-from",
];

/// Every other clause keyword the grammar defines. Recognised so an unknown
/// token can be reported as unparsed rather than silently ignored.
const OTHER_KEYWORDS: [&str; 18] = [
    "do", "doing", "when", "if", "unless", "else", "end", "into", "from", "to", "below", "above",
    "downto", "upto", "by", "in", "on", "across",
];

#[must_use]
pub fn build_loop_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<LoopForm> {
    let modelled = dialect == Dialect::CommonLisp;
    let source = tree.source();
    let mut findings = Vec::new();

    if modelled {
        collect(&tree.root_view(), source, &mut findings);
    }

    let unterminated = findings
        .iter()
        .filter(|form: &&LoopForm| !form.terminates)
        .count();
    let conflicting = findings
        .iter()
        .filter(|form| !form.conflicting_accumulations.is_empty())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        findings,
        vec![
            ("unterminated_count", json!(unterminated)),
            ("conflicting_accumulation_count", json!(conflicting)),
        ],
    )
}

fn collect(view: &ExpressionView, source: &str, findings: &mut Vec<LoopForm>) {
    if list_head(view).is_some_and(|head| common_lisp_operator_head_eq(head, "loop")) {
        findings.push(read_loop(view, source));
    }
    for child in &view.children {
        collect(child, source, findings);
    }
}

fn read_loop(view: &ExpressionView, source: &str) -> LoopForm {
    let tokens = &view.children[1..];

    let mut bindings = Vec::new();
    let mut accumulations = Vec::new();
    let mut accumulated_types = Vec::new();
    let mut terminations = Vec::new();
    let mut clauses = Vec::new();
    let mut recognised = 0usize;
    let mut has_return = false;

    let mut index = 0;
    while index < tokens.len() {
        let Some(keyword) = atom_symbol_text(&tokens[index]).map(str::to_ascii_lowercase) else {
            // A parenthesized form is a body form, not a clause keyword.
            index += 1;
            continue;
        };

        if BINDING_KEYWORDS.contains(&keyword.as_str()) {
            recognised += 1;
            clauses.push(keyword.clone());
            if let Some(name) = tokens.get(index + 1).and_then(atom_symbol_text) {
                bindings.push(name.to_ascii_uppercase());
            }
            index += 2;
            continue;
        }

        if let Some((_, result)) = ACCUMULATORS
            .iter()
            .find(|(verb, _)| *verb == keyword.as_str())
        {
            recognised += 1;
            clauses.push(keyword.clone());
            // `collect x into xs` accumulates into a named variable, which
            // takes it out of the loop's single implicit result and so out of
            // any conflict with another verb.
            let into = tokens
                .get(index + 2)
                .and_then(atom_symbol_text)
                .filter(|token| token.eq_ignore_ascii_case("into"))
                .and(tokens.get(index + 3).and_then(atom_symbol_text));
            match into {
                Some(target) => {
                    accumulations.push(format!("{keyword}->{}", target.to_ascii_uppercase()));
                    bindings.push(target.to_ascii_uppercase());
                    index += 4;
                }
                None => {
                    accumulations.push(keyword.clone());
                    accumulated_types.push((keyword.clone(), (*result).to_owned()));
                    index += 2;
                }
            }
            continue;
        }

        if EXTREMUM_ACCUMULATORS.contains(&keyword.as_str()) {
            recognised += 1;
            clauses.push(keyword.clone());
            accumulations.push(keyword.clone());
            accumulated_types.push((keyword.clone(), "number".to_owned()));
            index += 2;
            continue;
        }

        if TERMINATION_KEYWORDS.contains(&keyword.as_str()) {
            recognised += 1;
            clauses.push(keyword.clone());
            terminations.push(keyword.clone());
            has_return = true;
            index += 1;
            continue;
        }

        if OTHER_KEYWORDS.contains(&keyword.as_str()) {
            recognised += 1;
            clauses.push(keyword.clone());
            // `for x in xs` and friends iterate a finite sequence, so the
            // subclause keyword is what makes the loop terminate — the `for`
            // alone does not.
            if matches!(
                keyword.as_str(),
                "in" | "on" | "across" | "to" | "below" | "downto" | "upto"
            ) {
                terminations.push(keyword.clone());
                has_return = true;
            }
            index += 1;
            continue;
        }

        index += 1;
    }

    // Two accumulation verbs building different result types, with no `into`
    // to separate them.
    let mut conflicting = accumulated_types
        .iter()
        .filter(|(_, result)| accumulated_types.iter().any(|(_, other)| other != result))
        .map(|(verb, _)| verb.clone())
        .collect::<Vec<_>>();
    conflicting.sort();
    conflicting.dedup();

    let kind = if recognised == 0 {
        LoopKind::Simple
    } else if recognised < clause_keyword_candidates(tokens) {
        LoopKind::Unparsed
    } else {
        LoopKind::Extended
    };

    LoopForm {
        kind,
        bindings,
        accumulations,
        terminations,
        clauses,
        // A simple `loop` never terminates on its own; an extended one does
        // only if a clause says so.
        terminates: has_return,
        conflicting_accumulations: conflicting,
        span: view.span,
        line: line_of(source, view.span.start().get()),
    }
}

/// How many bare symbols the loop body holds.
///
/// Compared against the recognised-keyword count to decide whether the parse
/// covered the whole form. A `loop` full of symbols this grammar does not know
/// is reported as unparsed rather than as a loop that binds nothing.
fn clause_keyword_candidates(tokens: &[ExpressionView]) -> usize {
    tokens
        .iter()
        .filter(|token| atom_symbol_text(token).is_some())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<LoopForm> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_loop_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn only(source: &str) -> LoopForm {
        let report = report(source);
        assert_eq!(report.findings.len(), 1, "{report:?}");
        report.findings[0].clone()
    }

    #[test]
    fn a_for_clause_reports_the_variable_it_binds() {
        let form = only("(defun f (xs) (loop for x in xs collect x))");
        assert_eq!(form.bindings, vec!["X".to_owned()]);
        assert!(form.terminates);
    }

    #[test]
    fn a_with_clause_binds_too() {
        let form = only("(defun f () (loop with total = 0 repeat 3 do (incf total)))");
        assert!(form.bindings.contains(&"TOTAL".to_owned()));
    }

    #[test]
    fn an_accumulation_is_reported_with_its_verb() {
        let form = only("(defun f (xs) (loop for x in xs collect x))");
        assert_eq!(form.accumulations, vec!["collect".to_owned()]);
    }

    #[test]
    fn an_into_target_is_reported_and_bound() {
        let form = only("(defun f (xs) (loop for x in xs collect x into ys finally (return ys)))");
        assert_eq!(form.accumulations, vec!["collect->YS".to_owned()]);
        assert!(form.bindings.contains(&"YS".to_owned()));
    }

    #[test]
    fn two_verbs_building_different_types_are_a_conflict() {
        let form = only("(defun f (xs) (loop for x in xs collect x sum x))");
        assert_eq!(
            form.conflicting_accumulations,
            vec!["collect".to_owned(), "sum".to_owned()]
        );
        assert_eq!(form.kind(), "conflicting-accumulation");
    }

    #[test]
    fn two_verbs_building_the_same_type_are_not_a_conflict() {
        let form = only("(defun f (xs) (loop for x in xs collect x append x))");
        assert!(form.conflicting_accumulations.is_empty());
    }

    #[test]
    fn an_into_target_takes_a_verb_out_of_the_conflict() {
        let form = only("(defun f (xs) (loop for x in xs collect x into ys sum x))");
        assert!(form.conflicting_accumulations.is_empty(), "{form:?}");
    }

    #[test]
    fn a_loop_with_nothing_to_stop_it_is_reported() {
        let form = only("(defun f () (loop do (poll)))");
        assert!(!form.terminates);
        assert_eq!(form.kind(), "unterminated");
    }

    #[test]
    fn a_body_only_loop_is_reported_as_simple() {
        let form = only("(defun f () (loop (poll)))");
        assert_eq!(form.kind, LoopKind::Simple);
        assert!(!form.terminates);
    }

    #[test]
    fn a_while_clause_makes_the_loop_terminable() {
        let form = only("(defun f () (loop while (ready-p) do (poll)))");
        assert!(form.terminates);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree = SyntaxTree::parse_with_dialect("(loop [x 1] (recur x))", Dialect::Clojure)
            .expect("parse");
        let report = build_loop_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
