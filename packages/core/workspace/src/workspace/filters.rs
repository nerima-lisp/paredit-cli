use std::path::Path;

pub(super) fn is_hidden_workspace_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

pub(super) fn is_generated_workspace_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".git"
                    | ".direnv"
                    | ".devenv"
                    | "build"
                    | "coverage"
                    | "dist"
                    | "node_modules"
                    | "result"
                    | "target"
                    | "vendor"
            )
        })
}
