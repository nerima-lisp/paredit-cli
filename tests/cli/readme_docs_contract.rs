#[test]
fn readme_links_to_the_published_documentation() {
    let readme = std::fs::read_to_string("README.md").expect("read README");

    for required in [
        "docs/src/README.md",
        "https://nerima-lisp.github.io/paredit-cli/",
    ] {
        assert!(
            readme.contains(required),
            "README must link the documentation entry point: {required}"
        );
    }
}

#[test]
fn documentation_contains_command_reference() {
    let commands = std::fs::read_to_string("docs/src/commands.md").expect("read command reference");
    assert!(commands.contains("`paredit inspect`"));
    assert!(commands.contains("`paredit edit`"));
    assert!(commands.contains("`paredit refactor`"));
}
