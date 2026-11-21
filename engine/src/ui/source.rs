//! Where a plugin's files come from, and what confines them (spec §7).
//!
//! A pack's `ui/` folder is one source; the files a caller already holds are
//! another, which is how the embedded reference plugin and the export reach
//! the same rules as a player's pack. Confinement belongs to the source because
//! it is a property of the file set: no path either source hands a rule names
//! a file outside the plugin folder, and a path a plugin writes itself is
//! checked by [`check_relative`] before anything reads it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::finding::{rules, Finding, Location};

use super::caps::MAX_FILES;
use super::FOLDER;

/// The visible files of one plugin.
pub trait Source {
    /// Every regular file under the plugin folder, as `/`-separated paths
    /// relative to it, in a stable (byte) order.
    fn paths(&self) -> &[String];

    /// The bytes of one path from [`Source::paths`].
    fn read(&self, path: &str) -> Result<Vec<u8>, String>;

    /// The file's size in bytes, without reading it. Both sources know it, so
    /// §7's per-file cap rejects an oversized file instead of loading it.
    fn size(&self, path: &str) -> u64;

    /// The pack-relative name a finding gives this file.
    fn display(&self, path: &str) -> String;
}

/// The pack-relative name of a plugin file or, for the empty path, of the
/// folder itself: `ui/surfaces/play.ron`, `ui/`.
pub(super) fn display(path: &str) -> String {
    format!("{FOLDER}/{path}")
}

/// Reject a path that is not confined to the plugin folder: the one rule every
/// path a plugin names obeys, whether it comes from the manifest, a node tree
/// or a source's own file list.
pub(super) fn check_relative(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("the path is empty".to_string());
    }
    if path.contains('\\') {
        return Err(format!(
            "`{path}` uses `\\`, which is a separator on some platforms; a plugin path uses `/`"
        ));
    }
    if path.starts_with('/') {
        return Err(format!("`{path}` is an absolute path"));
    }
    if path.len() > 1 && path.as_bytes()[1] == b':' {
        return Err(format!("`{path}` starts with a drive letter"));
    }
    for segment in path.split('/') {
        if segment == ".." {
            return Err(format!(
                "`{path}` names `..`, which would leave the plugin folder"
            ));
        }
        if segment.is_empty() {
            return Err(format!("`{path}` has an empty segment"));
        }
    }
    Ok(())
}

/// The extension after a plugin path's last dot: `PNG` in
/// `ornament/arch.PNG`, and none for a name without one. A file's kind is read
/// from its name before anything opens it, which is how the image and font
/// rules tell a shipped asset from a manifest or a token table.
pub(super) fn extension(path: &str) -> Option<&str> {
    let name = match path.rfind('/') {
        Some(index) => &path[index + 1..],
        None => path,
    };
    name.rsplit_once('.').map(|(_, extension)| extension)
}

/// Confinement for a source that did not already guarantee it at the
/// filesystem, so a memory source's file list obeys the same rule.
pub(super) fn check_paths(source: &dyn Source) -> Vec<Finding> {
    let mut findings = Vec::new();
    for path in source.paths() {
        if let Err(reason) = check_relative(path) {
            findings.push(Finding::new(
                display(path),
                Location::File,
                rules::UI_PATH_ESCAPE,
                reason,
            ));
        }
    }
    findings
}

/// A pack's `ui/` folder, walked once when it opens.
pub struct DirSource {
    paths: Vec<String>,
    entries: BTreeMap<String, Entry>,
}

struct Entry {
    /// The file to open: the entry itself, or the target of a link that stays
    /// inside the folder.
    file: PathBuf,
    size: u64,
}

impl DirSource {
    /// Walk `folder` once, checking every entry's confinement before it is
    /// listed. A walk that cannot complete returns its findings, because a
    /// partial file list is not a plugin: no rule runs over one.
    pub fn open(folder: &Path) -> Result<DirSource, Vec<Finding>> {
        let root = fs::canonicalize(folder).map_err(|error| {
            vec![read_finding(
                display(""),
                format!("cannot open the plugin folder: {error}"),
            )]
        })?;
        if !root.is_dir() {
            return Err(vec![read_finding(
                display(""),
                "the plugin folder is not a folder".to_string(),
            )]);
        }
        // A `ui/` that is a link out of the pack would make another folder the
        // plugin, so confinement is decided before anything inside it is read.
        if !inside_pack(folder, &root) {
            return Err(vec![Finding::new(
                display(""),
                Location::File,
                rules::UI_PATH_ESCAPE,
                "the plugin folder resolves outside the pack",
            )]);
        }

        let mut walk = Walk {
            root: root.clone(),
            // The root counts as visited, so a link back to it is a loop.
            visited: BTreeSet::from([root.clone()]),
            files: Vec::new(),
            findings: Vec::new(),
        };
        walk.folder(&root, "");
        if !walk.findings.is_empty() {
            return Err(walk.findings);
        }

        let mut files = walk.files;
        files.sort_by(|left, right| left.0.cmp(&right.0));
        let mut paths = Vec::with_capacity(files.len());
        let mut entries = BTreeMap::new();
        for (path, entry) in files {
            paths.push(path.clone());
            entries.insert(path, entry);
        }
        Ok(DirSource { paths, entries })
    }
}

/// Whether the canonical plugin folder lies inside the canonical pack folder,
/// which is its parent by the format's layout of `<pack>/ui/`.
fn inside_pack(folder: &Path, root: &Path) -> bool {
    let pack = match folder.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    // A pack folder that cannot be resolved cannot prove confinement either, so
    // the folder is not opened.
    fs::canonicalize(pack).is_ok_and(|pack| root.starts_with(pack))
}

