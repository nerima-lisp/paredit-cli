use std::path::{Path as FsPath, PathBuf};

use anyhow::{Context, Result};

use crate::domain::definition::definition_shape;
use crate::domain::definition_report::{DefinitionReportItem, collect_definition_forms};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{
    ByteSpan, Delimiter, Edit, ExpressionKind, ExpressionView, Path, SyntaxTree,
};

#[derive(Debug)]
pub struct LoadedDefinitionSource {
    pub text: String,
    pub dialect: Dialect,
}

pub trait DefinitionSourcePort {
    fn load(&mut self, file: &FsPath) -> Result<LoadedDefinitionSource>;

    fn write(&mut self, file: &FsPath, content: &str) -> Result<()>;
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
) -> Result<RemoveDefinitionPlan> {
    let loaded = source.load(&request.file)?;
    let tree = SyntaxTree::parse_with_dialect(&loaded.text, loaded.dialect)?;

    let target_index = match request.path.indexes() {
        [index] => index.get(),
        _ => anyhow::bail!(
            "remove-definition requires a top-level definition path, for example --path 2"
        ),
    };
    if target_index >= tree.root_children().len() {
        anyhow::bail!("top-level path {} is out of range", request.path);
    }

    let selection = tree.select_path(&request.path)?;
    let view = selection.view();
    let span = selection.span();
    let Some(head) = list_head(&view) else {
        anyhow::bail!("selected top-level form is not a list definition");
    };
    if definition_shape(loaded.dialect, &view, head).is_none() {
        anyhow::bail!("selected top-level form is not recognized as a definition: {head}");
    }

    let definition_text = selection.text().to_owned();
    let (_, definitions) = collect_definition_forms(&tree, loaded.dialect)?;
    let definition = definitions
        .into_iter()
        .find(|definition| definition.path == request.path.to_string())
        .expect("recognized definition must be present in the definition report");
    let rewritten = Edit::kill(&loaded.text, &tree, selection)?;

    SyntaxTree::parse_with_dialect(&rewritten, loaded.dialect).with_context(|| {
        format!(
            "file would become invalid after removing definition: {}",
            request.file.display()
        )
    })?;

    let changed = rewritten != loaded.text;
    let written = request.write && changed;
    if written {
        source.write(&request.file, &rewritten)?;
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

    use anyhow::Result;

    use super::{
        DefinitionSourcePort, LoadedDefinitionSource, RemoveDefinitionRequest, remove_definition,
    };
    use crate::domain::dialect::Dialect;
    use crate::domain::sexpr::Path as ExpressionPath;

    struct MemorySource {
        text: String,
        writes: Vec<String>,
    }

    impl DefinitionSourcePort for MemorySource {
        fn load(&mut self, _file: &Path) -> Result<LoadedDefinitionSource> {
            Ok(LoadedDefinitionSource {
                text: self.text.clone(),
                dialect: Dialect::CommonLisp,
            })
        }

        fn write(&mut self, _file: &Path, content: &str) -> Result<()> {
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
