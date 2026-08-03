//! A reader for the Common Lisp `loop` macro's top-level clause grammar, far
//! enough to answer three questions the clause *sequence* alone decides.
//!
//! `loop` is the one standard macro with a grammar of its own rather than an
//! S-expression shape (CLHS 6.1). This module does not implement that grammar.
//! It tokenizes the top level, tells clause keywords from operands, and groups
//! variable clauses into the **parallel groups** that `and` creates — and it
//! refuses to answer at all whenever the question is not safely decidable.
//!
//! # Why the refusals matter
//!
//! Every clause keyword is an ordinary symbol, so `sum`, `count`, `do`, and
//! `it` are all legal variable names. `(loop for count from 1 to 3 collect
//! count)` is correct, idiomatic code in which the token `count` is a variable
//! twice and a keyword never. Three guards prevent a rule built on this module
//! from warning about it:
//!
//! - **Bound names are never keywords.** A first pass collects every name
//!   introduced by `for`/`as`/`with`/`into`/`named`/`and`, including every
//!   symbol of a destructuring pattern such as `for (sum . rest) in alist`. The
//!   pass deliberately over-collects, which can only *lose* findings.
//! - **Operand positions are never keywords.** A variable merely *read* after
//!   `to`, `below`, `from`, `=`, `in`, … is bound somewhere else entirely and
//!   the first pass never sees it. `(loop for i from 1 to count collect i)` is
//!   the everyday shape that needs this.
//! - **An unmodelled sub-grammar aborts the whole form.** A top-level `being`
//!   iteration path, a simple `loop`, or a `loop` reached as data all return
//!   `None`, which callers must treat as "report nothing", never as "nothing
//!   found".
//!
//! # Relationship to `lint-iteration-flow`'s `loop_syntax`
//!
//! That module solves the same tokenizing problem and this one reproduces its
//! three guards, which were arrived at there by corpus measurement and are not
//! reinvented lightly. What it does **not** provide is the reason this module
//! exists: it documents compound `and` clauses as "tokenized but never
//! interpreted", and lists `and` among its `NAME_INTRODUCERS` precisely so that
//! it over-collects and loses findings. Parallel-group structure is exactly
//! what [`ParallelGroup`] must model. See this crate's README for the merge
//! case.

use paredit_core_syntax::sexpr::{ExpressionView, ReaderPrefix};

use crate::shared::{is_call_to, list_head, symbol_word};
use paredit_core_syntax::view_query::unqualified;

/// Clause keywords that open a clause of their own.
const CLAUSE_KEYWORDS: [&str; 34] = [
    "named",
    "with",
    "for",
    "as",
    "initially",
    "finally",
    "do",
    "doing",
    "return",
    "collect",
    "collecting",
    "append",
    "appending",
    "nconc",
    "nconcing",
    "count",
    "counting",
    "sum",
    "summing",
    "maximize",
    "maximizing",
    "minimize",
    "minimizing",
    "if",
    "when",
    "unless",
    "else",
    "end",
    "while",
    "until",
    "repeat",
    "always",
    "never",
    "thereis",
];

/// Keywords that are part of a clause rather than the head of one.
const SUBCLAUSE_KEYWORDS: [&str; 17] = [
    "from", "upfrom", "downfrom", "to", "upto", "downto", "below", "above", "by", "in", "on", "=",
    "then", "across", "of-type", "into", "and",
];

/// Clause keywords after which the very next token names a variable rather than
/// being a keyword in its own right.
///
/// **`and` is deliberately absent, unlike in `lint-iteration-flow`'s
/// `loop_syntax`.** There it is present so the pass over-collects and loses
/// findings, which is safe for a reader that never interprets `and`. Here it
/// was actively harmful: in `(loop for x in items do (a x) and do (b x))` the
/// `and` introduced the *main-clause* token `do` as a bound name, which demoted
/// the earlier `do` from a keyword to an operand — so the variable clause never
/// closed, the second `do` was read as a variable name, and
/// `loop-parallel-binding-reads-sibling` reported a parallel group that does not
/// exist. It was a false positive on ordinary code, found by this crate's own
/// negative test.
///
/// Dropping it costs only false negatives: a variable genuinely introduced by
/// `and` and spelled like a clause keyword — `for a from 1 to 3 and count = 5` —
/// now reads as a keyword, which closes the group and loses the finding.
/// [`LoopForm::parallel_groups`] handles `and` explicitly instead.
const NAME_INTRODUCERS: [&str; 5] = ["for", "as", "with", "into", "named"];

