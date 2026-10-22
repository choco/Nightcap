//! The core logs to the system log under the app's subsystem, beside the shell (ADR-0009).
//! `tracing-oslog` writes every message as public, so nothing logged here names a host.

#[cfg(target_os = "macos")]
pub(crate) fn init(subsystem: &str) {
    use std::sync::Once;
    use tracing_subscriber::layer::SubscriberExt;

    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let subscriber =
            tracing_subscriber::registry().with(tracing_oslog::OsLogger::new(subsystem, "core"));
        tracing::subscriber::set_global_default(subscriber)
            .expect("nothing else installs a global subscriber");

        // Panics abort (they can't cross into Swift), so this is the only trace one leaves.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            tracing::error!("panic: {info}");
            previous(info);
        }));
    });
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn init(_subsystem: &str) {}
