//! `--graph`, on the three reports whose answer is a graph.
//!
//! Each of these already computed nodes and edges and could only print them as
//! a flat list, which is the one shape the structure is invisible in. The
//! assertions below are about the *drawing decisions* — what is grouped, what
//! is drawn open, what carries a label — rather than about the syntax, which
//! `paredit-core-cli`'s unit tests cover.

use super::*;

const CLASSES: &str = "(defpackage :app (:use :cl :alexandria))\n\
     (in-package :app)\n\
     (defclass base () ((id :initform 0)))\n\
     (defclass mixin () ((tag)))\n\
     (defclass widget (base mixin) ((id :initform 1)))\n\
     (defclass leaf (widget unknown-parent) ())\n";

const CALLS: &str = "(defun helper (x) (* x 2))\n\
     (defun main (y)\n\
     \x20 (helper y)\n\
     \x20 (helper (helper y))\n\
     \x20 (format t \"~a\" y))\n";

fn fixture(name: &str, source: &str) -> PathBuf {
    let dir = fresh_temp_dir(name);
    let file = dir.join("core.lisp");
    fs::write(&file, source).expect("write lisp fixture");
    file
}

fn drawing(command: &str, format: &str, file: &PathBuf, extra: &[&str]) -> String {
    let mut paredit = paredit();
    paredit.args(["inspect", command, "--graph", format]);
    paredit.args(extra);
    let assert = paredit.arg(file).assert().success();
    String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 stdout")
}

#[test]
fn cli_call_graph_dot_groups_definitions_by_their_file() {
    let file = fixture("graph-call-dot", CALLS);
    let dot = drawing("call-graph", "dot", &file, &["--include-external"]);

    assert!(dot.starts_with("digraph paredit {"), "{dot}");
    // `cluster_` is what makes Graphviz draw the box; a plain subgraph is
    // invisible, so the file grouping would silently vanish.
    assert!(dot.contains("subgraph cluster_0"), "{dot}");
    assert!(dot.contains("label=\"helper\", shape=box"), "{dot}");
}

/// Repeated calls collapse into one labelled arrow.
///
/// `main` calls `helper` three times. Three identical arrows say nothing the
/// label does not, and in a real codebase they make the picture unreadable.
#[test]
fn cli_call_graph_collapses_parallel_edges_into_a_count() {
    let file = fixture("graph-call-parallel", CALLS);
    let mermaid = drawing("call-graph", "mermaid", &file, &["--include-external"]);

    assert!(mermaid.starts_with("flowchart LR"), "{mermaid}");
    assert!(mermaid.contains("-- ×3 -->"), "{mermaid}");
    assert_eq!(
        mermaid.matches("-- ×3 -->").count(),
        1,
        "one arrow, not three: {mermaid}"
    );
}

/// A callee with no definition in the scanned set is drawn open and dashed.
///
/// The edge is real — the call site exists — but its far end was never
/// verified, and a picture that draws it the same as a resolved call asserts
/// something the analysis did not establish.
#[test]
fn cli_call_graph_draws_an_unresolved_callee_as_provisional() {
    let file = fixture("graph-call-external", CALLS);
    let dot = drawing("call-graph", "dot", &file, &["--include-external"]);
    assert!(
        dot.contains("label=\"format\", shape=ellipse, style=dashed"),
        "{dot}"
    );

    let mermaid = drawing("call-graph", "mermaid", &file, &["--include-external"]);
    assert!(mermaid.contains("([\"format\"])"), "{mermaid}");
    assert!(mermaid.contains("-.->"), "{mermaid}");
}

/// Superclass order is the class precedence list, so the drawing keeps it.
///
/// `widget` inherits from `base` then `mixin`, and which one wins a slot
/// conflict follows from that order. Two unlabelled arrows would lose it.
#[test]
fn cli_class_hierarchy_labels_superclasses_with_their_precedence_order() {
    let file = fixture("graph-classes-order", CLASSES);
    let mermaid = drawing("class-hierarchy", "mermaid", &file, &[]);

    assert!(mermaid.contains("-- 1 -->"), "{mermaid}");
    assert!(mermaid.contains("-- 2 -->"), "{mermaid}");
    assert!(mermaid.contains("[\"BASE\"]"), "{mermaid}");
}

