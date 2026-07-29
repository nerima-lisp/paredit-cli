//! The manual wiring steps a generator deliberately does not perform.
//!
//! `INTROSPECTION_COMMANDS`/`FORMAT_COMMANDS`/`STRUCTURAL_COMMANDS`/
//! `SEMANTIC_COMMANDS` in `src/presentation/cli/contract.rs`, and the eight
//! matching assertions in `tests/cli/dialect_contract.rs`, are pinned counts:
//! a command added without updating them is a compile-time or test-time
//! failure, on purpose (see `docs/src/architecture.md`). A script that edits
//! them blind risks the exact failure this project's own memory already
//! recorded once — a scripted insert silently double-commas `command.rs` or
//! duplicates a `contract.rs` entry. So this prints the current numbers,
//! freshly read, and lets a human make the one-line edits with eyes on them.

use std::fs;

use crate::error::Result;

use crate::repo::Repo;

#[derive(Debug, Clone, Copy)]
pub enum Namespace {
    Inspect,
    Edit,
    Refactor,
}

impl Namespace {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Edit => "edit",
            Self::Refactor => "refactor",
        }
    }

    #[must_use]
    const fn command_enum(&self) -> &'static str {
        match self {
            Self::Inspect => "InspectCommand",
            Self::Edit => "EditCommand",
            Self::Refactor => "RefactorCommand",
        }
    }
}

/// The first occurrence of `prefix` that is immediately followed by digits.
///
/// A plain "first occurrence" is not enough: `registry_paths.len(), ` also
/// prefixes `registry_paths.len(), unique_registry_paths.len())` earlier in
/// `dialect_contract.rs`, which has no number to read at all.
fn first_captured_number(text: &str, prefix: &str) -> Option<u32> {
    let mut search_from = 0;
    while let Some(offset) = text[search_from..].find(prefix) {
        let start = search_from + offset + prefix.len();
        let digits: String = text[start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(value) = digits.parse() {
            return Some(value);
        }
        search_from = start;
    }
    None
}

/// Everything [`print_command_wiring`] needs to name the generated symbols.
/// A struct rather than positional arguments, since seven of them are all
/// `&str` and a call site that swapped two by accident would still compile.
pub struct CommandWiring<'a> {
    pub namespace: Namespace,
    pub leaf: &'a str,
    pub feature_crate: &'a str,
    pub module_path: &'a str,
    pub args_type: &'a str,
    pub variant: &'a str,
    pub workflow_fn: &'a str,
}

/// Prints the checklist for wiring `<namespace> <leaf-name>` into the
/// composition root, using numbers read fresh from the repository rather
/// than restated by hand.
pub fn print_command_wiring(repo: &Repo, wiring: &CommandWiring<'_>) -> Result<()> {
    let namespace = &wiring.namespace;
    let leaf = wiring.leaf;
    let feature_crate = wiring.feature_crate;
    let module_path = wiring.module_path;
    let args_type = wiring.args_type;
    let variant = wiring.variant;
    let workflow_fn = wiring.workflow_fn;

    let contract = fs::read_to_string(repo.path("src/presentation/cli/contract.rs"))
        .map_err(crate::error::XtaskError::io("read src/presentation/cli/contract.rs"))?;
    let dialect_contract = fs::read_to_string(repo.path("tests/cli/dialect_contract.rs"))
        .map_err(crate::error::XtaskError::io("read tests/cli/dialect_contract.rs"))?;

    let introspection = first_captured_number(&contract, "const INTROSPECTION_COMMANDS: [&str; ");
    let format = first_captured_number(&contract, "const FORMAT_COMMANDS: [&str; ");
    let structural = first_captured_number(&contract, "const STRUCTURAL_COMMANDS: [&str; ");
    let semantic = first_captured_number(&contract, "const SEMANTIC_COMMANDS: [&str; ");
    let total = first_captured_number(&dialect_contract, "registry_paths.len(), ");
    let command_enum = namespace.command_enum();
    let namespace_name = namespace.as_str();

    println!();
    println!("== Remaining manual wiring for `paredit {namespace_name} {leaf}` ==");
    println!();
    println!("1. src/presentation/cli.rs — add a facade re-export near the others:");
    println!("     use {feature_crate}::{module_path}::cli as {module_path}_cli;");
    println!();
    println!("2. src/presentation/cli/command.rs — add a variant to `{command_enum}`:");
    println!("     /// <one-line doc — becomes the --help text>");
    println!("     {variant}({module_path}_cli::args::{args_type}),");
    println!("   and add `{module_path}_cli` to the multi-line `use` block above it — watch for a");
    println!("   double comma if you paste between two existing entries.");
    println!();
    println!(
        "3. src/presentation/cli/dispatch.rs — add the matching match arm in the \
         `{command_enum}` dispatch:"
    );
    println!(
        "     {command_enum}::{variant}(args) => {module_path}_cli::workflow::{workflow_fn}(args)?,"
    );
    println!();
    match namespace {
        Namespace::Inspect => {
            println!(
                "4. src/presentation/cli/contract.rs — INTROSPECTION_COMMANDS is currently \
                 [&str; {}]; add \"{} {leaf}\" and bump the length to {}.",
                introspection.unwrap_or(0),
                namespace.as_str(),
                introspection.map_or(0, |n| n + 1)
            );
        }
        Namespace::Edit => {
            println!(
                "4. src/presentation/cli/contract.rs — pick whichever fits and bump its length:"
            );
            println!(
                "     FORMAT_COMMANDS is [&str; {}] (printing-only edits)",
                format.unwrap_or(0)
            );
            println!(
                "     STRUCTURAL_COMMANDS is [&str; {}] (structural edits)",
                structural.unwrap_or(0)
            );
        }
        Namespace::Refactor => {
            println!(
                "4. src/presentation/cli/contract.rs — SEMANTIC_COMMANDS is currently \
                 [&str; {}]; add \"{} {leaf}\" and bump the length to {}.",
                semantic.unwrap_or(0),
                namespace.as_str(),
                semantic.map_or(0, |n| n + 1)
            );
        }
    }
    println!();
    if let Some(total) = total {
        let new_total = total + 1;
        println!(
            "5. tests/cli/dialect_contract.rs — the total across all four arrays is currently \
             {total} (grep the file for the literal `{total}`; it recurs in \
             `registry_paths.len()`, `command_count`, `cell_count` \u{d7}3, the `category_counts` \
             map, and the per-dialect status-sum loop). Every one of those becomes {new_total}, \
             and every `cell_count` becomes {new_total} \u{d7} 10 = {}.",
            new_total * 10
        );
    } else {
        println!(
            "5. tests/cli/dialect_contract.rs — bump the eight assertions that read off the \
             total command count (see docs/development.md's pinned-count list)."
        );
    }
    println!(
        "   Also add this command's row to the per-dialect support matrix — which of the 10 \
         dialects it actually supports is a judgment call, not something to default."
    );
    println!();
    println!("6. docs/src/commands.md — add a row for `{leaf}` to the relevant table.");
    println!();

    Ok(())
}
