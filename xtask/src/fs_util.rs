//! Writing generated files without silently clobbering hand-written ones.

use std::path::Path;

use anyhow::{Context, bail};

pub fn write_new_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    if path.exists() {
        bail!(
            "{} already exists — refusing to overwrite; remove it first if regenerating",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("write {}", path.display()))?;
    println!("  created {}", path.display());
    Ok(())
}

/// Inserts `module_line` into the contiguous block of `pub mod ...;`
/// declarations at the top of `lib_rs`, keeping it alphabetical the way every
/// existing `lib.rs` in this workspace already is.
pub fn insert_sorted_mod_line(lib_rs: &Path, module_line: &str) -> anyhow::Result<()> {
    let text =
        std::fs::read_to_string(lib_rs).with_context(|| format!("read {}", lib_rs.display()))?;
    if text.contains(module_line) {
        bail!("{} already contains `{module_line}`", lib_rs.display());
    }

    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let first = lines
        .iter()
        .position(|line| line.starts_with("pub mod "))
        .with_context(|| format!("{} has no `pub mod` line to anchor on", lib_rs.display()))?;
    let mut last = first;
    while last + 1 < lines.len() && lines[last + 1].starts_with("pub mod ") {
        last += 1;
    }

    let mut at = first;
    while at <= last && lines[at].as_str() < module_line {
        at += 1;
    }
    lines.insert(at, module_line.to_owned());

    let mut new_text = lines.join("\n");
    if text.ends_with('\n') {
        new_text.push('\n');
    }
    std::fs::write(lib_rs, new_text).with_context(|| format!("write {}", lib_rs.display()))?;
    println!("  updated {} (+ `{module_line}`)", lib_rs.display());
    Ok(())
}