/// Keywords after which the very next token is an operand — a form, a literal,
/// or a variable being *read* — and therefore cannot be a clause keyword
/// whatever it is spelled.
///
/// Every keyword here takes exactly one operand form, so marking exactly the
/// next token is right rather than merely safe. `of-type`, `into`, and `and`
/// are deliberately absent: the first two and `and` are handled by
/// [`NAME_INTRODUCERS`] instead. `do`/`doing`/`initially`/`finally` are absent
/// because they take *one or more* forms, so "the next token" does not delimit
/// them.
const OPERAND_INTRODUCERS: [&str; 17] = [
    "from", "upfrom", "downfrom", "to", "upto", "downto", "below", "above", "by", "in", "on",
    "across", "=", "then", "repeat", "while", "until",
];

/// Clause keywords that introduce a loop variable and so may open a parallel
/// group.
const VARIABLE_CLAUSE_KEYWORDS: [&str; 3] = ["with", "for", "as"];

/// Every accumulation verb, in both spellings.
const ACCUMULATION_VERBS: [&str; 14] = [
    "collect",
    "collecting",
    "append",
    "appending",
    "nconc",
    "nconcing",
    "count",
    "counting",
    "sum",
    "summing",
    "maximize",
    "maximizing",
    "minimize",
    "minimizing",
];

#[must_use]
pub fn is_clause_keyword(word: &str) -> bool {
    CLAUSE_KEYWORDS.contains(&word)
}

#[must_use]
pub fn is_accumulation_verb(word: &str) -> bool {
    ACCUMULATION_VERBS.contains(&word)
}

/// Whether `word` sits *inside* a clause rather than heading one.
#[must_use]
pub fn is_subclause_keyword(word: &str) -> bool {
    SUBCLAUSE_KEYWORDS.contains(&word)
}

/// What one top-level token of a `loop` form was read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRole {
    /// A clause or sub-clause keyword.
    Keyword,
    /// Anything else: a variable name, a literal, a compound form.
    Operand,
}

/// One top-level token of a `loop` form.
#[derive(Debug)]
pub struct LoopToken<'a> {
    pub view: &'a ExpressionView,
    /// The token's symbol text, lowercased and package-unqualified, for a token
    /// that reads as a bare symbol. `None` for a compound form, string, or
    /// number.
    pub word: Option<String>,
    pub role: TokenRole,
}

impl LoopToken<'_> {
    /// The token's text when it was read as a keyword, else `None`.
    #[must_use]
    pub fn keyword(&self) -> Option<&str> {
        match self.role {
            TokenRole::Keyword => self.word.as_deref(),
            TokenRole::Operand => None,
        }
    }

    /// The token's text when it was read as a bare-symbol operand, else `None`.
    #[must_use]
    pub fn operand_symbol(&self) -> Option<&str> {
        match self.role {
            TokenRole::Operand => self.word.as_deref(),
            TokenRole::Keyword => None,
        }
    }
}

/// One `loop` form's top-level tokens, already classified.
#[derive(Debug)]
pub struct LoopForm<'a> {
    pub tokens: Vec<LoopToken<'a>>,
}

/// One variable binding within a parallel group.
#[derive(Debug, Clone)]
pub struct Binding {
    /// Every name the binding introduces. More than one for a destructuring
    /// pattern: `for (a . b) in alist` binds both.
    pub names: Vec<String>,
    /// The token index of the name or pattern itself.
    pub name_index: usize,
    /// Token indices of every operand evaluated **once, at loop setup**: the
    /// `=` init form and the `in`/`on`/`across`/`from`/`to`/`by` operands.
    ///
    /// This is the position where a reference to a parallel sibling is a
    /// defect, because at that moment the sibling holds `nil` or is not bound
    /// at all.
    pub init_operands: Vec<usize>,
    /// The token index of the `then` step form, evaluated from the second
    /// iteration on.
    ///
    /// Kept separate from [`Self::init_operands`] because a sibling reference
    /// here is the deliberate "previous value" idiom rather than a defect —
    /// `(loop for x in l and prev = nil then x …)` is how Common Lisp spells
    /// it, and it *requires* the `and`. Measured under SBCL 2.6.0, that form
    /// yields `((NIL . 1) (1 . 2) (2 . 3))` while the sequential `for … for …`
    /// spelling yields the wrong `((NIL . 1) (2 . 2) (3 . 3))`.
    pub step_operand: Option<usize>,
}

