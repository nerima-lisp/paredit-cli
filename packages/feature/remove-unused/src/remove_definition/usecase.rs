use paredit_core_cli::CliError;
use std::path::{Path as FsPath, PathBuf};

use crate::error::{RemoveRequestError, RemoveUnusedError, RemoveUnusedResult};

use crate::definition_report::domain::{DefinitionReportItem, collect_definition_forms};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, Edit, ExpressionKind, ExpressionView, Path, SyntaxTree,
};

#[derive(Debug)]
pub struct LoadedDefinitionSource {
    pub text: String,
    pub dialect: Dialect,
}

/// The outside world, as this use case needs it.
///
/// The methods used to return `anyhow::Result`, justified by the port's whole
/// purpose: the use case must not know what an adapter can fail with. That
/// reasoning is right and `anyhow::Error` was the wrong way to say it — it
/// does not express "some error I do not name", it expresses "no error type at
/// all", and it took the classification down with it.
///
/// An associated type says the intended thing in the type system: each adapter
/// names its own error, and the use case never learns which. The `Into<CliError>`
/// bound is the one thing it does require, because whatever an adapter fails
/// with has to be reportable at the CLI boundary with a code.
pub trait DefinitionSourcePort {
    /// What this adapter's own failures look like.
    type Error: Into<CliError>;

    fn load(&mut self, file: &FsPath) -> Result<LoadedDefinitionSource, Self::Error>;

    fn write(&mut self, file: &FsPath, content: &str) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub struct RemoveDefinitionRequest {
    pub file: PathBuf,
    pub path: Path,
    pub write: bool,
}

#[derive(Debug)]
pub struct RemoveDefinitionPlan {
    pub file: PathBuf,
    pub dialect: Dialect,
    pub path: Path,
    pub span: ByteSpan,
    pub definition: DefinitionReportItem,
    pub definition_text: String,
    pub rewritten: String,
    pub changed: bool,
    pub written: bool,
}

pub fn remove_definition(
    source: &mut impl DefinitionSourcePort,
    request: RemoveDefinitionRequest,
) -> RemoveUnusedResult<RemoveDefinitionPlan> {
    let loaded = source
        .load(&request.file)
        .map_err(|error| RemoveUnusedError::Source(Box::new(error.into())))?;
    let tree = SyntaxTree::parse_with_dialect(&loaded.text, loaded.dialect)?;

    let target_index = match request.path.indexes() {
        [index] => index.get(),
        _ => return Err(RemoveRequestError::NotATopLevelPath.into()),
    };
    if target_index >= tree.root_children().len() {
        return Err(RemoveRequestError::TopLevelPathOutOfRange {
            path: request.path.to_string(),
        }
        .into());
    }

    let selection = tree.select_path(&request.path)?;
    let view = selection.view();
    let span = selection.span();
    let Some(head) = list_head(&view) else {
        return Err(RemoveRequestError::NotAListDefinition.into());
    };
    if definition_shape(loaded.dialect, &view, head).is_none() {
        return Err(RemoveRequestError::NotADefinition {
            head: head.to_owned(),
        }
        .into());
    }

    let definition_text = selection.text().to_owned();
    let (_, definitions) = collect_definition_forms(&tree, loaded.dialect)?;
    let definition = definitions
        .into_iter()
        .find(|definition| definition.path == request.path.to_string())
        .expect("recognized definition must be present in the definition report");
    let rewritten = Edit::kill(&loaded.text, &tree, selection)?;

    SyntaxTree::parse_with_dialect(&rewritten, loaded.dialect).map_err(|source| {
        RemoveUnusedError::WouldBecomeInvalid {
            stage: "after",
            what: "definition",
            path: request.file.display().to_string(),
            source,
        }
    })?;

    let changed = rewritten != loaded.text;
    let written = request.write && changed;
    if written {
        source
            .write(&request.file, &rewritten)
            .map_err(|error| RemoveUnusedError::Source(Box::new(error.into())))?;
    }

    Ok(RemoveDefinitionPlan {
        file: request.file,
        dialect: loaded.dialect,
        path: request.path,
        span,
        definition,
        definition_text,
        rewritten,
        changed,
        written,
    })
}

fn list_head(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::List || view.delimiter != Some(Delimiter::Paren) {
        return None;
    }

    view.children.first().and_then(|child| {
        (child.kind == ExpressionKind::Atom)
            .then_some(child.text.as_deref())
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        DefinitionSourcePort, LoadedDefinitionSource, RemoveDefinitionRequest, remove_definition,
    };
    use paredit_core_cli::CliError;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::Path as ExpressionPath;

    struct MemorySource {
        text: String,
        writes: Vec<String>,
    }

    impl DefinitionSourcePort for MemorySource {
        type Error = CliError;

        fn load(&mut self, _file: &Path) -> Result<LoadedDefinitionSource, CliError> {
            Ok(LoadedDefinitionSource {
                text: self.text.clone(),
                dialect: Dialect::CommonLisp,
            })
        }

        fn write(&mut self, _file: &Path, content: &str) -> Result<(), CliError> {
            self.writes.push(content.to_owned());
            Ok(())
        }
    }

    #[test]
    fn plans_definition_removal_without_persisting() {
        let mut source = MemorySource {
            text: "(in-package :demo)\n(defun keep () 1)\n(defun remove-me (x) x)\n".to_owned(),
            writes: Vec::new(),
        };

        let plan = remove_definition(
            &mut source,
            RemoveDefinitionRequest {
                file: "example.lisp".into(),
                path: ExpressionPath::root_child(2),
                write: false,
            },
        )
        .unwrap();

        assert_eq!(plan.definition.name.as_deref(), Some("remove-me"));
        assert_eq!(plan.definition.package.as_deref(), Some(":demo"));
        assert_eq!(plan.definition_text, "(defun remove-me (x) x)");
        assert_eq!(plan.rewritten, "(in-package :demo)\n(defun keep () 1)\n");
        assert!(plan.changed);
        assert!(!plan.written);
        assert!(source.writes.is_empty());
    }

    #[test]
    fn persists_rewritten_source_when_requested() {
        let mut source = MemorySource {
            text: "(defun remove-me () 1)\n".to_owned(),
            writes: Vec::new(),
        };

        let plan = remove_definition(
            &mut source,
            RemoveDefinitionRequest {
                file: "example.lisp".into(),
                path: ExpressionPath::root_child(0),
                write: true,
            },
        )
        .unwrap();

        assert!(plan.written);
        assert_eq!(source.writes, vec![""]);
    }

    #[test]
    fn rejects_non_definition_top_level_form() {
        let mut source = MemorySource {
            text: "(print :hello)\n".to_owned(),
            writes: Vec::new(),
        };

        let error = remove_definition(
            &mut source,
            RemoveDefinitionRequest {
                file: "example.lisp".into(),
                path: ExpressionPath::root_child(0),
                write: false,
            },
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "selected top-level form is not recognized as a definition: print"
        );
    }
}
