//! What of the core's state survives restarts, in `$XDG_STATE_HOME/<instance>/state.json`
//! (ADR-0008). Readers tolerate unknown keys, so only a breaking change bumps the version.

use std::io;
use std::sync::Arc;

use jiff::Timestamp;
use nightcap_protocol::{CheckKind, Length, OnOff, Place, Rule, SetBy};
use serde::{Deserialize, Serialize};

use crate::storage::config::NightShiftConfig;
use crate::storage::file::{self, Files, Owned};

/// Version 2 keeps rules (ADR-0018). Version 1 kept each place's hold and timer, which a load
/// translates.
const VERSION: u32 = 2;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Persisted {
    /// Every rule but the indefinite ones at apps and sites, which the config and `rules.toml`
    /// keep (ADR-0018).
    pub(crate) rules: Vec<Rule>,
    /// The place an outside change set a rule until the schedule turns at together with
    /// everywhere's, so taking off either takes off both.
    pub(crate) linked: Option<Place>,
    /// Website checking as last chosen in the app or the CLI (ADR-0012).
    pub(crate) websites: bool,
    /// The last onboarding version completed; a later one shows only its new steps.
    pub(crate) onboarding: u32,
    /// Whether the user wants Nightcap's notifications: set once they're allowed, and cleared
    /// only when the user accepts them off. The notifications check compares against it.
    pub(crate) notifications_wanted: bool,
    /// Whether the user wants Nightcap to open at login: set when they choose it or the login
    /// item turns up enabled, cleared only when they choose otherwise (ADR-0011).
    pub(crate) open_at_login: bool,
    /// The problems already announced with a notification, so a restart doesn't repeat them.
    pub(crate) announced: Vec<CheckKind>,
    /// What Nightcap was holding Night Shift at: forced off before a crash, or released on a
    /// clean exit. The next run finds Night Shift there and knows it wasn't the user.
    pub(crate) night_shift: Option<OnOff>,
    /// The schedule transition that ends `night_shift`, if one was coming: macOS applies its own
    /// schedule there.
    pub(crate) until: Option<Timestamp>,
    /// The strength and schedule last applied from the config: a config value applies again
    /// only once it differs (ADR-0014).
    pub(crate) applied_from_config: NightShiftConfig,
    /// The user accepted the schedule off while macOS has sunset to sunrise set aside, which
    /// the Night Shift check compares against. Cleared once it isn't set aside.
    pub(crate) schedule_off_accepted: bool,
}

impl Persisted {
    /// Drops what ended while Nightcap wasn't running.
    pub(crate) fn expire(&mut self, now: Timestamp) {
        if self.until.is_some_and(|until| until <= now) {
            self.night_shift = None;
            self.until = None;
        }
        self.rules
            .retain(|rule| rule.length.end().is_none_or(|end| end > now));
    }
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(flatten)]
    state: Persisted,
    /// Version 1's holds and timers, read only to translate them.
    #[serde(default, skip_serializing)]
    layers: Vec<FirstLayers>,
}

/// A place's hold and timer, as version 1 kept them.
#[derive(Deserialize)]
struct FirstLayers {
    place: Place,
    hold: Option<FirstLayer>,
    timer: Option<FirstLayer>,
}

/// On or off at a place until a time, or a hold with no end, with the place an outside change
/// held beside it.
#[derive(Deserialize)]
struct FirstLayer {
    state: OnOff,
    until: Option<Timestamp>,
    #[serde(default)]
    with: Option<Place>,
}

