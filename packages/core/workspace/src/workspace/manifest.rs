//! Project manifests as an input source.
//!
//! Walking a directory answers "which files exist". A manifest answers "which
//! files are part of the project, and in what order" — and those are not the
//! same set. A directory walk picks up an abandoned experiment left next to the
//! sources and misses a file the build pulls in from a sibling directory; an
//! ASDF `:components` list contains exactly the files the system compiles, in
//! the order it compiles them.
//!
//! Four manifest formats are read:
//!
//! * ASDF `.asd` — `defsystem` with `:components`, `:module`, `:pathname` and
//!   `:serial`, which yields an *ordered* file list (F4);
//! * Clojure `deps.edn` and `shadow-cljs.edn` — `:paths` / `:extra-paths` (F5);
//! * Leiningen `project.clj` — `:source-paths` / `:test-paths` (F5);
//! * Emacs Lisp package headers — `Package-Requires` (F6).
//!
//! All four are parsed with this project's own reader rather than with a format
//! library, which is the point: `deps.edn` is EDN and `project.clj` is Clojure
//! source, and both are S-expressions that the tool can already read exactly.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SyntaxTree};

use super::error::{WorkspaceError, WorkspaceLimit};

/// The largest manifest that will be parsed.
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

/// How deep a `:module` nest may go before the manifest is refused.
///
/// ASDF itself has no limit, but a `.asd` inside a scanned tree is untrusted
/// input, and unbounded recursion over it is a stack overflow waiting to be
/// found.
const MAX_MODULE_DEPTH: usize = 64;

/// The most component entries one manifest may declare.
const MAX_MANIFEST_COMPONENTS: usize = 100_000;

/// The number of leading lines searched for an Emacs Lisp package header.
///
/// `package.el` reads the header out of the commentary block at the top of the
/// file, and every real one is within a few dozen lines.
const MAX_ELISP_HEADER_LINES: usize = 200;

/// Which manifest format a source came from.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ManifestKind {
    /// An ASDF system definition (`*.asd`).
    Asdf,
    /// A `deps.edn` for the Clojure CLI.
    DepsEdn,
    /// A `shadow-cljs.edn` build configuration.
    ShadowCljs,
    /// A Leiningen `project.clj`.
    Leiningen,
    /// An Emacs Lisp file carrying a `Package-Requires` header.
    ElispPackage,
}

impl ManifestKind {
    /// A stable identifier for JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asdf => "asdf",
            Self::DepsEdn => "deps-edn",
            Self::ShadowCljs => "shadow-cljs",
            Self::Leiningen => "leiningen",
            Self::ElispPackage => "elisp-package",
        }
    }

    /// The dialect whose reader rules parse this manifest.
    const fn dialect(self) -> Dialect {
        match self {
            Self::Asdf => Dialect::CommonLisp,
            Self::DepsEdn | Self::ShadowCljs | Self::Leiningen => Dialect::Clojure,
            Self::ElispPackage => Dialect::EmacsLisp,
        }
    }

    /// Recognises a manifest by file name.
    #[must_use]
    pub fn from_file_name(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        match name {
            "deps.edn" => return Some(Self::DepsEdn),
            "shadow-cljs.edn" => return Some(Self::ShadowCljs),
            "project.clj" => return Some(Self::Leiningen),
            _ => {}
        }
        if path.extension().and_then(|extension| extension.to_str()) == Some("asd") {
            return Some(Self::Asdf);
        }
        None
    }
}

/// One declared dependency, however the manifest spelled it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ManifestDependency {
    /// The dependency name, with reader syntax (`"..."`, `#:`, `:`) stripped.
    pub name: String,
    /// The version constraint, when the format carries one.
    pub version: Option<String>,
}

/// Where a source path came from inside its manifest.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SourcePathRole {
    /// The primary source path set.
    Source,
    /// A test-only path set.
    Test,
    /// An alias- or profile-scoped extra path.
    Extra,
}

impl SourcePathRole {
    /// A stable identifier for JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Test => "test",
            Self::Extra => "extra",
        }
    }
}

