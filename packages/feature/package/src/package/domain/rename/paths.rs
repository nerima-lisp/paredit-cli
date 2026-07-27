use crate::domain::sexpr::Path;

pub(super) fn child_path(parent: &Path, child: usize) -> Path {
    parent.child(child)
}

pub(super) fn option_child_path(parent: &Path, option: usize, child: usize) -> Path {
    parent.descendant([option, child])
}

pub(super) fn local_nickname_package_path(
    parent: &Path,
    option: usize,
    pair: usize,
    child: usize,
) -> Path {
    parent.descendant([option, pair, child])
}
