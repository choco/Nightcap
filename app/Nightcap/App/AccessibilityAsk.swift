import NightcapCore

/// When the app asks for Accessibility, which website checking needs (ADR-0012): as the snapshot
/// after an outcome shows website checking turned on without the grant, whoever turned it on,
/// Settings or the CLI. Website checking the config sets, at launch or on a reload, isn't asked
/// for, nor the website checking onboarding turns on as it finishes, whose own page offers the
/// grant.
struct AccessibilityAsk {
    /// What decides whether to ask, as one snapshot shows it.
    private struct Seen {
        /// Website checking as the user chose it, or nil while the config sets it.
        let chosen: OnOff?
        let onboardingFinished: Bool

        init(_ snapshot: Snapshot) {
            let preferences = snapshot.preferences()
            let websites = preferences.websiteChecking
            chosen = websites.fromConfig ? nil : websites.state
            onboardingFinished = preferences.onboardingFinished
        }
    }

    private var seen: Seen

    /// From `snapshot`, the core as the app starts it, which asks for nothing itself.
    init(_ snapshot: Snapshot) {
        seen = Seen(snapshot)
    }

    /// Whether to ask now that `snapshot` is the latest, which this then compares the next with.
    mutating func isDue(after snapshot: Snapshot) -> Bool {
        let before = seen
        seen = Seen(snapshot)
        return before.chosen == .off && seen.chosen == .on && before.onboardingFinished == seen.onboardingFinished
            && !snapshot.checks().permissions.accessibility
    }
}
