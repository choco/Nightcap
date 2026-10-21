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

/// The domain a rule made from the front tab names: its host, short of a leading `www.`, so one
/// made on docs.google.com covers that service rather than all of google.com (ADR-0023). The
/// `www.` stays where only a top-level domain would be left, as on www.com.
pub(crate) fn of_tab(host: &str) -> String {
    host.strip_prefix("www.")
        .filter(|rest| rest.contains('.'))
        .unwrap_or(host)
        .to_owned()
}