/// Variable clauses joined by `and`, which `loop` binds **in parallel**.
///
/// A group of one is the ordinary case and carries no risk. A group of two or
/// more is the `do` versus `do*` distinction in `loop` clothing.
#[derive(Debug, Clone)]
pub struct ParallelGroup {
    pub bindings: Vec<Binding>,
}

/// Every symbol in a destructuring pattern, appended to `names`.
///
/// `for (a (b . c)) in pairs` binds `a`, `b`, and `c`; any of them may be
/// spelled like a clause keyword, and every one must therefore stop reading as
/// one. Lambda-list markers such as `&optional` are collected too, which is
/// harmless: they are not clause keywords either.
fn collect_pattern_names(view: &ExpressionView, names: &mut Vec<String>) {
    if let Some(word) = symbol_word(view) {
        // The dotted-pair marker in `for (k . v) in alist` reads as an atom but
        // names nothing.
        if word != "." {
            names.push(word);
        }
    }
    for child in &view.children {
        collect_pattern_names(child, names);
    }
}

/// Whether `view` is a `(loop …)` form, however its head is spelled.
#[must_use]
pub fn is_loop_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| unqualified(head).eq_ignore_ascii_case("loop"))
}

/// Reads one `loop` form's top-level tokens, or `None` when this reader
/// declines to model the form.
///
/// Callers must treat `None` as "report nothing", never as "nothing found".
#[must_use]
pub fn read_loop_form<'a>(view: &'a ExpressionView) -> Option<LoopForm<'a>> {
    if !is_loop_form(view) {
        return None;
    }
    // A quoted or quasiquoted `loop` is data or a macro template, not a call.
    // This catches only a prefix on the `loop` form itself; a `loop` nested
    // deeper inside quoted data is excluded by each rule's own
    // `is_unevaluated_at` check.
    //
    // Only `'` and `` ` `` disqualify. A `,`-prefixed `loop` inside a
    // quasiquote — `` `(progn ,(loop …)) `` — is code that escaped back out of
    // the template, and rejecting every prefix silenced exactly that case.
    if view
        .reader_prefixes
        .iter()
        .any(|prefix| matches!(prefix, ReaderPrefix::Quote | ReaderPrefix::Quasiquote))
    {
        return None;
    }

    let children = view.children.get(1..)?;
    let words: Vec<Option<String>> = children.iter().map(symbol_word).collect();

    // The `being` iteration-path sub-grammar is not modelled. Bailing on the
    // whole form is the only safe response, because the path's own tokens
    // (`the`, `each`, `of`, `hash-key`, `using`) would otherwise read as clause
    // keywords and its `using (hash-value v)` binding would be invisible.
    if words.iter().any(|word| word.as_deref() == Some("being")) {
        return None;
    }

    // A simple loop has no clauses at all, so nothing here applies to it.
    if !words
        .iter()
        .any(|word| word.as_deref().is_some_and(is_clause_keyword))
    {
        return None;
    }

    // Pass 1: every name a clause keyword introduces. Over-collecting is safe —
    // it only demotes a would-be keyword to an operand.
    let mut bound_names: Vec<String> = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let Some(word) = word.as_deref() else {
            continue;
        };
        if !NAME_INTRODUCERS.contains(&word) {
            continue;
        }
        // Written without a `let` chain: this workspace's 1.85 MSRV rejects
        // them even though edition 2024 makes them look available.
        match words.get(index + 1) {
            Some(Some(name)) => bound_names.push(name.clone()),
            _ => {
                if let Some(pattern) = children.get(index + 1) {
                    collect_pattern_names(pattern, &mut bound_names);
                }
            }
        }
    }

    // Pass 2: every position holding an operand of the keyword before it.
    let mut operand_positions = vec![false; words.len()];
    for (index, word) in words.iter().enumerate() {
        let introduces_operand = word
            .as_deref()
            .is_some_and(|word| OPERAND_INTRODUCERS.contains(&word));
        if !introduces_operand {
            continue;
        }
        if let Some(slot) = operand_positions.get_mut(index + 1) {
            *slot = true;
        }
    }

    // Pass 3: classify. Neither a bound name nor an operand position ever reads
    // as a keyword.
    let tokens = children
        .iter()
        .zip(words.iter())
        .enumerate()
        .map(|(offset, (child, word))| {
            let role = match word.as_deref() {
                Some(text)
                    if !operand_positions[offset]
                        && !bound_names.iter().any(|name| name == text)
                        && (is_clause_keyword(text) || SUBCLAUSE_KEYWORDS.contains(&text)) =>
                {
                    TokenRole::Keyword
                }
                _ => TokenRole::Operand,
            };
            LoopToken {
                view: child,
                word: word.clone(),
                role,
            }
        })
        .collect();

    Some(LoopForm { tokens })
}

