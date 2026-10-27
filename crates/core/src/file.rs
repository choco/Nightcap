//! Files the core keeps, which a crash must never leave half written.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use tempfile::NamedTempFile;

/// Writes `contents` to `path` whole or not at all: to a file beside it, then renamed over it.
/// A power cut can still leave the name on contents that never reached the disk, so this suits
/// only what is cheap to lose.
pub(crate) fn write_whole(path: &Path, contents: &[u8]) -> io::Result<()> {
    write(path, contents, false)
}

/// As `write_whole`, but synced to the disk before the rename, so a power cut leaves the old
/// contents or the new. macOS's full sync costs several milliseconds a write.
pub(crate) fn write_whole_synced(path: &Path, contents: &[u8]) -> io::Result<()> {
    write(path, contents, true)
}

fn write(path: &Path, contents: &[u8], synced: bool) -> io::Result<()> {
    let folder = path
        .parent()
        .ok_or_else(|| io::Error::other(format!("{} has no folder", path.display())))?;
    fs::create_dir_all(folder)?;
    let mut file = NamedTempFile::new_in(folder)?;
    file.write_all(contents)?;
    if synced {
        file.as_file().sync_all()?;
    }
    file.persist(path)?;
    Ok(())
}
