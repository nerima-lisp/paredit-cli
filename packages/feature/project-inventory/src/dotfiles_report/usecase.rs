use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use cap_std::fs::{Dir, Metadata};
use paredit_core_cli::error::{CliError, CliResult};

use super::domain::{Category, Diagnostic, Entry, EntryKind, EntryStatus, Report};

#[derive(Clone, Copy)]
struct Limits {
    entries: usize,
    depth: usize,
    file_bytes: u64,
}

const LIMITS: Limits = Limits {
    entries: 10_000,
    depth: 16,
    file_bytes: 16 * 1024 * 1024,
};

pub fn build(root: &Path) -> CliResult<Report> {
    build_with_limits(root, LIMITS)
}

fn build_with_limits(root: &Path, limits: Limits) -> CliResult<Report> {
    let root = Dir::open_ambient_dir(root, cap_std::ambient_authority())
        .map_err(CliError::io("opening project root for dotfiles inventory"))?;
    let mut report = Report {
        present: false,
        complete: true,
        entries: Vec::new(),
        diagnostics: Vec::new(),
    };
    let metadata = match root.symlink_metadata(".paredit") {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(report),
        Err(error) => return Err(CliError::io("inspecting .paredit metadata")(error)),
    };
    if !metadata.is_dir() && !metadata.file_type().is_symlink() {
        return Err(CliError::io("inspecting .paredit metadata")(
            io::Error::new(
                io::ErrorKind::InvalidData,
                ".paredit must be a directory or an unfollowed symlink",
            ),
        ));
    }
    report.present = true;
    add_entry(&mut report, ".paredit", &metadata, 0, limits);
    if metadata.is_dir() {
        let directory = open_child(&root, Path::new(".paredit"), ".paredit")?;
        walk(&directory, ".paredit", 0, limits, &mut report)?;
    }
    report.entries.sort_by(|a, b| a.path.cmp(&b.path));
    report
        .diagnostics
        .sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.code.cmp(b.code)));
    Ok(report)
}

fn open_child(parent: &Dir, name: &Path, display: &str) -> CliResult<Dir> {
    let parent_file = parent
        .try_clone()
        .map_err(CliError::io(format!(
            "retaining directory handle for {display}"
        )))?
        .into_std_file();
    // A single-component, no-follow open closes the metadata/open symlink-swap gap.
    let file = cap_primitives::fs::open_dir_nofollow(&parent_file, name).map_err(CliError::io(
        format!("opening inventory directory {display}"),
    ))?;
    Ok(Dir::from_std_file(file))
}

fn walk(
    directory: &Dir,
    display: &str,
    depth: usize,
    limits: Limits,
    report: &mut Report,
) -> CliResult<()> {
    let remaining = limits.entries.saturating_sub(report.entries.len());
    let mut names = Vec::new();
    for child in directory.entries().map_err(CliError::io(format!(
        "listing inventory directory {display}"
    )))? {
        let name = child
            .map_err(CliError::io(format!(
                "listing inventory directory {display}"
            )))?
            .file_name();
        if names.len() == remaining {
            entry_limit(report, limits);
            return Ok(());
        }
        names.push(inventory_name(name, display)?);
    }
    names.sort();
    for name in names {
        if report.entries.len() == limits.entries {
            entry_limit(report, limits);
            return Ok(());
        }
        let child_path = format!("{display}/{name}");
        let name = PathBuf::from(name);
        let metadata = directory
            .symlink_metadata(&name)
            .map_err(CliError::io(format!(
                "inspecting inventory metadata {child_path}"
            )))?;
        add_entry(report, &child_path, &metadata, depth + 1, limits);
        if metadata.is_dir() && depth + 1 < limits.depth {
            let child = open_child(directory, &name, &child_path)?;
            walk(&child, &child_path, depth + 1, limits, report)?;
            if report
                .diagnostics
                .iter()
                .any(|item| item.code == "entry_limit")
            {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn inventory_name(name: OsString, display: &str) -> CliResult<String> {
    name.into_string().map_err(|_| {
        CliError::io(format!("listing inventory directory {display}"))(io::Error::new(
            io::ErrorKind::InvalidData,
            "inventory filename is not UTF-8",
        ))
    })
}

fn add_entry(report: &mut Report, path: &str, metadata: &Metadata, depth: usize, limits: Limits) {
    let (kind, bytes, status) = if metadata.file_type().is_symlink() {
        (EntryKind::Symlink, None, EntryStatus::NotFollowed)
    } else if metadata.is_dir() {
        let status = if depth >= limits.depth {
            diagnostic(
                report,
                path,
                "depth_limit",
                format!("directory depth limit is {}", limits.depth),
            );
            EntryStatus::DepthLimit
        } else {
            EntryStatus::Inventoried
        };
        (EntryKind::Directory, None, status)
    } else if metadata.is_file() {
        let status = if metadata.len() > limits.file_bytes {
            diagnostic(
                report,
                path,
                "file_size_limit",
                format!("file metadata exceeds {} bytes", limits.file_bytes),
            );
            EntryStatus::FileSizeLimit
        } else {
            EntryStatus::Inventoried
        };
        (EntryKind::File, Some(metadata.len()), status)
    } else {
        (EntryKind::Other, None, EntryStatus::Unsupported)
    };
    let category = match path.strip_prefix(".paredit/") {
        Some("kill-ring.json") => Category::KillRing,
        Some("rules") => Category::Rule,
        Some(path) if path.starts_with("rules/") => Category::Rule,
        Some("migrations") => Category::Migration,
        Some(path) if path.starts_with("migrations/") => Category::Migration,
        _ => Category::Other,
    };
    report.entries.push(Entry {
        path: path.to_owned(),
        kind,
        category,
        bytes,
        status,
    });
}

fn diagnostic(report: &mut Report, path: &str, code: &'static str, message: String) {
    report.complete = false;
    report.diagnostics.push(Diagnostic {
        path: path.to_owned(),
        code,
        message,
    });
}

fn entry_limit(report: &mut Report, limits: Limits) {
    // Never expose an arbitrary read_dir prefix as a stable inventory.
    report.entries.clear();
    report.diagnostics.clear();
    diagnostic(
        report,
        ".paredit",
        "entry_limit",
        format!("inventory exceeds {} entries", limits.entries),
    );
}

#[cfg(test)]
mod tests;