/// What the group builder is currently reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// The next operand names a variable (or is a destructuring pattern).
    Name,
    /// The next operand is the `then` step form.
    Step,
    /// The next operand is an `of-type` type specifier, which is not code.
    TypeSpec,
    /// Anything else: an ordinary operand of the current clause.
    Operand,
}

impl LoopForm<'_> {
    /// Every parallel group of variable clauses in this `loop`.
    ///
    /// A group is opened by `for`/`as`/`with` and extended by each `and` that
    /// follows while a variable clause is still open. Any other clause keyword
    /// closes the group: `and` between *main* clauses (`collect x and count y`)
    /// joins body code, not bindings, and must not extend a group.
    ///
    /// Groups of a single binding are included; they are the ordinary case and
    /// callers filter them out.
    #[must_use]
    pub fn parallel_groups(&self) -> Vec<ParallelGroup> {
        let mut groups: Vec<ParallelGroup> = Vec::new();
        let mut group: Vec<Binding> = Vec::new();
        let mut current: Option<Binding> = None;
        let mut expect = Expect::Operand;

        for (index, token) in self.tokens.iter().enumerate() {
            match token.keyword() {
                Some(word) if VARIABLE_CLAUSE_KEYWORDS.contains(&word) => {
                    // A new variable clause not joined by `and` starts its own
                    // group: `for a = 1 for b = (f a)` binds sequentially, and
                    // reading `a` there is correct.
                    close_binding(&mut current, &mut group);
                    close_group(&mut group, &mut groups);
                    expect = Expect::Name;
                }
                Some("and") => {
                    // Only extends a group while a variable clause is open.
                    if current.is_some() {
                        close_binding(&mut current, &mut group);
                        expect = Expect::Name;
                    }
                }
                Some("then") => expect = Expect::Step,
                Some("of-type") => expect = Expect::TypeSpec,
                // `from`, `to`, `below`, `by`, `in`, `on`, `across`, `=` are
                // *inside* the clause they follow, not the head of a new one.
                // Treating them as terminators closed every binding the moment
                // it opened, which is how the first version of this builder
                // found no parallel groups at all.
                Some(word) if is_subclause_keyword(word) => {}
                Some(_) => {
                    // Every real clause keyword — `collect`, `do`, `while`,
                    // `finally`, `when` — closes the variable clause and its
                    // group. A later `for` opens a fresh one.
                    close_binding(&mut current, &mut group);
                    close_group(&mut group, &mut groups);
                    expect = Expect::Operand;
                }
                None => match expect {
                    Expect::Name => {
                        let mut names = Vec::new();
                        collect_pattern_names(token.view, &mut names);
                        current = Some(Binding {
                            names,
                            name_index: index,
                            init_operands: Vec::new(),
                            step_operand: None,
                        });
                        expect = Expect::Operand;
                    }
                    Expect::Step => {
                        if let Some(binding) = current.as_mut() {
                            binding.step_operand = Some(index);
                        }
                        expect = Expect::Operand;
                    }
                    Expect::TypeSpec => expect = Expect::Operand,
                    Expect::Operand => {
                        if let Some(binding) = current.as_mut() {
                            binding.init_operands.push(index);
                        }
                    }
                },
            }
        }
        close_binding(&mut current, &mut group);
        close_group(&mut group, &mut groups);
        groups
    }
}

fn close_binding(current: &mut Option<Binding>, group: &mut Vec<Binding>) {
    if let Some(binding) = current.take() {
        group.push(binding);
    }
}

fn close_group(group: &mut Vec<Binding>, groups: &mut Vec<ParallelGroup>) {
    if !group.is_empty() {
        groups.push(ParallelGroup {
            bindings: std::mem::take(group),
        });
    }
}

