//! The CLI's requests (ADR-0010): commands, an update check and a reload, which the socket's
//! thread puts into the core's mailbox, and the queries and an export, which it answers from the
//! snapshot.

use nightcap_protocol::{Payload, Refusal, Request, Response};

use crate::server::snapshot::Snapshot;
use crate::server::state::Inner;
use crate::{Effect, Outcome};

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
                Err(refusal) => Response::Refused(refusal),
            },
            // The socket's thread answers queries from the snapshot, which says the same.
            Request::Query(query) => Snapshot::of(self).answer(query),
            Request::Update => {
                if self.manages_itself {
                    tracing::info!("checking for updates");
                    effects.push(Effect::CheckForUpdates);
                    return Response::Ok(Payload::UpdateCheckStarted);
                }
                Response::Refused(Refusal::NoUpdater(self.not_self_managed()))
            }
            Request::Reload => match self.reload(effects) {
                Ok(()) => Response::Ok(Payload::Rules(self.every_rule(self.clock.now()))),
                Err(refusal) => Response::Refused(refusal),
            },
        }
    }
}
