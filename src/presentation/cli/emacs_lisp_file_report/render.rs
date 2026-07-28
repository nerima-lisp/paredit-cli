use anyhow::Result;
use serde_json::json;

use crate::application::usecase::emacs_lisp_file_report::{
    EmacsLispFileFacts, EmacsLispFilePolicy,
};
use crate::presentation::cli::OutputFormat;

pub(super) fn print_emacs_lisp_file_report(
    files: &[EmacsLispFileFacts],
    policy: &EmacsLispFilePolicy,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("file_count\t{}", files.len());
            if policy.fail_on_missing_lexical_binding {
                println!(
                    "policy\tfail_on_missing_lexical_binding=true\tpassed={}",
                    policy.passed
                );
            }
            for file in files {
                println!(
                    "file\t{}\tlexical_binding={}\tprovides={}\tdefinitions={}",
                    safe_text!(file.path.display()),
                    file.lexical_binding.as_str(),
                    safe_text!(file.provides.as_deref().unwrap_or("-")),
                    file.definition_count,
                );
                for feature in &file.features {
                    println!(
                        "feature\t{}\t{}\t{}\teager={}",
                        safe_text!(file.path.display()),
                        safe_text!(feature.form),
                        safe_text!(feature.designator),
                        feature.eager,
                    );
                }
                for autoload in &file.autoloads {
                    println!(
                        "autoload\t{}\t{}\tdefinition={}\tinline_form={}",
                        safe_text!(file.path.display()),
                        autoload.span.start().get(),
                        safe_text!(autoload.definition.as_deref().unwrap_or("-")),
                        autoload.inline_form,
                    );
                }
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": 1,
                    "file_count": files.len(),
                    "policy": {
                        "fail_on_missing_lexical_binding":
                            policy.fail_on_missing_lexical_binding,
                        "passed": policy.passed,
                        "violations": &policy.violations,
                    },
                    "files": files
                        .iter()
                        .map(|file| json!({
                            "path": file.path.display().to_string(),
                            "lexical_binding": file.lexical_binding.as_str(),
                            "provides": &file.provides,
                            "definition_count": file.definition_count,
                            "features": file.features
                                .iter()
                                .map(|feature| json!({
                                    "form": &feature.form,
                                    "designator": &feature.designator,
                                    "eager": feature.eager,
                                    "span": {
                                        "start": feature.span.start().get(),
                                        "end": feature.span.end().get(),
                                    },
                                }))
                                .collect::<Vec<_>>(),
                            "autoloads": file.autoloads
                                .iter()
                                .map(|autoload| json!({
                                    "definition": &autoload.definition,
                                    "prefix": &autoload.prefix,
                                    "inline_form": autoload.inline_form,
                                    "span": {
                                        "start": autoload.span.start().get(),
                                        "end": autoload.span.end().get(),
                                    },
                                }))
                                .collect::<Vec<_>>(),
                        }))
                        .collect::<Vec<_>>(),
                }))?
            );
        }
    }
    Ok(())
}
