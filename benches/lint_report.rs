use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use paredit_cli::lint::report::collect_lint_findings;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::SyntaxTree;

/// The two ends of the range, and deliberately nothing between them.
///
/// Every entry here is a benchmark, and `scripts/bench-compare.sh` pays for it
/// twice — once per revision — at roughly four seconds each. A middle point
/// earns that only if a regression could hide at 256 forms while leaving both
/// 64 and 1024 alone, and none can: a constant-factor regression moves every
/// size, and a complexity regression moves the large one hardest. 256 was
/// interpolation between two points already measured.
const FORM_COUNTS: [usize; 2] = [64, 1024];

/// One triggering form per rule, lifted verbatim from
/// `tests/fixtures/lint_golden/broad.lisp` (the golden fixture that pins one
/// form per `inspect lint` rule, all 134 of them). Cycling through this list
/// keeps every synthetic document dense with findings regardless of size.
const DENSE_FORMS: &[&str] = &[
    "(list '5)",
    "(progn only)",
    "(progn a (progn b c))",
    "(when q (progn s t))",
    "(let () (ela) (elb))",
    "(if c d nil)",
    "(funcall #'g m)",
    "(the t whatever)",
    "(funcall (lambda (fx) fx) 9)",
    "(mapcar #'(lambda (sq) sq) sqs)",
    "(identity h)",
    "(cons e nil)",
    "(reverse (reverse dr))",
    "(append (list al) ar)",
    "(format nil \"~A\" fs)",
    "(format t \"~%\")",
    "(floor fq 1)",
    "(- 0 amt)",
    "(list* la lb)",
    "(values-list (list va vb))",
    "(prog1 (p1x))",
    "(subseq sz 0)",
    "(car (nthcdr cn cx))",
    "(car (reverse crx))",
    "(append anx nil)",
    "(multiple-value-list (values mva mvb))",
    "(typep tpx 'string)",
    "(coerce ctx t)",
    "(gethash gdk gdh nil)",
    "(make-hash-table :test 'eql)",
    "(let* ((a 1)) a)",
    "(cond (ok (run)))",
    "(cond (t (r1) (r2)))",
    "(incf tally 1)",
    "(incf nsd -3)",
    "(return-from blk nil)",
    "(multiple-value-bind (mv) (vals) mv)",
    "(or za (or pb qc))",
    "(when wa (when wb (wc)))",
    "(unless ua (unless ub (uc)))",
    "(and x)",
    "(append solo)",
    "(* x)",
    "(when (not r) y)",
    "(if p q)",
    "(setf ctr (1+ ctr))",
    "(setf lst (cons e lst))",
    "(setf st (adjoin e st))",
    "(car (cdr z))",
    "(nth 0 zs)",
    "(nthcdr 0 nz)",
    "(nthcdr 2 ns)",
    "(apply #'g (list m))",
    "(find ret lst :test #'eql)",
    "(find rsz lst :start 0)",
    "(find ren lst :end nil)",
    "(find rfe lst :from-end nil)",
    "(remove rcn lst :count nil)",
    "(string= (string-downcase sa) (string-downcase sb))",
    "(char= (char-downcase ca) (char-downcase cb))",
    "(string-upcase (string-downcase nsc))",
    "(code-char (char-code ccc))",
    "(last ldc 1)",
    "(butlast bdc 1)",
    "(make-list mde :initial-element nil)",
    "(parse-integer pir :radix 10)",
    "(getf gdn :key nil)",
    "(make-array madk :adjustable nil)",
    "(char-upcase (char-downcase ncc))",
    "(list* lsn1 lsn2 nil)",
    "(sort rik #'< :key #'identity)",
    "(= tally 0)",
    "(not (< a b))",
    "(if (not c) a b)",
    "(if iv iv jv)",
    "(if iw nil t)",
    "(if iu nil (iue))",
    "(prog2 (p2a) (p2b))",
    "(handler-case (hcx))",
    "(unwind-protect (upx))",
    "(+ osa 1)",
    "(if t on off)",
    "(when t (bd))",
    "(and p t q)",
    "(and (not p) (not q))",
    "(equal w nil)",
    "(eq n 7)",
    "(eq c #\\a)",
    "(if a b c d)",
    "(setq x x)",
    "(defun reset () (setf total 0 total 1))",
    "(setf (slot a) 1 (slot b))",
    "(setq (car x) 5)",
    "(incf counter 1 2)",
    "(defun scale (factor x factor) (* factor x))",
    "(defun dupkw (&optional x &optional y) x)",
    "(defun kworder (&key a &optional b) a)",
    "(when (eq status status) 1)",
    "(defun ready? (x) (eq (compute x) t))",
    "(if test (foo x) (foo x))",
    "(the fixnum)",
    "(eq x)",
    "(gethash key)",
    "(case x (:a 1) (:b 2) (:a 3))",
    "(case sym ('apple :fruit) ('carrot :veg) (t :x))",
    "(case x (nil 1) (t 2))",
    "(typecase x (nil 1) (t 2))",
    "(case x (1 :one) 2 :two)",
    "(case x (1 :one) (t :def) (2 :two))",
    "(ecase x (1 :one) (t :default))",
    "(cond ((foo) 1) ((bar) 2) ((foo) 3))",
    "(cond ((foo) 1) (t 2) ((bar) 3))",
    "(cond ((plusp x) :pos) ((minusp x) :neg) :zero)",
    "(let ((x 1) (y 2) (x 3)) x)",
    "(let ((x 1 y 2)) (+ x y))",
    "(let ((nil 1) (:status :ok)) :status)",
    "(dolist (x) (print x))",
    "(eval-when (:compile-toplevel :executee) 1)",
    "(or x y x)",
    "(and (ready a) nil (finalize b))",
    "(when (eql name \"root\") 1)",
    "(when (eql p '(:a :b)) 1)",
    "(defun eqlsearch (items) (member \"x\" items))",
    "(defun singlearg (x) (when (< x) (go)))",
    "(defun missingdest (x) (format \"~a~%\" x))",
    "(defun litplace () (incf 5))",
    "(defun destrlit () (nreverse '(a b c)))",
    "(defun charopstr (c) (when (char= c \"a\") :hit))",
    "(defun emptybody (ready) (when ready))",
    "(defun identarith (x) (+ x 0))",
    "(/ x 0)",
    "(make-instance 'c :x 1 :x 2)",
    "(defpackage :app (:export 'foo))",
    "(incf counter 0)",
];