/// A directory a manifest declares as holding sources.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ManifestSourcePath {
    /// The path, resolved against the manifest's own directory.
    pub path: PathBuf,
    /// What the manifest called it.
    pub role: SourcePathRole,
}

/// Everything one manifest says about the project's inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestSource {
    /// Which format this came from.
    pub kind: ManifestKind,
    /// The manifest file itself.
    pub path: PathBuf,
    /// The declared system or project name, when the format has one.
    pub name: Option<String>,
    /// Directories declared as holding sources, for the formats that name
    /// directories rather than files.
    pub source_paths: Vec<ManifestSourcePath>,
    /// Files named explicitly, in declaration order.
    ///
    /// ASDF is the format that fills this in. The order is the build order,
    /// which is the one thing a directory walk can never recover.
    pub files: Vec<PathBuf>,
    /// Declared dependencies.
    pub dependencies: Vec<ManifestDependency>,
    /// Paths the manifest named that do not exist on disk.
    ///
    /// Worth reporting rather than dropping: a `:file "helper"` with no
    /// `helper.lisp` next to it is a broken build, and this is often the first
    /// tool to look.
    pub missing: Vec<PathBuf>,
}

impl ManifestSource {
    /// Every path this manifest contributes as a discovery root.
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        let mut roots = self
            .source_paths
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        roots.extend(self.files.iter().cloned());
        roots
    }
}

/// Parses one manifest file.
///
/// Returns `Ok(None)` when the file is not a manifest this tool reads, or when
/// it is one but carries nothing — an `.el` without a `Package-Requires`
/// header, for instance. A manifest that fails to parse is *not* an error
/// either: `.asd` files routinely contain read-time conditionals and macro
/// calls that no static reader resolves, and refusing to scan a project
/// because one of them is unusual would be the wrong trade.
pub fn parse_manifest(path: &Path) -> Result<Option<ManifestSource>, WorkspaceError> {
    let Some(kind) = ManifestKind::from_file_name(path) else {
        return Ok(None);
    };
    parse_manifest_as(path, kind)
}

/// Parses `path` as a specific manifest format.
pub fn parse_manifest_as(
    path: &Path,
    kind: ManifestKind,
) -> Result<Option<ManifestSource>, WorkspaceError> {
    let Some(contents) = read_manifest(path)? else {
        return Ok(None);
    };
    let base = path.parent().unwrap_or_else(|| Path::new("."));

    if kind == ManifestKind::ElispPackage {
        return Ok(parse_elisp_package_header(path, &contents));
    }

    let Ok(tree) = SyntaxTree::parse_with_dialect(&contents, kind.dialect()) else {
        return Ok(None);
    };
    let root = tree.root_view();

    let source = match kind {
        ManifestKind::Asdf => parse_asdf(path, base, &root),
        ManifestKind::DepsEdn | ManifestKind::ShadowCljs => parse_edn(path, base, &root, kind),
        ManifestKind::Leiningen => parse_leiningen(path, base, &root),
        ManifestKind::ElispPackage => unreachable!("handled above"),
    };
    Ok(source)
}

/// Reads an `.el` file's `Package-Requires` header (F6).
///
/// Exposed separately from [`parse_manifest`] because an Emacs Lisp package
/// header lives inside an ordinary source file rather than in a file whose name
/// identifies it, so the caller decides which files are worth looking at.
pub fn parse_elisp_package(path: &Path) -> Result<Option<ManifestSource>, WorkspaceError> {
    let Some(contents) = read_manifest(path)? else {
        return Ok(None);
    };
    Ok(parse_elisp_package_header(path, &contents))
}

fn read_manifest(path: &Path) -> Result<Option<String>, WorkspaceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(WorkspaceError::Io {
                context: format!("failed to inspect {}", path.display()),
                source,
            });
        }
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(WorkspaceLimit::FileSize {
            path: path.to_path_buf(),
            actual: metadata.len(),
            maximum: MAX_MANIFEST_BYTES,
        }
        .into());
    }
    match fs::read(path) {
        Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(WorkspaceError::Io {
            context: format!("failed to read {}", path.display()),
            source,
        }),
    }
}

// --- ASDF -----------------------------------------------------------------

