//! A trial (ADR-0016): what the places found as it began, which waits for its end and is all
//! `state.json` and `rules.toml` keep meanwhile, and the rules the user adds during it, which
//! last only while it runs.

use nightcap_protocol::{OnOff, RuleIdentity};

use crate::rules::places::Kept;

/// A trial that runs (ADR-0016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Trial {
    /// The ad-hoc rules and every other rule as the trial found them, which its end puts back,
    /// short of what ended meanwhile.
    found: Kept,
    /// The rules the user added during the trial, which it keeps instead of ad-hoc ones.
    rules: Vec<(RuleIdentity, OnOff)>,
    /// Whether it has Night Shift on, standing in for the schedule.
    stands_in: bool,
}

impl Trial {
    /// A trial that finds `found`, standing in for the schedule if `stands_in`.
    pub(crate) fn new(found: Kept, stands_in: bool) -> Self {
        Self {
            found,
            rules: Vec::new(),
            stands_in,
        }
    }

    /// What it found, which `state.json` and `rules.toml` keep while it runs.
    pub(crate) fn found(&self) -> &Kept {
        &self.found
    }

    /// What it found, for expiry, an edited schedule and graduation to reach as they reach the
    /// live rules.
    pub(crate) fn found_mut(&mut self) -> &mut Kept {
        &mut self.found
    }

    /// Its end, which gives back what it found.
    pub(crate) fn end(self) -> Kept {
        self.found
    }

    /// Whether it has Night Shift on in place of the schedule.
    pub(crate) fn stands_in(&self) -> bool {
        self.stands_in
    }

    /// The rules the user added during it.
    pub(crate) fn rules(&self) -> &[(RuleIdentity, OnOff)] {
        &self.rules
    }

    /// Makes its rule for `identity` say `says`.
    pub(crate) fn set_rule(&mut self, identity: &RuleIdentity, says: OnOff) {
        self.rules.retain(|(rule, _)| rule != identity);
        self.rules.push((identity.clone(), says));
    }

    /// Removes its rule for `identity`, and says whether it had one.
    pub(crate) fn remove_rule(&mut self, identity: &RuleIdentity) -> bool {
        let before = self.rules.len();
        self.rules.retain(|(rule, _)| rule != identity);
        self.rules.len() < before
    }

    /// Drops its rules for the identities in `config`, which have graduated (ADR-0007).
    pub(crate) fn graduate(&mut self, config: &[(RuleIdentity, OnOff)]) {
        self.rules
            .retain(|(identity, _)| !config.iter().any(|(each, _)| each == identity));
    }
}