/// Version 1's holds and timers as rules. Version 1 didn't keep who set a hold, so one is the
/// user's unless an outside change linked it or only Adoption could have made it: an app's or
/// site's with no end.
fn translated(first: Vec<FirstLayers>) -> (Vec<Rule>, Option<Place>) {
    let mut rules = Vec::new();
    let mut links = Vec::new();
    for FirstLayers { place, hold, timer } in first {
        if let Some(FirstLayer {
            state,
            until: Some(until),
            ..
        }) = timer
        {
            rules.push(Rule {
                place: place.clone(),
                says: state,
                length: Length::Until { until },
                set_by: SetBy::You,
            });
        }
        if let Some(hold) = hold {
            let switched =
                place == Place::Everywhere && hold.until.is_none() && hold.with.is_none();
            let adopted = hold.with.is_some() || (hold.until.is_none() && !switched);
            if let Some(with) = hold.with {
                links.push((place.clone(), with));
            }
            rules.push(Rule {
                place,
                says: hold.state,
                length: if switched {
                    Length::Indefinite
                } else {
                    Length::UntilScheduleTurns { until: hold.until }
                },
                set_by: if adopted {
                    SetBy::OutsideChange
                } else {
                    SetBy::You
                },
            });
        }
    }
    let linked = links.iter().find_map(|(place, with)| {
        (*with == Place::Everywhere && links.contains(&(Place::Everywhere, place.clone())))
            .then(|| place.clone())
    });
    (rules, linked)
}

pub(crate) struct Store {
    files: Arc<dyn Files>,
}

impl Store {
    pub(crate) fn new(files: Arc<dyn Files>) -> Self {
        Self { files }
    }