impl Source for DirSource {
    fn paths(&self) -> &[String] {
        &self.paths
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let entry = self
            .entries
            .get(path)
            .ok_or_else(|| format!("`{path}` is not a file in the plugin folder"))?;
        fs::read(&entry.file).map_err(|error| error.to_string())
    }

    fn size(&self, path: &str) -> u64 {
        self.entries.get(path).map_or(0, |entry| entry.size)
    }

    fn display(&self, path: &str) -> String {
        display(path)
    }
}

/// One walk over a plugin folder.
///
/// The walk stops as soon as it lists one file past the cap, because the
/// folder's count is only interesting up to the point the cap rejects it, and
/// listing 10,000 entries to say so reads exactly what the cap forbids. The
/// count a memory source already holds is `caps::check`'s to report; this is
/// the folder's way of never reaching it.
struct Walk {
    root: PathBuf,
    visited: BTreeSet<PathBuf>,
    files: Vec<(String, Entry)>,
    findings: Vec<Finding>,
}

impl Walk {
    fn stopped(&self) -> bool {
        self.files.len() > MAX_FILES
    }

    fn folder(&mut self, folder: &Path, prefix: &str) {
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(error) => {
                self.findings.push(read_finding(
                    display(prefix),
                    format!("cannot read: {error}"),
                ));
                return;
            }
        };
        for entry in entries {
            if self.stopped() {
                return;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    self.findings.push(read_finding(
                        display(prefix),
                        format!("cannot read one entry: {error}"),
                    ));
                    continue;
                }
            };
            self.entry(entry, prefix);
        }
    }

    fn entry(&mut self, entry: fs::DirEntry, prefix: &str) {
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(name) => {
                self.findings.push(read_finding(
                    display(prefix),
                    format!("`{}` is not a UTF-8 name", name.to_string_lossy()),
                ));
                return;
            }
        };
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let shown = display(&rel);
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(error) => {
                self.findings
                    .push(read_finding(shown, format!("cannot read: {error}")));
                return;
            }
        };
        if kind.is_symlink() {
            self.follow(&entry.path(), &shown, &rel);
        } else if kind.is_dir() {
            self.folder(&entry.path(), &rel);
        } else if kind.is_file() {
            match entry.metadata() {
                Ok(metadata) => self.file(rel, entry.path(), metadata.len()),
                Err(error) => self
                    .findings
                    .push(read_finding(shown, format!("cannot read: {error}"))),
            }
        }
        // A device, a socket or a fifo holds no plugin content, and the player
        // never opens one.
    }

    /// A link is listed only when its target stays inside the folder; a link
    /// out is the escape §7 forbids, and following it would read a file the
    /// pack does not own.
    fn follow(&mut self, link: &Path, shown: &str, rel: &str) {
        let target = match fs::canonicalize(link) {
            Ok(target) => target,
            Err(error) => {
                self.findings.push(read_finding(
                    shown.to_string(),
                    format!("a link that cannot be read: {error}"),
                ));
                return;
            }
        };
        if !target.starts_with(&self.root) {
            self.findings.push(Finding::new(
                shown.to_string(),
                Location::File,
                rules::UI_PATH_ESCAPE,
                "a link that resolves outside the plugin folder",
            ));
            return;
        }
        if target.is_dir() {
            // A link back to a folder already walked is a loop, not a second
            // copy.
            if self.visited.insert(target.clone()) {
                self.folder(&target, rel);
            }
            return;
        }
        match fs::metadata(&target) {
            Ok(metadata) => self.file(rel.to_string(), target, metadata.len()),
            Err(error) => self.findings.push(read_finding(
                shown.to_string(),
                format!("a link that cannot be read: {error}"),
            )),
        }
    }

    /// List one file, and end the walk once the folder holds more than the cap.
    fn file(&mut self, rel: String, file: PathBuf, size: u64) {
        self.files.push((rel, Entry { file, size }));
        if self.stopped() {
            let found = self.files.len();
            self.findings.push(Finding::new(
                display(""),
                Location::File,
                rules::UI_CAP,
                format!("the plugin folder holds at least {found} files; the cap is {MAX_FILES}"),
            ));
        }
    }
}

fn read_finding(file: String, message: String) -> Finding {
    Finding::new(file, Location::File, rules::UI_FILE_READ, message)
}

/// A plugin a caller holds in memory: the embedded reference plugin, the
/// export's file list and every test fixture that is not a folder.
pub struct MemorySource {
    paths: Vec<String>,
    files: BTreeMap<String, Vec<u8>>,
}

impl MemorySource {
    /// Sort the file list, so a source's paths read the same in every run.
    pub fn new(files: Vec<(String, Vec<u8>)>) -> Self {
        let files: BTreeMap<String, Vec<u8>> = files.into_iter().collect();
        let paths = files.keys().cloned().collect();
        MemorySource { paths, files }
    }

    /// The text form, for the reference plugin's `include_str!` files and for
    /// tests.
    pub fn from_text(files: &[(&str, &str)]) -> Self {
        MemorySource::new(
            files
                .iter()
                .map(|(path, text)| ((*path).to_string(), text.as_bytes().to_vec()))
                .collect(),
        )
    }
}

impl Source for MemorySource {
    fn paths(&self) -> &[String] {
        &self.paths
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("`{path}` is not a plugin file"))
    }

    fn size(&self, path: &str) -> u64 {
        self.files.get(path).map_or(0, |bytes| bytes.len() as u64)
    }

    fn display(&self, path: &str) -> String {
        display(path)
    }
}
