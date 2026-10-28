import NightcapCore

/// When the app asks for Accessibility, which website checking needs (ADR-0012): as the snapshot
/// after an outcome shows website checking turned on without the grant, whoever turned it on,
/// Settings or the CLI. Website checking the config sets, at launch or on a reload, isn't asked
/// for, nor the website checking onboarding turns on as it finishes, whose own page offers the
/// grant.
struct AccessibilityAsk {
    /// Website checking as the user chose it, as the latest snapshot shows it, or nil while the
    /// config sets it.
    private var chosen: OnOff?
    /// Onboarding sent website checking on as it finished, while the latest snapshot had it off:
    /// the first snapshot to show it changed is onboarding's doing.
    private var awaitingOnboarding = false

    /// From `snapshot`, the core as the app starts it, which asks for nothing itself.
    init(_ snapshot: Snapshot) {
        chosen = Self.chosen(in: snapshot)
    }

    /// Onboarding finished and sent website checking on.
    mutating func onboardingTurnedOn() {
        awaitingOnboarding = chosen == .off
    }

    /// Whether to ask now that `snapshot` is the latest, which this then compares the next with.
    mutating func isDue(after snapshot: Snapshot) -> Bool {
        let before = chosen
        chosen = Self.chosen(in: snapshot)
        guard before == .off, chosen != .off else { return false }
        let onboardings = awaitingOnboarding
        awaitingOnboarding = false
        return chosen == .on && !onboardings && !snapshot.checks().permissions.accessibility
    }

    private static func chosen(in snapshot: Snapshot) -> OnOff? {
        let websites = snapshot.preferences().websiteChecking
        return websites.fromConfig ? nil : websites.state
    }
}
