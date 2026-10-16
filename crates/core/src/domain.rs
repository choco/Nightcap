//! Domains and hosts in the one form rules compare: lowercase ASCII, IDNs as punycode, no
//! trailing dot (ADR-0008).

use idna::AsciiDenyList;

/// `input` as a bare domain, or None when it's anything else: a URL, a host with a port, a
/// wildcard, an empty label.
pub(crate) fn normalize(input: &str) -> Option<String> {
    let input = input.strip_suffix('.').unwrap_or(input);
    let ascii = idna::domain_to_ascii_cow(input.as_bytes(), AsciiDenyList::URL).ok()?;
    let bare = !ascii.contains('*') && ascii.split('.').all(|label| !label.is_empty());
    bare.then(|| ascii.into_owned())
}

/// Whether a rule for `domain` covers `host`: the domain itself and every subdomain, on label
/// boundaries, so figma.com covers www.figma.com but not notfigma.com (ADR-0008).
pub(crate) fn covers(domain: &str, host: &str) -> bool {
    host.strip_suffix(domain)
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('.'))
}

/// The domain a new rule for `host` gets: its registrable domain, or the host itself when it
/// has no public suffix, like `localhost` (ADR-0009).
pub(crate) fn registrable(host: &str) -> String {
    psl::domain_str(host).unwrap_or(host).to_owned()
}