    /// A missing, unreadable or newer file starts fresh: this state only carries intent
    /// that is cheap to lose.
    pub(crate) fn load(&self) -> Persisted {
        let path = self.files.path(file::File::State);
        let bytes = match self.files.read(file::File::State) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Persisted::default(),
            Err(error) => {
                tracing::warn!("couldn't read {}: {error}", path.display());
                return Persisted::default();
            }
        };
        match serde_json::from_slice::<File>(&bytes) {
            Ok(file) if file.version == 1 => {
                let (rules, linked) = translated(file.layers);
                Persisted {
                    rules,
                    linked,
                    ..file.state
                }
            }
            Ok(file) if file.version <= VERSION => file.state,
            Ok(file) => {
                tracing::warn!(
                    "{} is version {}, newer than this build's {VERSION}; starting fresh",
                    path.display(),
                    file.version
                );
                Persisted::default()
            }
            Err(error) => {
                tracing::warn!("couldn't parse {}: {error}", path.display());
                Persisted::default()
            }
        }
    }

    pub(crate) fn save(&self, state: &Persisted) -> io::Result<()> {
        let file = File {
            version: VERSION,
            state: state.clone(),
            layers: Vec::new(),
        };
        self.files
            .write(Owned::State, &serde_json::to_vec_pretty(&file)?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use nightcap_protocol::{Schedule, TimeOfDay};

    use super::*;
    use crate::storage::file::{Memory, Paths};

    /// Files in memory, with `state.json` holding `text`.
    fn holding(text: Option<&str>) -> Arc<Memory> {
        let files = Arc::new(Memory::new(Paths::in_folder(Path::new("/nightcap"))));
        files.put(file::File::State, text.map(Vec::from));
        files
    }

    fn at(instant: &str) -> Timestamp {
        instant.parse().unwrap()
    }

    fn figma() -> Place {
        Place::App("com.figma.Desktop".parse().unwrap())
    }

    fn rule(place: Place, says: OnOff, length: Length, set_by: SetBy) -> Rule {
        Rule {
            place,
            says,
            length,
            set_by,
        }
    }

    #[test]
    fn saved_state_loads_back() {
        let store = Store::new(holding(None));
        let morning = Length::UntilScheduleTurns {
            until: Some(at("2026-10-17T07:00:00+02:00")),
        };
        let state = Persisted {
            rules: vec![
                rule(figma(), OnOff::On, morning, SetBy::OutsideChange),
                rule(
                    Place::Everywhere,
                    OnOff::Off,
                    Length::Until {
                        until: at("2026-10-17T00:00:00+02:00"),
                    },
                    SetBy::You,
                ),
                rule(Place::Everywhere, OnOff::On, morning, SetBy::OutsideChange),
            ],
            linked: Some(figma()),
            websites: true,
            onboarding: 1,
            notifications_wanted: true,
            open_at_login: true,
            announced: vec![CheckKind::Accessibility],
            night_shift: Some(OnOff::Off),
            until: Some(at("2026-10-17T07:00:00+02:00")),
            applied_from_config: NightShiftConfig {
                strength: Some(40),
                schedule: Some(Schedule::Custom {
                    from: TimeOfDay {
                        hour: 22,
                        minute: 0,
                    },
                    to: TimeOfDay { hour: 7, minute: 0 },
                }),
            },
            schedule_off_accepted: true,
        };

        store.save(&state).unwrap();

        assert_eq!(store.load(), state);
    }

    #[test]
    fn version_1s_holds_and_timers_load_as_rules() {
        let files = holding(Some(
            r#"{"version":1,"layers":[{"place":{"app":"com.figma.Desktop"},"hold":{"state":"on","until":"2026-10-17T05:00:00Z","with":"everywhere"},"timer":{"state":"off","until":"2026-10-16T22:00:00Z"}},{"place":"everywhere","hold":{"state":"on","until":"2026-10-17T05:00:00Z","with":{"app":"com.figma.Desktop"}},"timer":null},{"place":{"domain":"figma.com"},"hold":{"state":"on","until":null},"timer":null},{"place":{"app":"com.apple.mail"},"hold":{"state":"off","until":"2026-10-16T20:00:00Z"},"timer":null}],"websites":true}"#,
        ));

        let state = Store::new(files).load();

        let morning = Length::UntilScheduleTurns {
            until: Some(at("2026-10-17T05:00:00Z")),
        };
        assert_eq!(
            state.rules,
            vec![
                rule(
                    figma(),
                    OnOff::Off,
                    Length::Until {
                        until: at("2026-10-16T22:00:00Z")
                    },
                    SetBy::You
                ),
                rule(figma(), OnOff::On, morning, SetBy::OutsideChange),
                rule(Place::Everywhere, OnOff::On, morning, SetBy::OutsideChange),
                rule(
                    Place::Domain("figma.com".parse().unwrap()),
                    OnOff::On,
                    Length::UntilScheduleTurns { until: None },
                    SetBy::OutsideChange
                ),
                rule(
                    Place::App("com.apple.mail".parse().unwrap()),
                    OnOff::Off,
                    Length::UntilScheduleTurns {
                        until: Some(at("2026-10-16T20:00:00Z"))
                    },
                    SetBy::You
                ),
            ]
        );
        assert_eq!(state.linked, Some(figma()));
        assert!(state.websites);
    }

    #[test]
    fn version_1s_switch_loads_as_everywheres_indefinite_rule() {
        let files = holding(Some(
            r#"{"version":1,"layers":[{"place":"everywhere","hold":{"state":"off","until":null},"timer":null}]}"#,
        ));

        let state = Store::new(files).load();

        assert_eq!(
            state.rules,
            vec![rule(
                Place::Everywhere,
                OnOff::Off,
                Length::Indefinite,
                SetBy::You
            )]
        );
        assert_eq!(state.linked, None);
    }

    #[test]
    fn state_from_an_earlier_build_loads_without_its_manual_state_snoozes_or_timed_disable() {
        let files = holding(Some(
            r#"{"version":1,"manual_state":"off","snoozes":["app:com.figma.Desktop"],"timed_disable":"2026-10-16T22:00:00Z","websites":true,"onboarding":1}"#,
        ));

        let state = Store::new(files).load();

        assert_eq!(state.rules, Vec::new());
        assert!(state.websites);
        assert_eq!(state.onboarding, 1);
    }

    #[test]
    fn missing_state_starts_fresh() {
        assert_eq!(Store::new(holding(None)).load(), Persisted::default());
    }

    #[test]
    fn state_from_a_newer_build_starts_fresh() {
        let files = holding(Some(r#"{"version":3,"login_item_registered":true}"#));

        assert_eq!(Store::new(files).load(), Persisted::default());
    }

    #[test]
    fn state_from_a_build_that_kept_what_the_app_was_told_still_loads() {
        let files = holding(Some(
            r#"{"version":2,"websites":true,"onboarding":1,"announced":["accessibility"],"last_version":"1.2.0"}"#,
        ));

        let state = Store::new(files).load();

        assert!(state.websites);
        assert_eq!(state.onboarding, 1);
    }

    #[test]
    fn state_with_keys_this_build_doesnt_know_still_loads() {
        let files = holding(Some(
            r#"{"version":2,"login_item_registered":true,"websites":true}"#,
        ));

        assert!(Store::new(files).load().websites);
    }
}