/// Forms that bind names of their own, and which this crate therefore refuses
/// to look inside when asking whether a form reads a variable.
///
/// Refusing is conservative in the finding-losing direction: a `let` that
/// shadows the name would make the reference innocent, and proving it does not
/// shadow costs a scope analysis this layer does not have. `loop` is here for
/// the reason `lint-iteration-flow`'s `support.rs` gives — it binds through a
/// grammar rather than a binding list, so "does this `loop` bind `n`?" cannot
/// be answered from a child index.
const OPAQUE_BINDERS: [&str; 17] = [
    "let",
    "let*",
    "lambda",
    "flet",
    "labels",
    "macrolet",
    "symbol-macrolet",
    "destructuring-bind",
    "multiple-value-bind",
    "do",
    "do*",
    "dolist",
    "dotimes",
    "loop",
    "with-slots",
    "with-accessors",
    "prog",
];

/// Whether `view` reads the variable `name`, conservatively.
///
/// Answers `false` — "cannot prove it does" — for anything under a reader
/// prefix that makes it data, and for anything inside a form that binds names
/// of its own. Both directions lose findings rather than invent them.
#[must_use]
pub fn reads_variable(view: &ExpressionView, name: &str) -> bool {
    // `'x`, `` `x ``, `#'x` — none of these reads the *variable* x.
    if !view.reader_prefixes.is_empty() {
        return false;
    }
    if let Some(word) = symbol_word(view) {
        return word == name;
    }
    if is_call_to(view, "quote") || is_call_to(view, "function") {
        return false;
    }
    if list_head(view).is_some_and(|head| {
        let head = unqualified(head).to_ascii_lowercase();
        OPAQUE_BINDERS.contains(&head.as_str())
    }) {
        return false;
    }
    // The head of a call is a function name, not a variable read. `(count x)`
    // calls `count`; it does not read a variable named `count`.
    let body = if view.kind == paredit_core_syntax::sexpr::ExpressionKind::List
        && view.delimiter == Some(paredit_core_syntax::sexpr::Delimiter::Paren)
    {
        view.children.get(1..).unwrap_or(&[])
    } else {
        &view.children
    };
    body.iter().any(|child| reads_variable(child, name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path as SexprPath, SyntaxTree};

    fn with_form<T>(input: &str, check: impl FnOnce(Option<LoopForm<'_>>) -> T) -> T {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("root form")
            .view();
        check(read_loop_form(&view))
    }

    fn keywords(input: &str) -> Option<Vec<String>> {
        with_form(input, |form| {
            form.map(|form| {
                form.tokens
                    .iter()
                    .filter_map(|token| token.keyword().map(str::to_owned))
                    .collect()
            })
        })
    }

    /// Each group's binding names, in order.
    fn groups(input: &str) -> Vec<Vec<Vec<String>>> {
        with_form(input, |form| {
            form.map(|form| {
                form.parallel_groups()
                    .into_iter()
                    .map(|group| {
                        group
                            .bindings
                            .into_iter()
                            .map(|binding| binding.names)
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default()
        })
    }

    // --- tokenizing, inherited from the guards lint-iteration-flow proved ---

    #[test]
    fn reads_a_plain_extended_loop() {
        assert_eq!(
            keywords("(loop for x in items collect x)"),
            Some(vec![
                "for".to_owned(),
                "in".to_owned(),
                "collect".to_owned()
            ])
        );
    }

    #[test]
    fn a_variable_named_like_a_keyword_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop for count from 1 to 3 collect count)"),
            Some(vec![
                "for".to_owned(),
                "from".to_owned(),
                "to".to_owned(),
                "collect".to_owned(),
            ])
        );
    }

    #[test]
    fn a_variable_read_in_operand_position_is_never_a_keyword() {
        assert_eq!(
            keywords("(loop for i from 1 to count for j from 1 to 3 collect (list i j))"),
            Some(vec![
                "for".to_owned(),
                "from".to_owned(),
                "to".to_owned(),
                "for".to_owned(),
                "from".to_owned(),
                "to".to_owned(),
                "collect".to_owned(),
            ])
        );
    }

    #[test]
    fn declines_an_iteration_path_a_simple_loop_and_quoted_data() {
        assert_eq!(
            keywords("(loop for k being the hash-keys of table collect k)"),
            None
        );
        assert_eq!(keywords("(loop (do-thing))"), None);
        assert_eq!(keywords("'(loop for x in items collect x)"), None);
        assert_eq!(keywords("`(loop for x in items collect x)"), None);
        assert_eq!(keywords("(dolist (x items) (print x))"), None);
    }

    // --- parallel groups, which is what this module adds -------------------

    #[test]
    fn successive_for_clauses_are_separate_groups() {
        assert_eq!(
            groups("(loop for a from 1 to 3 for b = (* a 10) collect (list a b))"),
            vec![vec![vec!["a".to_owned()]], vec![vec!["b".to_owned()]]]
        );
    }

    #[test]
    fn an_and_joined_clause_shares_a_group() {
        assert_eq!(
            groups("(loop for a from 1 to 3 and b = (* a 10) collect (list a b))"),
            vec![vec![vec!["a".to_owned()], vec!["b".to_owned()]]]
        );
    }

    #[test]
    fn a_with_clause_group_is_read_the_same_way() {
        assert_eq!(
            groups("(loop with a = 1 and b = (* a 2) repeat 1 collect (list a b))"),
            vec![vec![vec!["a".to_owned()], vec!["b".to_owned()]]]
        );
    }

    /// `and` between *main* clauses joins body code, not bindings. Reading it
    /// as a group would invent a binding out of an accumulation operand.
    #[test]
    fn an_and_between_main_clauses_opens_no_group() {
        assert_eq!(
            groups("(loop for x in items collect x and count x)"),
            vec![vec![vec!["x".to_owned()]]]
        );
    }

    #[test]
    fn a_destructuring_pattern_binds_every_symbol_in_it() {
        assert_eq!(
            groups("(loop for (a . b) in alist and c = 1 collect (list a b c))"),
            vec![vec![
                vec!["a".to_owned(), "b".to_owned()],
                vec!["c".to_owned()]
            ]]
        );
    }

    /// The distinction the whole flagship rule rests on: an `=` init form is
    /// setup, a `then` form is a step.
    #[test]
    fn init_and_step_operands_are_told_apart() {
        with_form(
            "(loop for x in items and prev = nil then x collect (cons prev x))",
            |form| {
                let form = form.expect("modelled");
                let groups = form.parallel_groups();
                assert_eq!(groups.len(), 1);
                let prev = &groups[0].bindings[1];
                assert_eq!(prev.names, vec!["prev".to_owned()]);
                // `nil` is the init; `x` is the step and must not count as one.
                assert_eq!(prev.init_operands.len(), 1);
                assert!(prev.step_operand.is_some());
                let step = prev.step_operand.expect("a step operand");
                assert_eq!(form.tokens[step].word.as_deref(), Some("x"));
            },
        );
    }

    /// An `of-type` type specifier is not code and must not read as an init
    /// operand — otherwise `for x of-type fixnum = 0 and y = 1` would report
    /// against the type name.
    #[test]
    fn an_of_type_specifier_is_not_an_init_operand() {
        with_form(
            "(loop for x of-type fixnum = 0 and y = 1 collect x)",
            |form| {
                let form = form.expect("modelled");
                let groups = form.parallel_groups();
                let x = &groups[0].bindings[0];
                assert_eq!(x.names, vec!["x".to_owned()]);
                // Only the `0`, never the `fixnum`.
                assert_eq!(x.init_operands.len(), 1);
                assert_eq!(
                    form.tokens[x.init_operands[0]].view.text.as_deref(),
                    Some("0")
                );
            },
        );
    }

    // --- reads_variable ----------------------------------------------------

    #[test]
    fn a_quoted_or_sharp_quoted_symbol_is_not_a_variable_read() {
        for source in ["'a", "#'a", "(quote a)", "(function a)", "`a"] {
            let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
            let view = tree
                .select_path(&SexprPath::root_child(0))
                .expect("form")
                .view();
            assert!(!reads_variable(&view, "a"), "{source} read as a variable");
        }
    }

    #[test]
    fn a_call_head_is_not_a_variable_read() {
        let tree = SyntaxTree::parse_with_dialect("(count x)", Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        assert!(!reads_variable(&view, "count"));
        assert!(reads_variable(&view, "x"));
    }

    #[test]
    fn a_binder_that_could_shadow_stops_the_search() {
        for source in [
            "(let ((a 1)) a)",
            "(lambda (a) a)",
            "(loop for a in z collect a)",
            "(destructuring-bind (a) z a)",
        ] {
            let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
            let view = tree
                .select_path(&SexprPath::root_child(0))
                .expect("form")
                .view();
            assert!(!reads_variable(&view, "a"), "{source} was not refused");
        }
    }

    #[test]
    fn a_nested_read_is_found() {
        let tree =
            SyntaxTree::parse_with_dialect("(* (+ a 1) 10)", Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&SexprPath::root_child(0))
            .expect("form")
            .view();
        assert!(reads_variable(&view, "a"));
        assert!(!reads_variable(&view, "b"));
    }
}
