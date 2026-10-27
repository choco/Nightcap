//! The files the core reads and keeps. Their stores read and write them here as bytes, so
//! parsing, versions and graduation are the same whatever holds them.

#[cfg(any(test, debug_assertions))]
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
#[cfg(any(test, debug_assertions))]
use std::sync::{Mutex, MutexGuard};

use etcetera::BaseStrategy;
use nightcap_protocol::Instance;
use tempfile::NamedTempFile;

use crate::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum File {
    /// `config.toml`: the config rules, which the user or their installer writes (ADR-0007).
    Config,
    /// `rules.toml`: the ad-hoc rules.
    Rules,
    /// `state.json`: what survives a restart (ADR-0008).
    State,
}

/// A file the core writes: every one but the config (ADR-0007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owned {
    Rules,
    State,
}

impl From<Owned> for File {
    fn from(file: Owned) -> Self {
        match file {
            Owned::Rules => Self::Rules,
            Owned::State => Self::State,
        }
    }
}

/// Where the core's files are kept.
pub(crate) trait Files: Send + Sync {
    /// The file's contents, or None where there's no such file.
    fn read(&self, file: File) -> io::Result<Option<Vec<u8>>>;
    /// Replaces the file whole, or leaves it as it was.
    fn write(&self, file: Owned, contents: &[u8]) -> io::Result<()>;
    /// Where the file is, for Settings and messages.
    fn path(&self, file: File) -> PathBuf;
}

/// Where an instance's files are: under its folder in each XDG base directory.
#[derive(Debug, Clone)]
pub(crate) struct Paths {
    config: PathBuf,
    rules: PathBuf,
    state: PathBuf,
}

impl Paths {
    pub(crate) fn xdg(instance: Instance) -> Result<Self, CoreError> {
        let xdg = etcetera::base_strategy::Xdg::new().map_err(|error| CoreError::Io {
            message: format!("can't find the XDG base directories: {error}"),
        })?;
        let state = xdg.state_dir().ok_or_else(|| CoreError::Io {
            message: "can't find the XDG state directory".to_owned(),
        })?;
        let folder = instance.xdg_folder_name();
        Ok(Self {
            config: xdg.config_dir().join(folder).join("config.toml"),
            rules: xdg.data_dir().join(folder).join("rules.toml"),
            state: state.join(folder).join("state.json"),
        })
    }

    /// Every file straight in `folder`.
    #[cfg(test)]
    pub(crate) fn in_folder(folder: &Path) -> Self {
        Self {
            config: folder.join("config.toml"),
            rules: folder.join("rules.toml"),
            state: folder.join("state.json"),
        }
    }

    fn of(&self, file: File) -> &Path {
        match file {
            File::Config => &self.config,
            File::Rules => &self.rules,
            File::State => &self.state,
        }
    }
}

/// The files on disk.
#[derive(Debug)]
pub(crate) struct Disk(pub(crate) Paths);

impl Files for Disk {
    fn read(&self, file: File) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.0.of(file)) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Writes to a file beside it, then renames that over it. `rules.toml` is synced to the disk
    /// before the rename, so a power cut leaves the old rules or the new; macOS's full sync costs
    /// several milliseconds a write. `state.json` isn't: a power cut can leave its name on
    /// contents that never reached the disk, which only costs intent that is cheap to lose.
    fn write(&self, file: Owned, contents: &[u8]) -> io::Result<()> {
        let path = self.0.of(file.into());
        let folder = path
            .parent()
            .ok_or_else(|| io::Error::other(format!("{} has no folder", path.display())))?;
        fs::create_dir_all(folder)?;
        let mut temporary = NamedTempFile::new_in(folder)?;
        temporary.write_all(contents)?;
        if file == Owned::Rules {
            temporary.as_file().sync_all()?;
        }
        temporary.persist(path)?;
        Ok(())
    }

    fn path(&self, file: File) -> PathBuf {
        self.0.of(file).to_owned()
    }
}

/// The files in memory, for a core staged without a disk. A write never fails.
#[cfg(any(test, debug_assertions))]
#[derive(Debug)]
pub(crate) struct Memory {
    paths: Paths,
    contents: Mutex<HashMap<File, Vec<u8>>>,
}

#[cfg(any(test, debug_assertions))]
impl Memory {
    /// No files yet, at `paths`, which only Settings and messages show.
    pub(crate) fn new(paths: Paths) -> Self {
        Self {
            paths,
            contents: Mutex::default(),
        }
    }

    /// Changes `file` from outside the core, as the user or their installer would, or takes it
    /// away.
    pub(crate) fn put(&self, file: File, contents: Option<Vec<u8>>) {
        let mut files = self.contents();
        match contents {
            Some(contents) => files.insert(file, contents),
            None => files.remove(&file),
        };
    }

    #[expect(
        clippy::expect_used,
        reason = "panics abort (ADR-0002), so none can poison the lock"
    )]
    fn contents(&self) -> MutexGuard<'_, HashMap<File, Vec<u8>>> {
        self.contents
            .lock()
            .expect("no panic happens while it's held")
    }
}

#[cfg(any(test, debug_assertions))]
impl Files for Memory {
    fn read(&self, file: File) -> io::Result<Option<Vec<u8>>> {
        Ok(self.contents().get(&file).cloned())
    }

    fn write(&self, file: Owned, contents: &[u8]) -> io::Result<()> {
        self.put(file.into(), Some(contents.to_owned()));
        Ok(())
    }

    fn path(&self, file: File) -> PathBuf {
        self.paths.of(file).to_owned()
    }
}