#[derive(Clone, Copy)]
enum Shape {
    /// Cycles through [`DENSE_FORMS`] so every rule fires repeatedly: the
    /// worst case for a lint pass, and the shape a shared operator index
    /// should speed up by skipping rules whose heads never occur.
    Dense,
    /// Idiomatic, rule-clean code: the common real-world case, where today
    /// every one of the 134 rules still walks the whole document only to
    /// find nothing.
    Clean,
}

impl Shape {
    const fn label(self) -> &'static str {
        match self {
            Self::Dense => "dense-findings",
            Self::Clean => "clean",
        }
    }

    fn source(self, form_count: usize) -> String {
        match self {
            Self::Dense => (0..form_count)
                .map(|index| DENSE_FORMS[index % DENSE_FORMS.len()])
                .collect::<Vec<_>>()
                .join("\n"),
            // The docstring is load-bearing. `collect_lint_findings` runs
            // *every* registered rule, not the `recommended` preset, so the
            // pedantic `missing-docstring` fired on every form here — which
            // made this fixture stop being clean, and `cargo bench` fail
            // outright, from the moment that rule was added. Nothing noticed
            // because the benchmarks were not run by CI; they are now.
            Self::Clean => (0..form_count)
                .map(|index| {
                    format!(
                        "(defun clean-fn-{index} (a b)\n  \"Return A plus twice B.\"\n  (+ a (* b 2)))"
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

fn bench_path() -> PathBuf {
    PathBuf::from("bench.lisp")
}

fn benchmark_lint_report(c: &mut Criterion) {
    let path = bench_path();

    for shape in [Shape::Dense, Shape::Clean] {
        let mut group = c.benchmark_group(shape.label());
        for form_count in FORM_COUNTS {
            let source = shape.source(form_count);
            let tree = SyntaxTree::parse_with_dialect(&source, Dialect::CommonLisp)
                .expect("bench fixture parses");

            // A regression that stops finding anything must fail the
            // benchmark, not silently look like a speedup.
            let findings = collect_lint_findings(&path, Dialect::CommonLisp, &tree)
                .expect("bench fixture lints");
            match shape {
                Shape::Dense => assert!(
                    !findings.is_empty(),
                    "dense-findings fixture at {form_count} forms produced no findings"
                ),
                Shape::Clean => assert!(
                    findings.is_empty(),
                    "clean fixture at {form_count} forms produced {} findings",
                    findings.len()
                ),
            }

            group.throughput(Throughput::Elements(form_count as u64));
            group.bench_with_input(
                BenchmarkId::new("forms", form_count),
                &tree,
                |bencher, tree| {
                    bencher.iter(|| {
                        black_box(
                            collect_lint_findings(&path, Dialect::CommonLisp, tree)
                                .expect("bench fixture lints"),
                        )
                    });
                },
            );
        }
        group.finish();
    }
}

fn criterion_config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .without_plots()
}

criterion_group! {
    name = benches;
    config = criterion_config();
    targets = benchmark_lint_report
}
criterion_main!(benches);