fn parse_asdf(path: &Path, base: &Path, root: &ExpressionView) -> Option<ManifestSource> {
    let mut name = None;
    let mut files = Vec::new();
    let mut missing = Vec::new();
    let mut dependencies = Vec::new();
    let mut found_system = false;

    for form in &root.children {
        if !is_list(form) {
            continue;
        }
        let Some(head) = form.children.first().and_then(atom_text) else {
            continue;
        };
        if !is_defsystem_head(head) {
            continue;
        }
        found_system = true;

        if name.is_none() {
            name = form.children.get(1).and_then(atom_text).map(designator);
        }

        let plist = &form.children[form.children.len().min(2)..];
        let system_base = plist_value(plist, "pathname")
            .and_then(atom_text)
            .map_or_else(|| base.to_path_buf(), |text| base.join(designator(text)));

        for keyword in ["depends-on", "defsystem-depends-on", "weakly-depends-on"] {
            if let Some(list) = plist_value(plist, keyword) {
                collect_asdf_dependencies(list, &mut dependencies);
            }
        }

        if let Some(components) = plist_value(plist, "components") {
            collect_asdf_components(components, &system_base, 0, &mut files, &mut missing);
        }
    }

    if !found_system {
        return None;
    }

    dependencies.sort();
    dependencies.dedup();
    Some(ManifestSource {
        kind: ManifestKind::Asdf,
        path: path.to_path_buf(),
        name,
        source_paths: Vec::new(),
        files,
        dependencies,
        missing,
    })
}

fn is_defsystem_head(head: &str) -> bool {
    let bare = head.rsplit(':').next().unwrap_or(head);
    bare.eq_ignore_ascii_case("defsystem")
}

fn collect_asdf_dependencies(list: &ExpressionView, dependencies: &mut Vec<ManifestDependency>) {
    if !is_list(list) {
        if let Some(text) = atom_text(list) {
            dependencies.push(ManifestDependency {
                name: designator(text),
                version: None,
            });
        }
        return;
    }

    for entry in &list.children {
        match atom_text(entry) {
            Some(text) => dependencies.push(ManifestDependency {
                name: designator(text),
                version: None,
            }),
            // ASDF accepts three nested spellings, and they put the system
            // name in three different places:
            //
            //   (:version "alexandria" "1.0")   -> name at 1, version at 2
            //   (:feature :sbcl "sb-posix")     -> name at 2
            //   (:require :sb-posix)            -> name at 1
            //
            // Guessing by position — "the last atom" — reads `"1.0"` as the
            // system name for the first of them, which is how this was wrong
            // the first time.
            None => {
                let head = entry.children.first().and_then(atom_text).unwrap_or("");
                let (name_index, version_index) =
                    match head.trim_start_matches(':').to_ascii_lowercase().as_str() {
                        "version" => (1, Some(2)),
                        "feature" => (2, None),
                        _ => (1, None),
                    };
                let name = entry
                    .children
                    .get(name_index)
                    .and_then(atom_text)
                    .map(designator);
                let version = version_index
                    .and_then(|index| entry.children.get(index))
                    .and_then(atom_text)
                    .map(designator);
                if let Some(name) = name {
                    dependencies.push(ManifestDependency { name, version });
                }
            }
        }
    }
}

fn collect_asdf_components(
    components: &ExpressionView,
    base: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
    missing: &mut Vec<PathBuf>,
) {
    if depth > MAX_MODULE_DEPTH || !is_list(components) {
        return;
    }

    for component in &components.children {
        if files.len() + missing.len() >= MAX_MANIFEST_COMPONENTS {
            return;
        }
        if !is_list(component) {
            continue;
        }
        let Some(kind) = component.children.first().and_then(atom_text) else {
            continue;
        };
        let kind = kind.trim_start_matches(':').to_ascii_lowercase();
        let Some(name) = component.children.get(1).and_then(atom_text) else {
            continue;
        };
        let name = designator(name);
        let plist = &component.children[component.children.len().min(2)..];
        let pathname = plist_value(plist, "pathname")
            .and_then(atom_text)
            .map(designator);

        match kind.as_str() {
            "module" | "package-inferred-system" => {
                let module_base = base.join(pathname.unwrap_or(name));
                if let Some(nested) = plist_value(plist, "components") {
                    collect_asdf_components(nested, &module_base, depth + 1, files, missing);
                }
            }
            "static-file" | "doc-file" | "html-file" => {
                // A static file's name already carries its type.
                let file = base.join(pathname.unwrap_or(name));
                push_component(file, files, missing);
            }
            "file" | "source-file" | "cl-source-file" | "c-source-file" | "java-source-file" => {
                let file = base.join(pathname.unwrap_or(name));
                push_component(with_default_extension(file, "lisp"), files, missing);
            }
            other if other.starts_with("cl-source-file.") => {
                // `:cl-source-file.cl` names the extension in the type itself.
                let extension = other.trim_start_matches("cl-source-file.");
                let file = base.join(pathname.unwrap_or(name));
                push_component(with_default_extension(file, extension), files, missing);
            }
            // `:system` and `:module`-less references name another system, not
            // a file in this one.
            _ => {}
        }
    }
}

