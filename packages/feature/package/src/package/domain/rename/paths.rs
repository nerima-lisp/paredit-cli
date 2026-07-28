use paredit_core_syntax::sexpr::Path;

pub fn child_path(parent: &Path, child: usize) -> Path {
    parent.child(child)
}

pub fn option_child_path(parent: &Path, option: usize, child: usize) -> Path {
    parent.descendant([option, child])
}

pub fn local_nickname_package_path(
    parent: &Path,
    option: usize,
    pair: usize,
    child: usize,
) -> Path {
    parent.descendant([option, pair, child])
}