#[test]
fn cli_class_hierarchy_marks_a_shadowing_class_and_an_undefined_parent() {
    let file = fixture("graph-classes-shadow", CLASSES);
    let mermaid = drawing("class-hierarchy", "mermaid", &file, &[]);

    // `widget` redeclares `base`'s `id` slot: the report's actionable finding,
    // kept visible in the picture.
    assert!(mermaid.contains("[/\"WIDGET\"/]"), "{mermaid}");
    // No scanned file defines `unknown-parent`, so its slots were never
    // attributed.
    assert!(mermaid.contains("([\"UNKNOWN-PARENT\"])"), "{mermaid}");
}

#[test]
fn cli_dependencies_draws_the_declaring_package_reaching_out() {
    let file = fixture("graph-deps", CLASSES);
    let dot = drawing("dependencies", "dot", &file, &[]);

    assert!(dot.contains("label=\":app\", shape=box"), "{dot}");
    assert!(dot.contains("label=\":cl\", shape=ellipse"), "{dot}");
    assert!(
        dot.contains("[label=\"defpackage-use\", style=dashed]"),
        "{dot}"
    );
}

/// A Lisp symbol is mostly punctuation, and Mermaid identifiers may not be.
///
/// Generated identifiers are what make that a non-problem; this asserts the
/// generated ones are what reach the output, and that a quote in a name cannot
/// close its own label.
#[test]
fn cli_graph_identifiers_survive_punctuation_heavy_symbols() {
    let file = fixture(
        "graph-punctuation",
        "(defun *weird-name* (x) (funcall #'|odd \"name\"| x))\n",
    );
    let mermaid = drawing("call-graph", "mermaid", &file, &["--include-external"]);

    assert!(mermaid.contains("[\"*weird-name*\"]"), "{mermaid}");
    for line in mermaid.lines().skip(1) {
        let identifier = line
            .trim()
            .split(['[', '(', ' '])
            .next()
            .unwrap_or_default();
        if identifier.is_empty() || identifier == "end" || identifier.starts_with("subgraph") {
            continue;
        }
        assert!(
            identifier
                .chars()
                .all(|character| character.is_ascii_alphanumeric()),
            "identifier {identifier:?} is not a bare Mermaid id: {mermaid}"
        );
    }
}

/// `--graph` selects a different view, not a different encoding, and the
/// catalog has to say so — otherwise an agent reads `--output` and `--graph` as
/// two spellings of one choice.
#[test]
fn cli_capabilities_reports_graph_as_its_own_option() {
    let assert = paredit()
        .args(["inspect", "capabilities", "--output", "json"])
        .assert()
        .success();
    let catalog: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout).expect("catalog parses");

    let inspect = catalog["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .find(|command| command["name"] == "inspect")
        .expect("the inspect namespace");

    for leaf in ["call-graph", "dependencies", "class-hierarchy"] {
        let args = inspect["commands"]
            .as_array()
            .expect("leaves")
            .iter()
            .find(|command| command["name"] == leaf)
            .unwrap_or_else(|| panic!("no `inspect {leaf}` in the catalog"))["args"]
            .as_array()
            .expect("args");
        let graph = args
            .iter()
            .find(|arg| arg["id"] == "graph")
            .unwrap_or_else(|| panic!("`inspect {leaf}` has no --graph"));
        assert_eq!(
            graph["possible_values"],
            serde_json::json!(["dot", "mermaid"]),
            "for inspect {leaf}"
        );
        assert!(
            graph["default_values"].as_array().is_none_or(Vec::is_empty),
            "--graph must be opt-in for inspect {leaf}"
        );
    }
}

/// The gate is about the analysis, not about the rendering, so drawing the
/// graph must not turn a failing run green.
#[test]
fn cli_graph_output_still_honours_the_gate() {
    let file = fixture("graph-gate", CLASSES);
    paredit()
        .args([
            "inspect",
            "class-hierarchy",
            "--graph",
            "dot",
            "--fail-on-shadowed-slot",
        ])
        .arg(&file)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("digraph paredit"));
}
