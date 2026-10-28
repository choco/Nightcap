import Foundation
import NightcapCore
import Testing

/// A staged core at noon whose user went through onboarding, after `steps`.
private func onboarded(config: String? = nil, _ steps: Step...) throws -> Core {
    try Core.staged(
        scenario: Scenario(
            now: at("2026-10-16T12:00:00+02:00"), config: config,
            steps: [.appCommand(.finishOnboarding(websites: false))] + steps))
}

/// When the app asks for Accessibility, from a staged core's snapshot after each input, as the
/// app's redraw hands them on.
struct AccessibilityAskTests {
    /// The CLI's command reaches the app only as the snapshot after its outcome, as Settings' does,
    /// so either asks alike.
    @Test func turningWebsiteCheckingOnWithoutAccessibilityAsksForItOnce() throws {
        let core = try onboarded(permissions(false, .granted))
        var ask = AccessibilityAsk(core.snapshot())

        core.send(command: .websites(.on))
        let turnedOn = ask.isDue(after: core.snapshot())
        core.report(fact: .permissionsChanged(accessibility: false, notifications: .granted))
        let after = ask.isDue(after: core.snapshot())

        #expect(turnedOn)
        #expect(!after)
    }

    @Test func turningItOnBeforeMacOSReportsTheGrantAsksForIt() throws {
        let core = try onboarded()
        var ask = AccessibilityAsk(core.snapshot())

        core.send(command: .websites(.on))
        let turnedOn = ask.isDue(after: core.snapshot())

        #expect(turnedOn)
    }

    /// The grant going missing while website checking is on is the Accessibility check's problem.
    @Test func turningItOnWithAccessibilityAllowedOrLosingTheGrantOrTurningItOffAsksNothing() throws {
        let core = try onboarded(permissions(true, .granted))
        var ask = AccessibilityAsk(core.snapshot())

        core.send(command: .websites(.on))
        let turnedOn = ask.isDue(after: core.snapshot())
        core.report(fact: .permissionsChanged(accessibility: false, notifications: .granted))
        let grantLost = ask.isDue(after: core.snapshot())
        core.send(command: .websites(.off))
        let turnedOff = ask.isDue(after: core.snapshot())

        #expect(!turnedOn)
        #expect(!grantLost)
        #expect(!turnedOff)
    }

    @Test func websiteCheckingOnAsTheAppStartsAsksNothing() throws {
        for core in try [
            onboarded(permissions(false, .granted), .command(.websites(.on))),
            onboarded(config: "websites = true\n", permissions(false, .granted)),
        ] {
            var ask = AccessibilityAsk(core.snapshot())

            core.report(fact: .launched)
            let launched = ask.isDue(after: core.snapshot())

            #expect(core.snapshot().preferences().websiteChecking.state == .on)
            #expect(!launched)
        }
    }

    /// The same core before a reload and after it, as the steps that lead to each stage it.
    @Test func websiteCheckingAReloadedConfigTurnsOnAsksNothing() throws {
        let before = try onboarded(config: "", permissions(false, .granted))
        let reloaded = try onboarded(
            config: "", permissions(false, .granted), .config("websites = true\n"), .command(.reload))
        var ask = AccessibilityAsk(before.snapshot())

        let afterTheReload = ask.isDue(after: reloaded.snapshot())

        #expect(reloaded.snapshot().preferences().websiteChecking.state == .on)
        #expect(!afterTheReload)
    }

    /// Onboarding's own page offers the grant.
    @Test func websiteCheckingOnboardingTurnsOnAsItFinishesAsksNothing() throws {
        let core = try atNoon(permissions(false, .granted))
        var ask = AccessibilityAsk(core.snapshot())

        core.sendApp(command: .finishOnboarding(websites: true))
        let finished = ask.isDue(after: core.snapshot())

        #expect(core.snapshot().preferences().websiteChecking.state == .on)
        #expect(!finished)
    }

    /// Only the CLI can, as Settings opens once onboarding is finished.
    @Test func turningItOnWhileOnboardingIsOpenAsksForIt() throws {
        let core = try atNoon(permissions(false, .granted))
        var ask = AccessibilityAsk(core.snapshot())

        core.send(command: .websites(.on))
        let turnedOn = ask.isDue(after: core.snapshot())

        #expect(turnedOn)
    }
}