fn push_component(file: PathBuf, files: &mut Vec<PathBuf>, missing: &mut Vec<PathBuf>) {
    let file = lexically_normalize(&file);
    if file.is_file() {
        if !files.contains(&file) {
            files.push(file);
        }
    } else if !missing.contains(&file) {
        missing.push(file);
    }
}

/// Adds `extension` unless the last component already has one.
///
/// ASDF's rule, which is not the same as "append if absent": a component named
/// `foo.bar` is the file `foo.bar.lisp`, because the type comes from the
/// component class rather than from the name. Only a name whose extension is
/// already the expected one is left alone.
fn with_default_extension(path: PathBuf, extension: &str) -> PathBuf {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    {
        return path;
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(extension);
    path.with_file_name(name)
}

// --- Clojure --------------------------------------------------------------

fn parse_edn(
    path: &Path,
    base: &Path,
    root: &ExpressionView,
    kind: ManifestKind,
) -> Option<ManifestSource> {
    let map = root
        .children
        .iter()
        .find(|child| child.delimiter_is_brace())?;

    let mut source_paths = Vec::new();
    push_string_paths(
        map_value(map, ":paths"),
        base,
        SourcePathRole::Source,
        &mut source_paths,
    );
    push_string_paths(
        map_value(map, ":source-paths"),
        base,
        SourcePathRole::Source,
        &mut source_paths,
    );
    push_string_paths(
        map_value(map, ":extra-paths"),
        base,
        SourcePathRole::Extra,
        &mut source_paths,
    );

    // `:aliases {:dev {:extra-paths ["dev"]}}` is where a project keeps the
    // directories that only some invocations see. They are reported as `extra`
    // so a caller can decide whether to scan them.
    if let Some(aliases) = map_value(map, ":aliases") {
        for value in map_values(aliases) {
            push_string_paths(
                map_value(value, ":extra-paths"),
                base,
                SourcePathRole::Extra,
                &mut source_paths,
            );
            push_string_paths(
                map_value(value, ":paths"),
                base,
                SourcePathRole::Extra,
                &mut source_paths,
            );
        }
    }

    let mut dependencies = Vec::new();
    for key in [":deps", ":extra-deps", ":dependencies"] {
        if let Some(deps) = map_value(map, key) {
            collect_edn_dependencies(deps, &mut dependencies);
        }
    }

    if source_paths.is_empty() && dependencies.is_empty() {
        return None;
    }

    let (source_paths, missing) = split_existing(source_paths);
    dependencies.sort();
    dependencies.dedup();
    Some(ManifestSource {
        kind,
        path: path.to_path_buf(),
        name: None,
        source_paths,
        files: Vec::new(),
        dependencies,
        missing,
    })
}

fn parse_leiningen(path: &Path, base: &Path, root: &ExpressionView) -> Option<ManifestSource> {
    let form = root.children.iter().find(|child| {
        is_list(child)
            && child
                .children
                .first()
                .and_then(atom_text)
                .is_some_and(|head| head == "defproject")
    })?;

    let name = form.children.get(1).and_then(atom_text).map(designator);
    // `(defproject name version & options)`: the options start at index 3, not
    // at 2. Starting a pair walk one slot early pairs the version string with
    // the first keyword and reads every option as a value.
    let plist = &form.children[form.children.len().min(3)..];

    let mut source_paths = Vec::new();
    push_string_paths(
        plist_keyword_value(plist, ":source-paths"),
        base,
        SourcePathRole::Source,
        &mut source_paths,
    );
    push_string_paths(
        plist_keyword_value(plist, ":test-paths"),
        base,
        SourcePathRole::Test,
        &mut source_paths,
    );
    push_string_paths(
        plist_keyword_value(plist, ":resource-paths"),
        base,
        SourcePathRole::Extra,
        &mut source_paths,
    );

    // Leiningen's documented defaults, applied only when the project says
    // nothing. Reporting an empty set for a project that simply relies on the
    // defaults would make `--from-manifest` scan nothing at all.
    if source_paths.is_empty() {
        source_paths.push(ManifestSourcePath {
            path: base.join("src"),
            role: SourcePathRole::Source,
        });
        source_paths.push(ManifestSourcePath {
            path: base.join("test"),
            role: SourcePathRole::Test,
        });
    }

    let mut dependencies = Vec::new();
    if let Some(deps) = plist_keyword_value(plist, ":dependencies") {
        collect_leiningen_dependencies(deps, &mut dependencies);
    }

    let (source_paths, missing) = split_existing(source_paths);
    dependencies.sort();
    dependencies.dedup();
    Some(ManifestSource {
        kind: ManifestKind::Leiningen,
        path: path.to_path_buf(),
        name,
        source_paths,
        files: Vec::new(),
        dependencies,
        missing,
    })
}

fn collect_edn_dependencies(map: &ExpressionView, dependencies: &mut Vec<ManifestDependency>) {
    if !map.delimiter_is_brace() {
        return;
    }
    let mut index = 0;
    while index + 1 < map.children.len() {
        let key = &map.children[index];
        let value = &map.children[index + 1];
        index += 2;
        let Some(name) = atom_text(key) else {
            continue;
        };
        let version = map_value(value, ":mvn/version")
            .or_else(|| map_value(value, ":git/tag"))
            .or_else(|| map_value(value, ":git/sha"))
            .or_else(|| map_value(value, ":local/root"))
            .and_then(atom_text)
            .map(designator);
        dependencies.push(ManifestDependency {
            name: designator(name),
            version,
        });
    }
}

fn collect_leiningen_dependencies(
    vector: &ExpressionView,
    dependencies: &mut Vec<ManifestDependency>,
) {
    for entry in &vector.children {
        let Some(name) = entry.children.first().and_then(atom_text) else {
            continue;
        };
        let version = entry.children.get(1).and_then(atom_text).map(designator);
        dependencies.push(ManifestDependency {
            name: designator(name),
            version,
        });
    }
}

// --- Emacs Lisp -----------------------------------------------------------

fn parse_elisp_package_header(path: &Path, contents: &str) -> Option<ManifestSource> {
    let mut dependencies = Vec::new();
    let mut name = None;
    let mut found = false;

    for (index, line) in contents.lines().enumerate() {
        if index >= MAX_ELISP_HEADER_LINES {
            break;
        }
        let Some(body) = strip_comment_prefix(line) else {
            continue;
        };
        if let Some(value) = header_value(body, "Package-Requires") {
            found = true;
            collect_elisp_requires(&value, &mut dependencies);
        }
        if name.is_none() {
            name = header_value(body, "Package")
                .or_else(|| header_value(body, "Provide"))
                .map(|value| value.trim().to_owned());
        }
    }

    if !found {
        return None;
    }

    let name = name.or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
    });
    dependencies.sort();
    dependencies.dedup();
    // An Emacs Lisp package has no file that lists its sources; the header is
    // carried by one `.el` among the others. So the source path is the
    // directory the header was found in — which, for `package.el`, is exactly
    // what the package is.
    let source_paths = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| {
            vec![ManifestSourcePath {
                path: parent.to_path_buf(),
                role: SourcePathRole::Source,
            }]
        })
        .unwrap_or_default();
    Some(ManifestSource {
        kind: ManifestKind::ElispPackage,
        path: path.to_path_buf(),
        name,
        source_paths,
        files: Vec::new(),
        dependencies,
        missing: Vec::new(),
    })
}

