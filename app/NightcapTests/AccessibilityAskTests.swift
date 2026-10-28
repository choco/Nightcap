import Foundation
import NightcapCore
import Testing

/// When the app asks for Accessibility, from a staged core's snapshot after each input, as the
/// app's redraw hands them on.
struct AccessibilityAskTests {
    /// The CLI's command reaches the app only as the snapshot after its outcome, as Settings' does,
    /// so either asks alike.
    @Test func turningWebsiteCheckingOnWithoutAccessibilityAsksForItOnce() throws {
        let core = try atNoon(permissions(false, .granted))
        var ask = AccessibilityAsk(core.snapshot())

        core.send(command: .websites(.on))
        let turnedOn = ask.isDue(after: core.snapshot())
        core.report(fact: .permissionsChanged(accessibility: false, notifications: .granted))
        let after = ask.isDue(after: core.snapshot())

        #expect(turnedOn)
        #expect(!after)
    }

    @Test func turningItOnBeforeMacOSReportsTheGrantAsksForIt() throws {
        let core = try atNoon()
        var ask = AccessibilityAsk(core.snapshot())

        core.send(command: .websites(.on))
        let turnedOn = ask.isDue(after: core.snapshot())

        #expect(turnedOn)
    }

    /// The grant going missing while website checking is on is the Accessibility check's problem.
    @Test func turningItOnWithAccessibilityAllowedOrLosingTheGrantOrTurningItOffAsksNothing() throws {
        let core = try atNoon(permissions(true, .granted))
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
            atNoon(permissions(false, .granted), .command(.websites(.on))),
            atNoon(config: "websites = true\n", permissions(false, .granted)),
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
        let before = try atNoon(config: "", permissions(false, .granted))
        let reloaded = try atNoon(
            config: "", permissions(false, .granted), .config("websites = true\n"), .command(.reload))
        var ask = AccessibilityAsk(before.snapshot())

        let afterTheReload = ask.isDue(after: reloaded.snapshot())

        #expect(reloaded.snapshot().preferences().websiteChecking.state == .on)
        #expect(!afterTheReload)
    }

    /// Onboarding's own page offers the grant. Its command reaches the app only as the snapshot
    /// after its outcome, once onboarding has finished, as the CLI's does.
    @Test func websiteCheckingOnboardingTurnsOnAsItFinishesAsksNothing() throws {
        let core = try atNoon(permissions(false, .granted))
        var ask = AccessibilityAsk(core.snapshot())

        core.send(command: .websites(.on))
        ask.onboardingTurnedOn()
        let finished = ask.isDue(after: core.snapshot())
        core.send(command: .websites(.off))
        let turnedOff = ask.isDue(after: core.snapshot())
        core.send(command: .websites(.on))
        let turnedOnAgain = ask.isDue(after: core.snapshot())

        #expect(!finished)
        #expect(!turnedOff)
        #expect(turnedOnAgain)
    }

    /// Onboarding's command changes nothing then, so the next turn on is the user's.
    @Test func onboardingSendingWebsiteCheckingOnWhileItsOnAlreadyLeavesTheNextTurnOnToAsk() throws {
        let core = try atNoon(permissions(false, .granted), .command(.websites(.on)))
        var ask = AccessibilityAsk(core.snapshot())

        ask.onboardingTurnedOn()
        let finished = ask.isDue(after: core.snapshot())
        core.send(command: .websites(.off))
        _ = ask.isDue(after: core.snapshot())
        core.send(command: .websites(.on))
        let turnedOnAgain = ask.isDue(after: core.snapshot())

        #expect(!finished)
        #expect(turnedOnAgain)
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
