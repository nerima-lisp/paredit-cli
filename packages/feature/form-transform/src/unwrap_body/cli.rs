use super::usecase::{UnwrapBodyPlan, UnwrapBodyRequest, plan_unwrap_body};
use clap::Args;
use paredit_core_cli::CliResult;
use paredit_core_cli::args::{CompactSelectorArgs, DialectArg, OutputFormat};
use paredit_core_cli::safe_text;
use paredit_core_cli::shared::{
    ExpectedWriteTarget, read_input_dialect_and_tree, read_input_dialect_tree_and_expected_target,
    resolve_compact_target, write_files_with_rollback_expected,
};
use paredit_core_syntax::sexpr::SymbolName;
use serde_json::json;
use std::path::PathBuf;

/// Remove a wrapper and retain its body forms. Structural only: lexical scope and evaluation semantics are not preserved.
#[derive(Debug, Args)]
pub struct UnwrapBodyArgs {
    /// Input file. Required with --write; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[command(flatten)]
    selector: CompactSelectorArgs,
    /// Optional guard requiring the selected wrapper's function head.
    #[arg(long)]
    function: Option<SymbolName>,
    /// First argument to retain, zero-based excluding the head. All following arguments are retained.
    #[arg(long)]
    body_start: usize,
    /// Rewrite the input file in place; otherwise print a preview.
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

struct PreparedUnwrapBody {
    plan: UnwrapBodyPlan,
    write_target: Option<(PathBuf, ExpectedWriteTarget)>,
}

impl PreparedUnwrapBody {
    fn write(&self) -> CliResult<()> {
        let (file, expected) = self
            .write_target
            .as_ref()
            .ok_or(paredit_core_cli::ArgumentError::WriteRequiresFile)?;
        if self.plan.changed {
            write_files_with_rollback_expected([(
                file.clone(),
                self.plan.rewritten.clone(),
                *expected,
            )])?;
        }
        Ok(())
    }
}

fn prepare_unwrap_body(args: &UnwrapBodyArgs) -> CliResult<PreparedUnwrapBody> {
    let (input, dialect, tree, write_target) = match &args.file {
        Some(file) => {
            let (input, dialect, tree, expected) =
                read_input_dialect_tree_and_expected_target(file.clone(), args.dialect)?;
            (input, dialect, tree, Some((file.clone(), expected)))
        }
        None => {
            let (input, dialect, tree) = read_input_dialect_and_tree(None, args.dialect)?;
            (input, dialect, tree, None)
        }
    };
    let target = resolve_compact_target(&tree, dialect, &args.selector, "refactor unwrap-body")?;
    let plan = plan_unwrap_body(UnwrapBodyRequest {
        input: &input.text,
        dialect,
        path: target.path,
        expected_function: args.function.clone(),
        body_start: args.body_start,
    })?;
    Ok(PreparedUnwrapBody { plan, write_target })
}

pub fn unwrap_body(args: UnwrapBodyArgs) -> CliResult<()> {
    let prepared = prepare_unwrap_body(&args)?;
    if args.write {
        prepared.write()?;
    }
    let plan = prepared.plan;
    let written = args.write;
    match args.output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("function\t{}", safe_text!(plan.function));
            println!("body_start\t{}", plan.body_start);
            println!("body_count\t{}", plan.body_count);
            println!("semantics\tstructural-only");
            println!("replacement\t{}", safe_text!(plan.replacement));
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1, "dialect": plan.dialect.label(), "path": plan.path.to_string(),
                "function": plan.function.as_str(), "semantics": "structural-only",
                "span": { "start": plan.span.start().get(), "end": plan.span.end().get() },
                "bodyStart": plan.body_start, "bodyCount": plan.body_count,
                "bodySpan": { "start": plan.body_span.start().get(), "end": plan.body_span.end().get() },
                "callArgumentCount": plan.call_argument_count, "replacement": plan.replacement,
                "changed": plan.changed, "written": written, "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_cli::{CliError, IoRefusal};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const ORIGINAL: &str = "(wrap config (a) (b))\n";
    const REWRITTEN: &str = "(a) (b)\n";
    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    struct TestFile {
        directory: PathBuf,
        path: PathBuf,
    }

    impl TestFile {
        fn new() -> Self {
            let parent = std::env::temp_dir()
                .canonicalize()
                .expect("resolve temp directory");
            let directory = loop {
                let directory = parent.join(format!(
                    "paredit-unwrap-body-{}-{}",
                    std::process::id(),
                    NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&directory) {
                    Ok(()) => break directory,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create test directory: {error}"),
                }
            };
            let path = directory.join("wrapper.lisp");
            let file = Self { directory, path };
            fs::write(&file.path, ORIGINAL).expect("write input");
            file
        }

        fn prepare(&self) -> PreparedUnwrapBody {
            let prepared = prepare_unwrap_body(&UnwrapBodyArgs {
                file: Some(self.path.clone()),
                dialect: None,
                selector: CompactSelectorArgs {
                    path: Some("0".parse().expect("parse path")),
                    at: None,
                    select: None,
                },
                function: None,
                body_start: 1,
                write: true,
                output: OutputFormat::Json,
            })
            .expect("prepare unwrap");
            assert!(prepared.plan.changed);
            assert_eq!(prepared.plan.rewritten, REWRITTEN);
            prepared
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn write_rejects_content_changed_after_read_and_preserves_new_bytes() {
        let file = TestFile::new();
        let prepared = file.prepare();
        let newer = "(wrap config (newer))\n";
        fs::write(&file.path, newer).expect("change input after read");

        let error = prepared.write().expect_err("reject changed input");

        assert!(matches!(
            error,
            CliError::Refused(IoRefusal::TargetChangedSinceParsing { .. })
        ));
        assert_eq!(
            fs::read(&file.path).expect("read newer input"),
            newer.as_bytes()
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_rejects_same_content_replacement_after_read_and_preserves_both_files() {
        let file = TestFile::new();
        let prepared = file.prepare();
        let preserved = file.directory.join("parsed-wrapper.lisp");
        fs::rename(&file.path, &preserved).expect("retain parsed inode");
        fs::write(&file.path, ORIGINAL).expect("replace input after read");

        let error = prepared.write().expect_err("reject replacement input");

        assert!(matches!(
            error,
            CliError::Refused(IoRefusal::TargetReplacedSinceParsing { .. })
        ));
        assert_eq!(
            fs::read(&file.path).expect("read replacement input"),
            ORIGINAL.as_bytes()
        );
        assert_eq!(
            fs::read(&preserved).expect("read parsed input"),
            ORIGINAL.as_bytes()
        );
    }

    #[test]
    fn write_rejects_deletion_after_read_without_recreating_file() {
        let file = TestFile::new();
        let prepared = file.prepare();
        fs::remove_file(&file.path).expect("delete input after read");

        let error = prepared.write().expect_err("reject deleted input");

        assert!(matches!(
            error,
            CliError::Refused(IoRefusal::TargetRemovedSinceParsing { .. })
        ));
        assert_eq!(
            fs::symlink_metadata(&file.path)
                .expect_err("input must remain absent")
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[test]
    fn write_accepts_unchanged_target() {
        let file = TestFile::new();
        let prepared = file.prepare();

        prepared.write().expect("write unchanged input");

        assert_eq!(
            fs::read(&file.path).expect("read rewritten input"),
            REWRITTEN.as_bytes()
        );
    }
}