/// Reads every Emacs Lisp package header directly inside `directory`.
///
/// Separate from [`manifests_in_directory`] because an `.el` file is not
/// recognisable as a manifest by its name: the only way to know is to read the
/// header. That makes this the one manifest scan with a per-file cost, so it is
/// deliberately shallow and only reads files whose extension says `.el`.
pub fn elisp_package_manifests_in_directory(
    directory: &Path,
) -> Result<Vec<ManifestSource>, WorkspaceError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(WorkspaceError::Io {
                context: format!("failed to list {}", directory.display()),
                source,
            });
        }
    };

    let mut candidates = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|source| WorkspaceError::Io {
            context: format!("failed to list {}", directory.display()),
            source,
        })?;
        let path = entry.path();
        let is_elisp = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("el"));
        if is_elisp && path.is_file() {
            candidates.insert(path);
        }
    }

    let mut found = Vec::new();
    for candidate in candidates {
        if let Some(source) = parse_elisp_package(&candidate)? {
            found.push(source);
        }
    }
    Ok(found)
}

/// Strips the `;` run that opens an Emacs Lisp comment line.
fn strip_comment_prefix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with(';') {
        return None;
    }
    Some(trimmed.trim_start_matches(';'))
}

/// Reads a `Header: value` pair out of a comment body.
///
/// `package.el` matches the header name case-insensitively, and so does this.
fn header_value(body: &str, header: &str) -> Option<String> {
    let trimmed = body.trim_start();
    let (candidate, rest) = trimmed.split_once(':')?;
    if !candidate.trim().eq_ignore_ascii_case(header) {
        return None;
    }
    Some(rest.trim().to_owned())
}

