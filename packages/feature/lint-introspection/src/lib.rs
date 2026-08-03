#![doc = include_str!("../README.md")]

pub mod intern_dynamic_package_target;
pub mod introspection_probe_unchecked;
pub mod support;
pub mod symbol_function_fset_dynamic_name;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// Every rule here driven through the *engine*, rather than through its own
/// `build_*_report`.
///
/// The two entry points do not share their quote handling. A report walks with
/// [`crate::support::for_each_evaluated_subview`], which never visits data at
/// all; a head-filtered rule is handed matched nodes by the dispatcher
/// *including* the ones inside `'(…)` and `` `(…) ``, and depends on each
/// `check`'s [`crate::support::is_unevaluated_at`] call to decline them.
/// Testing only the reports would leave that call — the one thing standing
/// between these three rules and a finding on every macro template in the
/// program — unexercised.
///
/// Running the real pass also covers the two declarations a domain test cannot
/// see: each rule's head filter and its `RuleDialectScope`.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{
        PassOptions, build_head_index, collect_lint_outcomes, collect_lint_pass,
    };
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 3] = [
        RuleEntry::new(
            &crate::intern_dynamic_package_target::rule::META,
            &crate::intern_dynamic_package_target::rule::RULE,
        ),
        RuleEntry::new(
            &crate::introspection_probe_unchecked::rule::META,
            &crate::introspection_probe_unchecked::rule::RULE,
        ),
        RuleEntry::new(
            &crate::symbol_function_fset_dynamic_name::rule::META,
            &crate::symbol_function_fset_dynamic_name::rule::RULE,
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
            dialect,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
        names.sort_unstable();
        names
    }

    // -- each rule reaches the engine ---------------------------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        assert_eq!(
            fired(
                "(intern \"HANDLER\" (find-package module))",
                Dialect::CommonLisp
            ),
            vec!["intern-dynamic-package-target"]
        );
        assert_eq!(
            fired(
                "(funcall (find-symbol (string-upcase op) :app) request)",
                Dialect::CommonLisp
            ),
            vec!["introspection-probe-unchecked"]
        );
        assert_eq!(
            fired(
                "(setf (symbol-function (intern (format nil \"~A-h\" k))) #'run)",
                Dialect::CommonLisp
            ),
            vec!["symbol-function-fset-dynamic-name"]
        );
        assert_eq!(
            fired(
                "(fset (intern (concat \"h-\" k)) #'run)",
                Dialect::EmacsLisp
            ),
            vec!["symbol-function-fset-dynamic-name"]
        );
        assert_eq!(
            fired("(apply (resolve sym) args)", Dialect::Clojure),
            vec!["introspection-probe-unchecked"]
        );
    }

    // -- the guard the report path cannot exercise ---------------------------

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without each `check`'s `is_unevaluated_at` call, every one of these
    /// fires.
    #[test]
    fn no_rule_fires_on_a_hard_quoted_form() {
        for source in [
            "'(intern \"HANDLER\" (find-package module))",
            "'(funcall (find-symbol name) request)",
            "'(setf (symbol-function (intern name)) #'run)",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    #[test]
    fn no_rule_fires_inside_a_long_hand_quote_form() {
        assert_eq!(
            fired(
                "(quote (setf (symbol-function (intern name)) #'run))",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    /// The archetype: a macro whose expansion contains every shape this
    /// package reports. All of it is template text.
    #[test]
    fn no_rule_fires_inside_a_quasiquoted_macro_template() {
        assert_eq!(
            fired(
                "(defmacro define-handler (name module)\n  \
                 `(progn\n     \
                 (setf (symbol-function (intern (format nil \"~A-handler\" ,name)))\n           \
                 (lambda (r) (funcall (find-symbol ,name) r)))\n     \
                 (intern \"REGISTERED\" (find-package ,module))))",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    /// A comma inside a hard quote is a literal comma, not an escape back to
    /// code — the shape a single depth counter reads wrongly.
    #[test]
    fn no_rule_fires_on_a_comma_inside_a_hard_quote() {
        assert_eq!(
            fired(
                "'(a ,(setf (symbol-function (intern name)) #'run))",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    /// The one shape that *is* code again.
    #[test]
    fn an_unquoted_form_inside_a_quasiquote_still_fires() {
        assert_eq!(
            fired(
                "`(a ,(setf (symbol-function (intern name)) #'run))",
                Dialect::CommonLisp
            ),
            vec!["symbol-function-fset-dynamic-name"]
        );
    }

    // -- the declarations a domain test cannot see ---------------------------

    /// `RuleDialectScope`: the dispatcher skips a rule before walking
    /// anything.
    #[test]
    fn no_rule_runs_outside_its_declared_dialects() {
        for dialect in [Dialect::Scheme, Dialect::Racket, Dialect::Fennel] {
            assert_eq!(
                fired(
                    "(intern \"X\" (find-package m))\n(funcall (find-symbol n) r)\n",
                    dialect
                ),
                Vec::<&str>::new(),
                "{dialect:?} is outside every rule's scope"
            );
        }
        // `intern-dynamic-package-target` is Common Lisp only: Emacs Lisp's
        // `intern` takes an obarray, not a package.
        assert_eq!(
            fired("(intern \"X\" (find-package m))", Dialect::EmacsLisp),
            Vec::<&str>::new()
        );
        // `symbol-function-fset-dynamic-name` does not model Clojure.
        assert_eq!(
            fired("(fset (intern name) f)", Dialect::Clojure),
            Vec::<&str>::new()
        );
    }

    /// `HeadFilter::Heads`: a file with none of this package's heads is never
    /// handed to any of its rules, which is what keeps the zero-finding
    /// benchmarks cheap.
    #[test]
    fn a_file_with_none_of_these_heads_trips_nothing() {
        assert_eq!(
            fired(
                "(defun add (a b) (+ a b))\n\
                 (defmethod draw ((s shape)) (render s))\n\
                 (defclass shape () ((size :accessor size-of)))\n\
                 (let ((x 1)) (dolist (y '(1 2 3)) (incf x y)))\n",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    // -- realistic correct code ---------------------------------------------

    /// A macro-heavy, correct Common Lisp file: the case a reviewer runs
    /// first, and this package's own risk area.
    #[test]
    fn a_realistic_macro_heavy_common_lisp_file_produces_no_findings() {
        assert_eq!(
            fired(REALISTIC_COMMON_LISP, Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn a_realistic_emacs_lisp_file_produces_no_findings() {
        assert_eq!(
            fired(REALISTIC_EMACS_LISP, Dialect::EmacsLisp),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn a_realistic_clojure_file_produces_no_findings() {
        assert_eq!(
            fired(REALISTIC_CLOJURE, Dialect::Clojure),
            Vec::<&str>::new()
        );
    }

    /// The dangerous twin. `a_realistic_*_file_produces_no_findings` and the
    /// corpus sweep both assert an *empty* result, which a harness that
    /// silently reports nothing would also satisfy. This is the same harness
    /// on a file built to trip all three rules: if it does not fire, the
    /// clean-file assertions above prove nothing.
    #[test]
    fn the_dangerous_twin_of_the_clean_files_fires_every_rule() {
        let twin = "(in-package :app)\n\n\
             (defun register (kind handler)\n  \
             (setf (symbol-function (intern (format nil \"~A-handler\" kind))) handler)\n  \
             (intern \"REGISTERED\" (find-package (string-upcase kind))))\n\n\
             (defun dispatch (op request)\n  \
             (funcall (find-symbol (string-upcase op) :app) request))\n";
        assert_eq!(
            fired(twin, Dialect::CommonLisp),
            vec![
                "intern-dynamic-package-target",
                "introspection-probe-unchecked",
                "symbol-function-fset-dynamic-name",
            ]
        );
    }

    // -- the repository's own fixtures ---------------------------------------

    /// Every Lisp fixture this repository ships, through the real engine.
    ///
    /// These files are hand-written, correct, and were not written with this
    /// package in mind, so a finding in one is a false positive by
    /// construction. Paired with `the_dangerous_twin_of_the_clean_files_fires_every_rule`,
    /// which proves this harness can still report.
    #[test]
    fn no_rule_fires_on_any_lisp_fixture_this_repository_ships() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..");
        let mut sources = Vec::new();
        for relative in ["tests/fixtures", "packages/feature/migrate/recipes"] {
            collect_lisp_files(&root.join(relative), &mut sources);
        }
        assert!(
            sources.len() >= 15,
            "found only {} fixture files; the sweep is not reaching them",
            sources.len()
        );

        for (path, dialect, source) in sources {
            // A fixture that does not parse is some other test's subject.
            let Ok(tree) = SyntaxTree::parse_with_dialect(&source, dialect) else {
                continue;
            };
            let catalog = RuleCatalog::new(&ENTRIES);
            let index = build_head_index(catalog);
            let names: Vec<&'static str> = collect_lint_outcomes(
                catalog,
                &index,
                &path,
                dialect,
                &tree,
                &source,
                RuleSelection::All,
            )
            .expect("lint pass")
            .into_iter()
            .map(|outcome| outcome.into_parts().0.rule)
            .collect();
            assert_eq!(
                names,
                Vec::<&str>::new(),
                "{} is correct hand-written code",
                path.display()
            );
        }
    }

    /// Every `.lisp`/`.el`/`.clj` under `directory`, with the dialect its
    /// extension implies.
    fn collect_lisp_files(
        directory: &Path,
        found: &mut Vec<(std::path::PathBuf, Dialect, String)>,
    ) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_lisp_files(&path, found);
                continue;
            }
            let dialect = match path.extension().and_then(|ext| ext.to_str()) {
                Some("lisp" | "asd") => Dialect::CommonLisp,
                Some("el") => Dialect::EmacsLisp,
                Some("clj" | "cljs" | "cljc") => Dialect::Clojure,
                _ => continue,
            };
            if let Ok(source) = std::fs::read_to_string(&path) {
                found.push((path, dialect, source));
            }
        }
    }

    // -- cost ----------------------------------------------------------------

    /// Each rule is dispatched exactly once per head match, and the count
    /// scales with the input rather than with the input squared.
    ///
    /// **Invocations, not wall clock.** A rule invoked more often than its
    /// heads occur is one whose head filter is not doing its job, and that is
    /// a property of the dispatch, not of how busy the machine is — the
    /// engine's own counter answers it deterministically.
    ///
    /// The doubling *ratio* that used to be asserted here lives in
    /// [`ignored_bench_doubling_ratio`] instead. Its docstring claimed
    /// "neither is a wall-clock threshold", which was wrong about itself: a
    /// ratio of two wall-clock measurements normalizes for machine speed but
    /// not for load *changing between* the two, which is exactly what happens
    /// on a shared box. It failed three CI runs at 4.30× and 9.24× on code
    /// that had not been touched, and passed 3/3 on re-run.
    #[test]
    fn each_rule_is_dispatched_once_per_head_match() {
        let small = measure(dense_source(400));
        let large = measure(dense_source(800));

        // Each definition in `dense_source` spells `intern` twice — once in
        // the `setf` place and once on its own — and `funcall` and `setf` once
        // each. So the head counts per definition are 2, 1, 1, in the
        // registration order of `ENTRIES`.
        assert_eq!(small.1, [800, 400, 400], "invocations must equal heads");
        assert_eq!(large.1, [1600, 800, 800]);

        // Doubling the definitions doubles the dispatches — no rule re-walks
        // the file per match, which is the shape the ratio was there to catch.
        for index in 0..small.1.len() {
            assert_eq!(
                large.1[index],
                small.1[index] * 2,
                "rule {index} did not scale linearly in dispatch count"
            );
        }
    }

    /// The wall-clock doubling ratio, as a benchmark rather than a gate.
    ///
    ///     cargo test -p paredit-feature-lint-introspection \
    ///       -- --ignored --nocapture ignored_bench_doubling_ratio
    ///
    /// A rule that re-derives the file on each match grows with the square of
    /// the input (ratio ≈ 4, and ≈ 3.7 in the two shipped rules that did it);
    /// a rule that works from the node it was handed grows linearly (≈ 2).
    /// Worth reading when a rule here changes — just not worth failing CI on.
    #[test]
    #[ignore = "a benchmark: wall-clock ratios are unstable under parallel load"]
    fn ignored_bench_doubling_ratio() {
        let small = measure(dense_source(400));
        let large = measure(dense_source(800));

        for (index, name) in [
            "intern-dynamic-package-target",
            "introspection-probe-unchecked",
            "symbol-function-fset-dynamic-name",
        ]
        .into_iter()
        .enumerate()
        {
            let ratio = large.0[index].as_secs_f64() / small.0[index].as_secs_f64().max(1e-9);
            eprintln!(
                "{name}: {:.0}µs @400 → {:.0}µs @800, ratio {ratio:.2}",
                small.0[index].as_secs_f64() * 1e6,
                large.0[index].as_secs_f64() * 1e6,
            );
        }
    }

    /// The number the `clean/forms/*` bench gate actually measures: what a
    /// rule costs on a file it finds *nothing* in.
    ///
    /// The heads are all there — `intern`, `setf`, `funcall` — so the
    /// dispatcher invokes every rule on every one of them; each then declines
    /// on a structural test and allocates nothing. `is_unevaluated_at`, the
    /// only part that touches the tree, is never reached.
    ///
    /// Asserted against the *same rules' own* cost on the finding-dense file
    /// rather than against a wall-clock constant, so the comparison is between
    /// two numbers measured on the same machine in the same run.
    #[test]
    fn a_file_with_no_findings_costs_a_fraction_of_a_file_full_of_them() {
        let clean = measure(clean_source(400));
        let dense = measure(dense_source(400));

        // Same head counts in both: the rules are invoked just as often and
        // simply find nothing.
        assert_eq!(clean.1, [800, 400, 400], "invocations must equal heads");

        for (index, name) in [
            "intern-dynamic-package-target",
            "introspection-probe-unchecked",
            "symbol-function-fset-dynamic-name",
        ]
        .into_iter()
        .enumerate()
        {
            eprintln!(
                "{name}: clean {:.0}µs vs dense {:.0}µs over {} invocations",
                clean.0[index].as_secs_f64() * 1e6,
                dense.0[index].as_secs_f64() * 1e6,
                clean.1[index],
            );
            assert!(
                clean.0[index] < dense.0[index],
                "{name} costs as much on a clean file as on one full of findings, so it is doing \
                 its expensive work before deciding it has nothing to report"
            );
        }
    }

    /// A file with `count` definitions spelling every head these rules anchor
    /// on, in the shapes that are *not* defects — so every rule is invoked as
    /// often as in [`dense_source`] and reports nothing.
    fn clean_source(count: usize) -> String {
        (0..count)
            .map(|index| {
                format!(
                    "(defun handler-{index} (request)\n  \
                     (setf (symbol-function 'run-{index}) #'run)\n  \
                     (intern \"REGISTERED\" :app)\n  \
                     (intern \"OTHER\" *package*)\n  \
                     (funcall #'run request))\n"
                )
            })
            .collect()
    }

    /// A file with `count` definitions, each carrying one match for each of
    /// the three rules.
    fn dense_source(count: usize) -> String {
        (0..count)
            .map(|index| {
                format!(
                    "(defun handler-{index} (op request)\n  \
                     (setf (symbol-function (intern (format nil \"~A-h\" op))) #'run)\n  \
                     (intern \"REGISTERED\" (find-package op))\n  \
                     (funcall (find-symbol (string-upcase op) :app) request))\n"
                )
            })
            .collect()
    }

    /// One measured pass: each rule's elapsed time and invocation count, in
    /// the registration order of `ENTRIES`.
    fn measure(source: String) -> ([std::time::Duration; 3], [u64; 3]) {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp).expect("parse");
        let outcome = collect_lint_pass(
            catalog,
            &index,
            Path::new("dense.lisp"),
            Dialect::CommonLisp,
            &tree,
            &source,
            RuleSelection::All,
            PassOptions {
                settings: None,
                measure: true,
            },
        )
        .expect("measured lint pass");
        let timings = outcome.timings.expect("measure: true yields timings");

        let mut elapsed = [std::time::Duration::ZERO; 3];
        let mut invocations = [0_u64; 3];
        for (position, spent, calls) in timings.entries() {
            elapsed[position] = spent;
            invocations[position] = calls;
        }
        (elapsed, invocations)
    }

    // -- the realistic files -------------------------------------------------

    /// Correct Common Lisp that uses every operator these rules anchor on, in
    /// the ways that are *not* defects: literal package designators, literal
    /// definition names, probes that are checked before use, and — the risk
    /// area — macros whose templates contain all three shapes.
    const REALISTIC_COMMON_LISP: &str = r#"(defpackage :app/registry
  (:use :cl)
  (:export #:register #:dispatch #:handler-for))

(in-package :app/registry)

(defvar *handlers* (make-hash-table :test #'equal))

;;; A literal package designator, in each of its four spellings.
(defun canonical-symbol (name)
  (list (intern name)
        (intern "CONSTANT" :app/registry)
        (intern "CONSTANT" "APP/REGISTRY")
        (intern "CONSTANT" (find-package :app/registry))
        (intern "CONSTANT" *package*)))

;;; A parameterized helper: the package is the caller's, not this call's.
(defun intern-into (name package)
  (intern name package))

;;; The accessor-generator idiom: the destination travels in with the symbol.
(defun setter-name (sym)
  (intern "SETTER" (symbol-package sym)))

(defun rename-into (sym)
  (intern "RENAMED" (package-name (symbol-package sym))))

;;; The ordinary function definition, with a name the source shows.
(setf (symbol-function 'legacy-dispatch) #'dispatch)
(setf (fdefinition 'legacy-register) #'register)
(setf (symbol-function (intern "BOOTSTRAP")) #'identity)

;;; Probing and then checking: the correct idiom, in each of its spellings.
(defun handler-for (op)
  (let ((found (find-symbol (string-upcase op) :app/registry)))
    (when (and found (fboundp found))
      (symbol-function found))))

(defun dispatch (op request)
  (let ((handler (find-symbol (string-upcase op) :app/registry)))
    (if handler
        (funcall handler request)
        (error "no handler for ~A" op))))

(defun dispatch-or-default (op request)
  (funcall (or (find-symbol (string-upcase op) :app/registry) #'reject)
           request))

(defun expand-once (form env)
  (let ((expander (macro-function (first form))))
    (check-type expander (or null function))
    (if expander
        (funcall expander form env)
        form)))

;;; The named probe, which is not a dynamically-named one.
(defun expand-when (form)
  (funcall (macro-function 'when) form nil))

;;; The risk area: macros whose expansions are exactly what these rules
;;; report. None of this is a call.
(defmacro define-handler (name &body body)
  `(progn
     (setf (symbol-function (intern (format nil "~A-HANDLER" ,name)))
           (lambda (request) ,@body))
     (intern "REGISTERED" (find-package (string-upcase ,name)))))

(defmacro with-dynamic-dispatch ((op) &body body)
  `(let ((handler (funcall (find-symbol (string-upcase ,op) :app/registry))))
     ,@body))

(defmacro define-accessor (slot)
  `(defun ,(intern (format nil "~A-OF" slot)) (object)
     (slot-value object ',slot)))

;;; Quoted data that happens to look like every shape above.
(defparameter *documented-shapes*
  '((setf (symbol-function (intern name)) #'run)
    (funcall (find-symbol name) request)
    (intern "X" (find-package module))))

(defparameter *documentation*
  "(setf (symbol-function (intern name)) #'run) is what this macro expands to.")

(defun register (kind handler)
  (setf (gethash kind *handlers*) handler))
"#;

    /// Correct Emacs Lisp: `fset`/`defalias` with literal names, `intern-soft`
    /// checked before use, and a macro template containing both.
    const REALISTIC_EMACS_LISP: &str = r#";;; app-registry.el --- a registry -*- lexical-binding: t -*-

(require 'cl-lib)

(defvar app-handlers (make-hash-table :test #'equal))

;;; Literal definition names.
(defalias 'app-legacy-dispatch #'app-dispatch)
(fset 'app-legacy-register #'app-register)
(defalias (intern "app-bootstrap") #'identity)

;;; Probing, then checking.
(defun app-handler-for (op)
  (let ((found (intern-soft (format "app-%s-handler" op))))
    (when (and found (fboundp found))
      (symbol-function found))))

(defun app-dispatch (op request)
  (let ((handler (intern-soft (format "app-%s-handler" op))))
    (if (and handler (fboundp handler))
        (funcall handler request)
        (error "No handler for %s" op))))

(defun app-dispatch-or-default (op request)
  (funcall (or (intern-soft (format "app-%s-handler" op)) #'app-reject) request))

;;; A named probe.
(defun app-expand-when (form)
  (funcall (intern-soft "when") form))

;;; The macro template, which is data.
(defmacro app-define-handler (name &rest body)
  `(progn
     (fset (intern (concat "app-" ,name "-handler"))
           (lambda (request) ,@body))
     (defalias (intern (format "%s-p" ,name)) #'always)))

(defvar app-documented-shapes
  '((fset (intern name) #'run)
    (funcall (intern-soft name) request)))

(defun app-register (kind handler)
  (puthash kind handler app-handlers))

(provide 'app-registry)
;;; app-registry.el ends here
"#;

    /// Correct Clojure: `resolve` checked before use, and `apply` over an
    /// ordinary function.
    const REALISTIC_CLOJURE: &str = r#"(ns app.registry
  (:require [clojure.string :as str]))

(def handlers (atom {}))

(defn handler-for [op]
  (when-let [v (resolve (symbol "app.handlers" (name op)))]
    (var-get v)))

(defn dispatch [op request]
  (if-let [f (resolve (symbol "app.handlers" (name op)))]
    (apply f [request])
    (throw (ex-info "no handler" {:op op}))))

(defn dispatch-or-default [op request]
  (apply (or (resolve (symbol "app.handlers" (name op))) identity) [request]))

;;; A named probe.
(defn expand-when [form]
  (apply (resolve 'clojure.core/when) form))

;;; An ordinary application.
(defn call-all [fs request]
  (doseq [f fs] (apply f [request])))

(defmacro define-handler [nm & body]
  `(do
     (defn ~(symbol (str (name nm) "-handler")) [request#] ~@body)
     (swap! handlers assoc ~(str nm) ~(symbol (str (name nm) "-handler")))))

(defn register [kind handler]
  (swap! handlers assoc kind handler))
"#;
}
