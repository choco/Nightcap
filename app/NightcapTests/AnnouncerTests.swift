import Foundation
import NightcapCore
import Testing

private func titles(_ notices: [Notice]) -> [String] {
    notices.map(\.title)
}

/// Website checking on, from the config, at noon with Night Shift off by the custom hours, after
/// `steps`.
private func checkingWebsites(_ steps: Step...) throws -> Core {
    try Core.staged(
        scenario: Scenario(
            now: at("2026-10-16T12:00:00+02:00"), config: "websites = true\n", steps: [reported(false)] + steps))
}

private let accessibilityNotAllowed = Fact.permissionsChanged(accessibility: false, notifications: .granted)

/// Which problems the app announces, and when, from staged cores' snapshots and outcomes' causes
/// given by hand, as the executor hands them over.
struct AnnouncerTests {
    @Test func aProblemThatAppearsOnItsOwnIsToldOnce() throws {
        try withScratchDefaults { defaults in
            let announcer = Announcer(defaults: defaults, words: .staged)
            let core = try checkingWebsites()

            core.report(fact: accessibilityNotAllowed)
            let appeared = announcer.heard(.fact, core.snapshot())
            core.report(fact: accessibilityNotAllowed)
            let again = announcer.heard(.fact, core.snapshot())

            #expect(titles(appeared) == ["Website checking needs Accessibility"])
            #expect(again.isEmpty)
        }
    }

    @Test func aProblemThatEndsAndComesBackIsToldAgain() throws {
        try withScratchDefaults { defaults in
            let announcer = Announcer(defaults: defaults, words: .staged)
            let core = try checkingWebsites(.fact(accessibilityNotAllowed))
            _ = announcer.heard(.fact, core.snapshot())

            core.report(fact: .permissionsChanged(accessibility: true, notifications: .granted))
            let ended = announcer.heard(.fact, core.snapshot())
            core.report(fact: accessibilityNotAllowed)
            let back = announcer.heard(.fact, core.snapshot())

            #expect(ended.isEmpty)
            #expect(titles(back) == ["Website checking needs Accessibility"])
        }
    }

    /// The relaunched core answers no Accessibility check until macOS first reports the grant.
    @Test func aProblemToldIsntToldAgainAfterARelaunch() throws {
        try withScratchDefaults { defaults in
            let before = try checkingWebsites(.fact(accessibilityNotAllowed))
            let told = Announcer(defaults: defaults, words: .staged).heard(.fact, before.snapshot())
            let relaunched = Announcer(defaults: defaults, words: .staged)
            let core = try checkingWebsites()

            let beforeTheGrant = relaunched.heard(.fact, core.snapshot())
            core.report(fact: accessibilityNotAllowed)
            let afterIt = relaunched.heard(.fact, core.snapshot())

            #expect(titles(told) == ["Website checking needs Accessibility"])
            #expect(beforeTheGrant.isEmpty)
            #expect(afterIt.isEmpty)
        }
    }

    /// The CLI's commands come as commands too.
    @Test func aProblemTheUserCausesWithACommandIsntToldWhileItLasts() throws {
        try withScratchDefaults { defaults in
            let announcer = Announcer(defaults: defaults, words: .staged)
            let core = try atNoon(reported(false), permissions(false, .granted))
            _ = announcer.heard(.fact, core.snapshot())

            core.send(command: .websites(.on))
            let turnedOn = announcer.heard(.command, core.snapshot())
            core.report(fact: accessibilityNotAllowed)
            let later = announcer.heard(.fact, core.snapshot())

            #expect(core.snapshot().checks().checks.contains { $0.detail == .accessibility(.notAllowed) })
            #expect(turnedOn.isEmpty)
            #expect(later.isEmpty)
        }
    }

    @Test func quittingTellsNothing() throws {
        try withScratchDefaults { defaults in
            let core = try checkingWebsites(.fact(accessibilityNotAllowed))

            #expect(Announcer(defaults: defaults, words: .staged).heard(.quit, core.snapshot()).isEmpty)
        }
    }

    @Test func aConfigBrokenAtLaunchIsTold() throws {
        try withScratchDefaults { defaults in
            let announcer = Announcer(defaults: defaults, words: .staged)
            let core = try atNoon(config: "[rules.off]\ndomian = [\"figma.com\"]\n")

            core.report(fact: .launched)

            #expect(titles(announcer.heard(.launch, core.snapshot())) == ["Nightcap’s config is invalid"])
        }
    }

    @Test func openingAtLoginRemovedInSystemSettingsIsTold() throws {
        try withScratchDefaults { defaults in
            let announcer = Announcer(defaults: defaults, words: .staged)
            let core = try selfManaged(.enabled, reported(false))
            _ = announcer.heard(.fact, core.snapshot())

            core.report(fact: .loginItemChanged(item: .notRegistered))

            #expect(titles(announcer.heard(.fact, core.snapshot())) == ["Nightcap no longer opens at login"])
        }
    }

    /// Switched-off notifications can't announce themselves, and macOS announces a login item
    /// waiting for its approval itself.
    @Test func switchedOffNotificationsAndALoginItemAwaitingApprovalAreNeverTold() throws {
        try withScratchDefaults { defaults in
            let announcer = Announcer(defaults: defaults, words: .staged)
            let notifications = try atNoon(reported(false), permissions(true, .granted))
            notifications.report(fact: .permissionsChanged(accessibility: true, notifications: .denied))
            let login = try selfManaged(.notRegistered, reported(false), .command(.openAtLogin(.on)))
            login.report(fact: .loginItemChanged(item: .requiresApproval))

            #expect(notifications.snapshot().checks().checks.contains { $0.detail == .notifications(.switchedOff) })
            #expect(login.snapshot().checks().checks.contains { $0.detail == .openAtLogin(.awaitingApproval) })
            #expect(announcer.heard(.fact, notifications.snapshot()).isEmpty)
            #expect(announcer.heard(.fact, login.snapshot()).isEmpty)
        }
    }

    /// Night Shift is unavailable until CoreBrightness first reports it, which the launch's facts
    /// do at once.
    @Test func nightShiftUnavailableIsntToldNorKeepsWhatItTurnsIntoUntold() throws {
        try withScratchDefaults { defaults in
            let announcer = Announcer(defaults: defaults, words: .staged)
            let core = try staged("2026-10-16T01:40:00+02:00")

            let unavailable = announcer.heard(.fact, core.snapshot())
            core.report(fact: .statusChanged(enabled: false, schedule: .off, sunScheduleSetAside: true))
            let setAside = announcer.heard(.fact, core.snapshot())

            #expect(unavailable.isEmpty)
            #expect(titles(setAside) == ["Sunset to sunrise needs Location Services"])
        }
    }

    /// With nothing kept, a problem standing as the app first hears the core is told as one that
    /// appears then would be.
    @Test func theFirstRunTellsAProblemStandingThenOnce() throws {
        try withScratchDefaults { defaults in
            let announcer = Announcer(defaults: defaults, words: .staged)
            let core = try atNoon(config: "websites = true\n", rules: "version = [", reported(false))

            let first = announcer.heard(.fact, core.snapshot())
            core.report(fact: .launched)
            let launched = announcer.heard(.launch, core.snapshot())

            #expect(titles(first) == ["Ad-hoc rules are off"])
            #expect(launched.isEmpty)
        }
    }
}