/// Parses the `((emacs "27.1") (dash "2.19"))` value of `Package-Requires`.
fn collect_elisp_requires(value: &str, dependencies: &mut Vec<ManifestDependency>) {
    let Ok(tree) = SyntaxTree::parse_with_dialect(value, Dialect::EmacsLisp) else {
        return;
    };
    let root = tree.root_view();
    let Some(list) = root.children.first() else {
        return;
    };
    for entry in &list.children {
        match atom_text(entry) {
            Some(text) => dependencies.push(ManifestDependency {
                name: designator(text),
                version: None,
            }),
            None => {
                let Some(name) = entry.children.first().and_then(atom_text) else {
                    continue;
                };
                dependencies.push(ManifestDependency {
                    name: designator(name),
                    version: entry.children.get(1).and_then(atom_text).map(designator),
                });
            }
        }
    }
}

// --- shared helpers -------------------------------------------------------

trait BraceView {
    fn delimiter_is_brace(&self) -> bool;
}

impl BraceView for ExpressionView {
    fn delimiter_is_brace(&self) -> bool {
        self.delimiter == Some(paredit_core_syntax::sexpr::Delimiter::Brace)
    }
}

fn is_list(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

/// Strips the reader syntax around a designator and unescapes a string.
fn designator(text: &str) -> String {
    let unprefixed = text
        .strip_prefix("#:")
        .or_else(|| text.strip_prefix(':'))
        .unwrap_or(text);
    match unprefixed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        Some(inner) => unescape_string(inner),
        None => unprefixed.to_owned(),
    }
}

fn unescape_string(inner: &str) -> String {
    let mut result = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            if let Some(escaped) = characters.next() {
                result.push(escaped);
            }
            continue;
        }
        result.push(character);
    }
    result
}

/// Looks up `:keyword value` in a property list, matching ASDF's spelling rules.
fn plist_value<'a>(plist: &'a [ExpressionView], keyword: &str) -> Option<&'a ExpressionView> {
    let mut index = 0;
    while index + 1 < plist.len() {
        let key = atom_text(&plist[index]);
        if key.is_some_and(|key| {
            key.trim_start_matches(':')
                .eq_ignore_ascii_case(keyword.trim_start_matches(':'))
        }) {
            return Some(&plist[index + 1]);
        }
        index += 2;
    }
    None
}

