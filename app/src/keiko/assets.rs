//! The Keiko asset reader: Bevy's file reader with the two asset faults applied
//! (spec §4.2, §7.2).
//!
//! It is installed only by the Keiko composition, so the shipping player never
//! gains a fault hook. Assets are still read in place from the original pack and
//! never copied or written.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::io::file::FileAssetReader;
use bevy::asset::io::{
    AssetReader, AssetReaderError, PathStream, Reader, VecReader,
};

/// The asset faults a take declared, as pack-relative paths.
#[derive(Debug, Default, Clone)]
pub struct AssetFaults {
    pub missing: HashSet<String>,
    pub corrupt: HashSet<String>,
}

impl AssetFaults {
    /// Read the faults out of a case's headers.
    pub fn of<'a>(faults: impl IntoIterator<Item = &'a unmei_engine::keiko::Fault>) -> Self {
        let mut out = AssetFaults::default();
        for fault in faults {
            match fault {
                unmei_engine::keiko::Fault::AssetMissing { path } => {
                    out.missing.insert(normalize(path));
                }
                unmei_engine::keiko::Fault::AssetCorrupt { path } => {
                    out.corrupt.insert(normalize(path));
                }
                _ => {}
            }
        }
        out
    }
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/")
}

/// A file-backed asset reader that can pretend one file is absent or its bytes
/// are undecodable.
pub struct KeikoAssetReader {
    inner: FileAssetReader,
    faults: Arc<AssetFaults>,
}

impl KeikoAssetReader {
    pub fn new(root: PathBuf, faults: Arc<AssetFaults>) -> Self {
        Self {
            inner: FileAssetReader::new(root),
            faults,
        }
    }
}

impl AssetReader for KeikoAssetReader {
    async fn read<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        let key = normalize(&path.to_string_lossy());
        if self.faults.missing.contains(&key) {
            return Err(AssetReaderError::NotFound(path.to_path_buf()));
        }
        if self.faults.corrupt.contains(&key) {
            return Ok(Box::new(VecReader::new(b"keiko asset-corrupt".to_vec())));
        }
        let reader = self.inner.read(path).await?;
        Ok(Box::new(reader))
    }

    async fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        let reader = self.inner.read_meta(path).await?;
        Ok(Box::new(reader))
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        self.inner.read_directory(path).await
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        self.inner.is_directory(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fault_names_a_pack_relative_asset() {
        let faults = AssetFaults::of(&[
            unmei_engine::keiko::Fault::AssetMissing {
                path: "backgrounds/a.png".to_string(),
            },
            unmei_engine::keiko::Fault::AssetCorrupt {
                path: "sprites/a/b.png".to_string(),
            },
        ]);
        assert!(faults.missing.contains("backgrounds/a.png"));
        assert!(faults.corrupt.contains("sprites/a/b.png"));
    }
}
