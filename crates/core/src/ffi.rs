//! What the shell reaches through UniFFI beside the protocol's own types: the moments the core's
//! messages carry, as the protocol lowers them, and the checks the shell runs before it sends a
//! bundle ID or a domain.

use jiff::Timestamp;
use nightcap_protocol::{BundleId, Domain};

uniffi::use_remote_type!(nightcap_protocol::Timestamp);

/// The domain a domain rule would have for what the user typed, or None when it isn't one.
#[uniffi::export]
pub fn normalize_domain(text: String) -> Option<String> {
    text.trim().parse::<Domain>().ok().map(String::from)
}

/// Whether an app rule can name this bundle ID: letters, digits, hyphens and periods. A
/// command naming any other stops the app as it reaches the core.
#[uniffi::export]
pub fn is_bundle_id(text: String) -> bool {
    text.parse::<BundleId>().is_ok()
}