/// Looks up `:keyword value` where the keyword is matched exactly.
///
/// Clojure is case-sensitive, so this is deliberately not the ASDF lookup.
fn plist_keyword_value<'a>(
    plist: &'a [ExpressionView],
    keyword: &str,
) -> Option<&'a ExpressionView> {
    let mut index = 0;
    while index + 1 < plist.len() {
        if atom_text(&plist[index]) == Some(keyword) {
            return Some(&plist[index + 1]);
        }
        index += 2;
    }
    None
}

fn map_value<'a>(map: &'a ExpressionView, key: &str) -> Option<&'a ExpressionView> {
    if !map.delimiter_is_brace() {
        return None;
    }
    let mut index = 0;
    while index + 1 < map.children.len() {
        if atom_text(&map.children[index]) == Some(key) {
            return Some(&map.children[index + 1]);
        }
        index += 2;
    }
    None
}

fn map_values(map: &ExpressionView) -> Vec<&ExpressionView> {
    if !map.delimiter_is_brace() {
        return Vec::new();
    }
    map.children.iter().skip(1).step_by(2).collect()
}

fn push_string_paths(
    value: Option<&ExpressionView>,
    base: &Path,
    role: SourcePathRole,
    into: &mut Vec<ManifestSourcePath>,
) {
    let Some(value) = value else {
        return;
    };
    for entry in &value.children {
        let Some(text) = atom_text(entry) else {
            continue;
        };
        if !text.starts_with('"') {
            continue;
        }
        let path = lexically_normalize(&base.join(designator(text)));
        let candidate = ManifestSourcePath { path, role };
        if !into.contains(&candidate) {
            into.push(candidate);
        }
    }
}

