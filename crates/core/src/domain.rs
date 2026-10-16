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
