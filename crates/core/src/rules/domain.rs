//! The front tab's host, and which domains cover it (ADR-0006).

use nightcap_protocol::Domain;

/// The front browser tab's host, in the form domains take: lowercase ASCII, IDNs as punycode,
/// no trailing dot. The core never logs it (ADR-0009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Host(Domain);

impl Host {
    /// `host` in the form rules compare, or None for one no domain could cover, such as an
    /// IPv6 literal.
    pub(crate) fn new(host: &str) -> Option<Self> {
        host.parse().ok().map(Self)
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The domain a rule made from the front tab names: the host, short of a leading `www.`, so
    /// one made on docs.google.com covers that service rather than all of google.com
    /// (ADR-0017). The `www.` stays where only a top-level domain would be left, as on www.com.
    pub(crate) fn tab_domain(&self) -> Domain {
        match self.0.parent() {
            Some(parent) if self.as_str().starts_with("www.") && parent.as_str().contains('.') => {
                parent
            }
            _ => self.0.clone(),
        }
    }
}

/// Whether a rule for `domain` covers `host`: the domain itself and every subdomain, on label
/// boundaries, so figma.com covers www.figma.com but not notfigma.com (ADR-0006).
pub(crate) fn covers(domain: &Domain, host: &Host) -> bool {
    host.as_str()
        .strip_suffix(domain.as_str())
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_domain_covers_itself_and_its_subdomains_on_label_boundaries() {
        let figma: Domain = "figma.com".parse().unwrap();
        let covered = |host: &str| covers(&figma, &Host::new(host).unwrap());

        assert!(covered("figma.com"));
        assert!(covered("www.figma.com"));
        assert!(!covered("notfigma.com"));
        assert!(!covered("figma.com.example.net"));
    }
}
