//! The command line: its commands and their arguments, as clap parses them.

use std::str::FromStr;
use std::time::Duration;

use clap::{Parser, Subcommand};
use jiff::SignedDuration;
use nightcap_protocol::{BundleId, Domain, IdentityError, Place, RuleIdentity};

use crate::wording;

#[derive(Parser)]
#[command(
    name = "nightcap",
    version,
    about = "Control Nightcap from the command line"
)]
pub(crate) struct Cli {
    /// Talk to the dev build instead of the installed app.
    #[arg(long, global = true)]
    pub(crate) dev: bool,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Show what Nightcap is doing and why.
    Status {
        /// Print the status as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Turn Night Shift on. Alone, it's the switch: on everywhere, with the schedule if the
    /// schedule has it on, else until you turn it off. With a length, a time or --until-schedule,
    /// it's on that long, everywhere or in one app or on one site, over any rule there.
    On(Change),
    /// Turn Night Shift off, as `on` turns it on.
    Off(Change),
    /// Turn Night Shift the other way everywhere, until you turn it back.
    Toggle,
    /// Take off what you set for a place, everywhere unless you name an app or a site: its rule
    /// comes back, or for everywhere the schedule.
    Reset {
        #[command(flatten)]
        place: Where,
    },
    /// Show how warm Night Shift tints the display, or set it, from 0 to 100.
    Strength {
        /// 0 is the least warm, 100 the warmest.
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        value: Option<u8>,
    },
    /// Check for an update now; Nightcap shows what it finds and offers to install it.
    Update,
    /// Turn website checking on or off: domain rules need it, and it needs Accessibility.
    Websites {
        #[arg(value_enum)]
        state: Switch,
    },
    /// List, add or remove rules, or export the ad-hoc ones for the config.
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
    /// Read the config again, after changing it.
    Reload {
        /// Keep trying this long for Nightcap to start, e.g. 5s.
        #[arg(long, value_parser = parse_duration)]
        wait: Option<Duration>,
    },
}

/// How long, and where: `on` or `off` alone is the switch, everywhere until turned back.
#[derive(clap::Args)]
pub(crate) struct Change {
    /// For this long, e.g. 30m, 1h or 1h30m.
    #[arg(long = "for", value_parser = parse_length, group = "how_long")]
    pub(crate) length: Option<SignedDuration>,
    /// Until this time of day, e.g. 23:30: today, or tomorrow once it has passed.
    #[arg(long, value_parser = parse_time_of_day, group = "how_long")]
    pub(crate) until: Option<jiff::civil::Time>,
    /// Until the schedule next turns it back: when it ends for an on, when it starts for an off.
    #[arg(long, group = "how_long")]
    pub(crate) until_schedule: bool,
    #[command(flatten)]
    pub(crate) place: Where,
}

/// An app or a site; everywhere when neither is given.
#[derive(clap::Args)]
pub(crate) struct Where {
    /// An app, by its bundle ID, e.g. com.figma.Desktop.
    #[arg(long, group = "where", value_parser = identified::<BundleId>)]
    app: Option<BundleId>,
    /// A site and its subdomains, e.g. figma.com.
    #[arg(long, group = "where", value_parser = identified::<Domain>)]
    domain: Option<Domain>,
}

impl Where {
    pub(crate) fn place(&self) -> Place {
        match (&self.app, &self.domain) {
            (Some(bundle_id), _) => Place::App(bundle_id.clone()),
            (None, Some(domain)) => Place::Domain(domain.clone()),
            (None, None) => Place::Everywhere,
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum RulesAction {
    /// Show every rule and where it comes from.
    List {
        /// Print the rules as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Add an ad-hoc rule, e.g. app:com.figma.Desktop or domain:figma.com, that turns Night Shift
    /// off there, or on with --on. It replaces your own rule for the same app or site.
    Add {
        #[arg(value_parser = identified::<RuleIdentity>)]
        identity: RuleIdentity,
        /// Keep Night Shift on there, even by day.
        #[arg(long)]
        on: bool,
    },
    /// Remove an ad-hoc rule; a config rule changes only in the config.
    Remove {
        #[arg(value_parser = identified::<RuleIdentity>)]
        identity: RuleIdentity,
    },
    /// Print the ad-hoc rules, ready to paste into the config.
    Export {
        #[arg(long, value_enum)]
        format: Format,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum Switch {
    On,
    Off,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum Format {
    Nix,
    Toml,
}

/// An app, a site or a rule's identity, or why the text names none, in the CLI's words.
fn identified<T: FromStr<Err = IdentityError>>(text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|error| wording::identity_error(&error))
}

/// A length such as 30m, 1h or 1h30m, longer than zero.
fn parse_length(text: &str) -> Result<SignedDuration, String> {
    let length: SignedDuration = text.parse().map_err(|_| wording::not_a_length(text))?;
    if length <= SignedDuration::ZERO {
        return Err(wording::not_longer_than_zero());
    }
    Ok(length)
}

/// A time of day such as 23:30.
fn parse_time_of_day(text: &str) -> Result<jiff::civil::Time, String> {
    jiff::civil::Time::strptime("%H:%M", text).map_err(|_| wording::not_a_time_of_day(text))
}

/// A wait such as 5s, zero or longer.
fn parse_duration(text: &str) -> Result<Duration, String> {
    text.parse::<SignedDuration>()
        .ok()
        .and_then(|duration| Duration::try_from(duration).ok())
        .ok_or_else(|| wording::not_a_wait(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What clap says of a line of arguments it refuses.
    fn refused(args: &[&str]) -> String {
        match Cli::try_parse_from(["nightcap"].iter().chain(args)) {
            Ok(_) => panic!("{args:?} parsed"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn lengths_read_as_30m_1h_or_1h30m() {
        assert_eq!(parse_length("30m"), Ok(SignedDuration::from_mins(30)));
        assert_eq!(parse_length("1h"), Ok(SignedDuration::from_hours(1)));
        assert_eq!(parse_length("1h30m"), Ok(SignedDuration::from_mins(90)));
        assert_eq!(
            parse_length("0m"),
            Err("a length is longer than zero".to_owned())
        );
        assert_eq!(
            parse_length("soon"),
            Err("\"soon\" isn't a length like 30m, 1h or 1h30m".to_owned())
        );
    }

    #[test]
    fn a_wait_is_zero_or_longer() {
        assert_eq!(parse_duration("5s"), Ok(Duration::from_secs(5)));
        assert_eq!(parse_duration("0s"), Ok(Duration::ZERO));
        assert_eq!(
            parse_duration("-5s"),
            Err("\"-5s\" isn't a wait like 5s or 1m".to_owned())
        );
    }

    #[test]
    fn an_app_a_site_or_a_rule_that_cant_be_one_is_said_in_the_clis_words() {
        for (args, said) in [
            (
                &["rules", "add", "figma"][..],
                "\"figma\" names no app or site: write it like app:com.figma.Desktop or domain:figma.com",
            ),
            (
                &["rules", "remove", "app:com.figma.Desktop/"],
                "\"com.figma.Desktop/\" isn't a bundle ID like com.figma.Desktop: only letters, digits, hyphens and periods",
            ),
            (
                &["on", "--app", "com.figma.Desktop ", "--for", "1h"],
                "\"com.figma.Desktop \" isn't a bundle ID like com.figma.Desktop: only letters, digits, hyphens and periods",
            ),
            (
                &["off", "--domain", "figma.com/file", "--for", "1h"],
                "\"figma.com/file\" isn't a domain like figma.com: no scheme, path, port or wildcard",
            ),
        ] {
            let error = refused(args);
            assert!(error.contains(said), "{args:?}: {error}");
        }
    }
}
