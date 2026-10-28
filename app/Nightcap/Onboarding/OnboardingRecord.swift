import Foundation
import NightcapCore

/// The onboarding version the user went through, which the app keeps in `defaults`, and whether
/// onboarding opens as the launch's outcome is carried out.
struct OnboardingRecord {
    /// The current onboarding. Night Shift's part comes first: on a schedule already, the user
    /// keeps it or reviews it; otherwise how warm and when, each unless the config sets it or the
    /// Mac can't (ADR-0014). Then Nightcap's: rules for the suggested apps on this Mac; website
    /// checking, with the Accessibility grant and rules for the suggested sites, unless the config
    /// sets it (ADR-0012); notifications while they aren't allowed, and opening at login where
    /// Nightcap opens itself (ADR-0011). Ready sums it up, and the moon is pointed out once it
    /// closes.
    static let current = 1

    let defaults: UserDefaults

    private static let key = "onboardingCompleted"

    /// The version the user last went through, 0 for none.
    var completed: Int { defaults.integer(forKey: Self.key) }

    /// Whether the user went through the current onboarding, or the config and the install
    /// answered all it would ask.
    var finished: Bool { completed >= Self.current }

    /// Whether onboarding opens, as the launch's outcome leaves `snapshot`: unless the user went
    /// through the current one, or the config and the install answer all it would ask, which it
    /// records as gone through. The config can answer website checking and Night Shift's steps;
    /// opening at login, which a self-managed release asks, it can't.
    func launched(_ snapshot: Snapshot) -> Bool {
        guard !finished else { return false }
        let setup = OnboardingFlow.Setup(snapshot)
        let answered =
            setup.websitesFromConfig != nil && !setup.askStrength && !setup.askSchedule
            && !snapshot.about().managesItself
        if answered { finish() }
        return !answered
    }

    /// The user went through the current onboarding.
    func finish() {
        defaults.set(Self.current, forKey: Self.key)
    }
}
