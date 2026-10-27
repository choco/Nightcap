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

/// One of the core's files.
#[cfg_attr(
    not(any(test, debug_assertions)),
    expect(
        unreachable_pub,
        reason = "only a staged core, which debug builds have, names them"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum File {
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

    /// What `file` holds, or None where there's no such file.
    pub(crate) fn get(&self, file: File) -> Option<Vec<u8>> {
        self.contents().get(&file).cloned()
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
        Ok(self.get(file))
    }

    fn write(&self, file: Owned, contents: &[u8]) -> io::Result<()> {
        self.put(file.into(), Some(contents.to_owned()));
        Ok(())
    }

    fn path(&self, file: File) -> PathBuf {
        self.paths.of(file).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use jiff::tz::TimeZone;
    use nightcap_protocol::{
        Command, ErrorCode, Install, Instance, Lasting, Length, OnOff, Place, Rule, RuleIdentity,
        Schedule, SetBy, TimeOfDay,
    };

    use super::{Disk, File, Files, Owned, Paths};
    use crate::Fact;
    use crate::scenario::ManualClock;
    use crate::server::react::{Input, react};
    use crate::server::start::Setup;
    use crate::server::state::Inner;

    fn disk(folder: &Path) -> Disk {
        Disk(Paths::in_folder(folder))
    }

    #[test]
    fn what_the_disk_keeps_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let disk = disk(dir.path());

        disk.write(Owned::Rules, b"version = 2\n").unwrap();
        disk.write(Owned::State, b"{\"version\":1}").unwrap();

        assert_eq!(
            disk.read(File::Rules).unwrap().as_deref(),
            Some(&b"version = 2\n"[..])
        );
        assert_eq!(
            disk.read(File::State).unwrap().as_deref(),
            Some(&b"{\"version\":1}"[..])
        );
    }

    #[test]
    fn a_missing_file_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(disk(dir.path()).read(File::Config).unwrap(), None);
    }

    #[test]
    fn a_write_makes_the_folder_and_replaces_the_file_whole() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("nightcap");
        let disk = disk(&folder);

        disk.write(Owned::State, b"a longer first version").unwrap();
        disk.write(Owned::State, b"second").unwrap();

        assert_eq!(
            disk.read(File::State).unwrap().as_deref(),
            Some(&b"second"[..])
        );
        let names: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["state.json"]);
    }

    #[test]
    fn a_rule_rules_toml_cant_keep_is_refused_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        // Night Shift on at 23:00 by the custom hours until 07:00, with Mail snoozed.
        let mut inner = Inner::start(Setup {
            instance: Instance::Release,
            install: Install::Managed,
            socket: None,
            files: Arc::new(disk(dir.path())),
            clock: Arc::new(ManualClock::new(
                "2026-10-16T23:00:00+02:00".parse().unwrap(),
            )),
            zone: TimeZone::get("Europe/Zurich").unwrap(),
        });
        let hours = Schedule::Custom {
            from: TimeOfDay {
                hour: 22,
                minute: 0,
            },
            to: TimeOfDay { hour: 7, minute: 0 },
        };
        react(
            &mut inner,
            Input::Fact(Fact::StatusChanged {
                enabled: true,
                schedule: hours,
                sun_schedule_set_aside: false,
            }),
        );
        let mail = RuleIdentity::App("com.apple.mail".parse().unwrap());
        let snooze = Command::Set {
            place: Place::from(mail.clone()),
            state: OnOff::Off,
            length: Lasting::For(jiff::SignedDuration::from_mins(30)),
        };
        inner.choose(snooze).1.unwrap();
        std::fs::create_dir(dir.path().join("rules.toml")).unwrap();

        let (_, answer) = inner.choose(Command::AddRule {
            identity: mail.clone(),
            says: OnOff::Off,
        });

        assert_eq!(answer.unwrap_err().code, ErrorCode::AdHocRulesOff);
        let status = inner.status();
        assert_eq!(
            status.rules,
            vec![Rule {
                place: Place::from(mail),
                says: OnOff::Off,
                length: Length::Until {
                    until: "2026-10-16T23:30:00+02:00".parse().unwrap(),
                },
                set_by: SetBy::You,
            }]
        );
    }
}