fn split_existing(paths: Vec<ManifestSourcePath>) -> (Vec<ManifestSourcePath>, Vec<PathBuf>) {
    let mut existing = Vec::new();
    let mut missing = Vec::new();
    for entry in paths {
        if entry.path.exists() {
            existing.push(entry);
        } else {
            missing.push(entry.path);
        }
    }
    (existing, missing)
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

/// Finds every manifest directly inside `directory`.
///
/// Deliberately not recursive. A manifest names its own inputs, so recursing to
/// find more of them would defeat the point: the whole reason to read one is to
/// stop guessing which directories matter.
pub fn manifests_in_directory(directory: &Path) -> Result<Vec<PathBuf>, WorkspaceError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(WorkspaceError::Io {
                context: format!("failed to list {}", directory.display()),
                source,
            });
        }
    };

    let mut found = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|source| WorkspaceError::Io {
            context: format!("failed to list {}", directory.display()),
            source,
        })?;
        let path = entry.path();
        if ManifestKind::from_file_name(&path).is_some() && path.is_file() {
            found.insert(path);
        }
    }
    Ok(found.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        ManifestKind, SourcePathRole, designator, parse_manifest_as, with_default_extension,
    };

    fn write(directory: &Path, relative: &str, contents: &str) -> PathBuf {
        let path = directory.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    fn temporary_directory(name: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("paredit-manifest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create temp dir");
        base
    }

    #[test]
    fn designators_lose_their_reader_syntax() {
        assert_eq!(designator("\"app\""), "app");
        assert_eq!(designator("#:app"), "app");
        assert_eq!(designator(":app"), "app");
        assert_eq!(designator("app"), "app");
        assert_eq!(designator(r#""a\"b""#), "a\"b");
    }

    #[test]
    fn a_component_name_with_a_dot_still_gets_the_class_extension() {
        assert_eq!(
            with_default_extension(PathBuf::from("/x/foo.bar"), "lisp"),
            PathBuf::from("/x/foo.bar.lisp")
        );
        assert_eq!(
            with_default_extension(PathBuf::from("/x/foo.lisp"), "lisp"),
            PathBuf::from("/x/foo.lisp")
        );
    }

    #[test]
    fn asdf_components_are_read_in_declaration_order_through_modules() {
        let base = temporary_directory("asdf");
        write(&base, "src/package.lisp", "");
        write(&base, "src/core/a.lisp", "");
        write(&base, "src/core/b.lisp", "");
        let manifest = write(
            &base,
            "app.asd",
            r#"
(defsystem "app"
  :pathname "src/"
  :serial t
  :depends-on ("alexandria" (:version "bordeaux-threads" "0.8"))
  :components ((:file "package")
               (:module "core"
                :components ((:file "a") (:file "b")))
               (:static-file "NOTES")))
"#,
        );

        let source = parse_manifest_as(&manifest, ManifestKind::Asdf)
            .expect("manifest reads")
            .expect("manifest has a system");
        assert_eq!(source.name.as_deref(), Some("app"));
        assert_eq!(
            source.files,
            vec![
                base.join("src/package.lisp"),
                base.join("src/core/a.lisp"),
                base.join("src/core/b.lisp"),
            ]
        );
        assert_eq!(source.missing, vec![base.join("src/NOTES")]);
        let names = source
            .dependencies
            .iter()
            .map(|dependency| dependency.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["alexandria", "bordeaux-threads"]);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn deps_edn_yields_paths_and_alias_extras() {
        let base = temporary_directory("deps");
        std::fs::create_dir_all(base.join("src")).expect("create src");
        std::fs::create_dir_all(base.join("dev")).expect("create dev");
        let manifest = write(
            &base,
            "deps.edn",
            r#"{:paths ["src"]
 :deps {org.clojure/clojure {:mvn/version "1.11.1"}}
 :aliases {:dev {:extra-paths ["dev"]}}}"#,
        );

        let source = parse_manifest_as(&manifest, ManifestKind::DepsEdn)
            .expect("manifest reads")
            .expect("manifest has paths");
        assert_eq!(
            source
                .source_paths
                .iter()
                .map(|entry| (entry.path.clone(), entry.role))
                .collect::<Vec<_>>(),
            vec![
                (base.join("src"), SourcePathRole::Source),
                (base.join("dev"), SourcePathRole::Extra),
            ]
        );
        assert_eq!(source.dependencies.len(), 1);
        assert_eq!(source.dependencies[0].name, "org.clojure/clojure");
        assert_eq!(source.dependencies[0].version.as_deref(), Some("1.11.1"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn project_clj_falls_back_to_the_leiningen_defaults() {
        let base = temporary_directory("lein");
        std::fs::create_dir_all(base.join("src")).expect("create src");
        let manifest = write(
            &base,
            "project.clj",
            r#"(defproject my-app "0.1.0"
  :dependencies [[org.clojure/clojure "1.11.1"]])"#,
        );

        let source = parse_manifest_as(&manifest, ManifestKind::Leiningen)
            .expect("manifest reads")
            .expect("manifest is a project");
        assert_eq!(source.name.as_deref(), Some("my-app"));
        assert_eq!(
            source.source_paths,
            vec![super::ManifestSourcePath {
                path: base.join("src"),
                role: SourcePathRole::Source,
            }]
        );
        assert_eq!(source.missing, vec![base.join("test")]);
        assert_eq!(source.dependencies[0].version.as_deref(), Some("1.11.1"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn elisp_package_requires_is_read_from_the_header() {
        let base = temporary_directory("elisp");
        let manifest = write(
            &base,
            "thing.el",
            r#";;; thing.el --- A thing  -*- lexical-binding: t; -*-
;; Package-Requires: ((emacs "27.1") (dash "2.19.1"))
;;; Commentary:
"#,
        );

        let source = parse_manifest_as(&manifest, ManifestKind::ElispPackage)
            .expect("manifest reads")
            .expect("header is present");
        assert_eq!(source.name.as_deref(), Some("thing"));
        assert_eq!(
            source
                .dependencies
                .iter()
                .map(|dependency| (dependency.name.as_str(), dependency.version.as_deref()))
                .collect::<Vec<_>>(),
            vec![("dash", Some("2.19.1")), ("emacs", Some("27.1"))]
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_file_without_the_header_is_not_a_manifest() {
        let base = temporary_directory("elisp-none");
        let manifest = write(
            &base,
            "plain.el",
            ";;; plain.el --- nothing\n(provide 'plain)\n",
        );
        assert!(
            parse_manifest_as(&manifest, ManifestKind::ElispPackage)
                .expect("manifest reads")
                .is_none()
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
