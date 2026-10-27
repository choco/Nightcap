//! The CLI's requests (ADR-0010), which the IPC thread queues for the next tick: commands,
//! status, the rules, an export, an update check and a reload.

use nightcap_protocol::{
    ErrorCode, ErrorReply, Instance, OnOff, Payload, Permissions, Query, Request, Response,
    Schedule, Status, WebsiteChecking,
};

use crate::commands::refused;
use crate::state::Inner;
use crate::{Effect, Outcome, adhoc};

impl Inner {
    /// Answers a CLI request in a turn of its own. A problem it causes isn't announced: the CLI
    /// already printed what went wrong.
    pub(crate) fn answer(&mut self, request: &Request) -> (Outcome, Response) {
        self.turn(false, |inner, outcome| {
            inner.respond(request, &mut outcome.effects)
        })
    }

    fn respond(&mut self, request: &Request, effects: &mut Vec<Effect>) -> Response {
        match request {
            Request::Command(command) => match self.run_command(command, effects) {
                Ok(decision) => Response::Ok(Payload::Decision(decision)),
                Err(error) => Response::Error(error),
            },
            Request::Query(Query::Status) => self.status(),
            Request::Query(Query::Rules) => {
                let look = self.places.look(&self.facts.world(self.clock.now()));
                Response::Ok(Payload::Rules(
                    look.rules
                        .into_iter()
                        .map(|(rule, _)| rule.clone())
                        .collect(),
                ))
            }
            Request::Query(Query::Export(format)) => {
                match adhoc::export(&self.places.kept().ad_hoc, *format) {
                    Ok(text) => Response::Ok(Payload::Export(text)),
                    Err(error) => Response::Error(refused(
                        ErrorCode::Internal,
                        format!("couldn't write the rules as TOML: {error}"),
                    )),
                }
            }
            Request::Update => {
                if self.manages_itself {
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
            Request::Reload => match self.reload(effects) {
                Ok(()) => self.status(),
                Err(error) => Response::Error(error),
            },
        }
    }

    pub(crate) fn status(&self) -> Response {
        let facts = &self.facts;
        let look = self.places.look(&facts.world(self.clock.now()));
        Response::Ok(Payload::Status(Box::new(Status {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            install: self.install,
            night_shift: facts.night_shift,
            strength: self.strength,
            config_strength: self.loaded.config_night_shift.strength,
            decision: look.decision,
            schedule: facts.timeline.schedule.map(Schedule::from),
            config_schedule: self.loaded.config_night_shift.schedule,
            scheduled_state: facts
                .timeline
                .scheduled_state(facts.applied_until)
                .map(OnOff::from),
            next_transition: facts.timeline.next_transition(facts.applied_until),
            frontmost_app: facts.context.app.clone().map(String::from),
            watching: self.watching.is_watching(),
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
            config_error: self.loaded.config_error.as_ref().map(ToString::to_string),
            ad_hoc_error: self.loaded.ad_hoc_error.as_ref().map(ToString::to_string),
            website_checking: WebsiteChecking {
                state: OnOff::from(facts.website_checking()),
                from_config: facts.config_websites.is_some(),
            },
            layers: look.layers.to_vec(),
            permissions: Permissions {
                accessibility: self.grants.accessibility == Some(true),
                notifications: self.grants.notifications,
            },
            checks: self.checks(self.clock.now()),
        })))
    }
}
