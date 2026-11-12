//! The pack-snapshot helper the native authoring-filename witnesses share.
//!
//! Each integration test file is its own crate, so this is included by path
//! (`#[path = "common/pack.rs"] mod pack;`) in the tests that need it rather
//! than through `common`, whose HTTP helpers the other crates use.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Every file under `root`, keyed by its pack-relative path, so one write's
/// exact footprint (the file it lands, the bytes it changes, nothing else) is
/// comparable across the whole pack.
pub fn pack_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(root).unwrap().to_path_buf();
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}
