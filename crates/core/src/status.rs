//! The CLI's requests (ADR-0010), which the IPC thread queues for the next tick: commands,
//! status, the rules, an export, an update check and a reload.

use nightcap_protocol::{
    ErrorCode, ErrorReply, Instance, OnOff, Payload, Permissions, Query, Request, Response,
    Schedule, Status, WebsiteChecking,
};

use crate::commands::refused;
use crate::state::State;
use crate::{Core, Effect, adhoc};

impl Core {
    pub(crate) fn answer(
        &self,
        state: &mut State,
        request: &Request,
        effects: &mut Vec<Effect>,
    ) -> Response {
        match request {
            Request::Command(command) => match self.run_command(state, command, effects) {
                Ok(decision) => Response::Ok(Payload::Decision(decision)),
                Err(error) => Response::Error(error),
            },
            Request::Query(Query::Status) => self.status(state),
            Request::Query(Query::Rules) => {
                let look = state.places.look(&state.facts.world(self.clock.now()));
                Response::Ok(Payload::Rules(
                    look.rules
                        .into_iter()
                        .map(|(rule, _)| rule.clone())
                        .collect(),
                ))
            }
            Request::Query(Query::Export(format)) => {
                match adhoc::export(&state.places.kept().ad_hoc, *format) {
                    Ok(text) => Response::Ok(Payload::Export(text)),
                    Err(error) => Response::Error(refused(
                        ErrorCode::Internal,
                        format!("couldn't write the rules as TOML: {error}"),
                    )),
                }
            }
            Request::Update => {
                if self.updates_itself() {
                    tracing::info!("checking for updates");
                    effects.push(Effect::CheckForUpdates);
                    return Response::Ok(Payload::UpdateCheckStarted);
                }
                Response::Error(ErrorReply {
                    code: ErrorCode::NoUpdater,
                    message: match self.instance {
                        Instance::Dev => "Dev builds don't update themselves; rebuild instead",
                        Instance::Release => {
                            "This install is managed by its installer, which keeps it up to date"
                        }
                    }
                    .to_owned(),
                })
            }
            Request::Reload => match self.reload(state, effects) {
                Ok(()) => self.status(state),
                Err(error) => Response::Error(error),
            },
        }
    }

    pub(crate) fn status(&self, state: &State) -> Response {
        let facts = &state.facts;
        let look = state.places.look(&facts.world(self.clock.now()));
        Response::Ok(Payload::Status(Box::new(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: self.install,
            night_shift: facts.night_shift,
            strength: state.strength,
            config_strength: state.loaded.config_night_shift.strength,
            decision: look.decision,
            schedule: facts.timeline.schedule.map(Schedule::from),
            config_schedule: state.loaded.config_night_shift.schedule,
            scheduled_state: facts
                .timeline
                .scheduled_state(facts.applied_until)
                .map(OnOff::from),
            next_transition: facts.timeline.next_transition(facts.applied_until),
            frontmost_app: facts.context.app.clone().map(String::from),
            watching: state.watching.is_watching(),
            host: facts
                .context
                .host
                .as_ref()
                .map(|host| host.as_str().to_owned()),
            rules: look
                .rules
                .into_iter()
                .map(|(rule, _)| rule.clone())
                .collect(),
            config_error: state.loaded.config_error.as_ref().map(ToString::to_string),
            ad_hoc_error: state.loaded.ad_hoc_error.as_ref().map(ToString::to_string),
            website_checking: WebsiteChecking {
                state: OnOff::from(facts.website_checking()),
                from_config: facts.config_websites.is_some(),
            },
            layers: look.layers.to_vec(),
            permissions: Permissions {
                accessibility: state.grants.accessibility == Some(true),
                notifications: state.grants.notifications,
            },
            checks: state.checks(self.clock.now()),
        })))
    }
}
