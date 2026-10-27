import Foundation
import NightcapCore
import Testing

private let words = Wording.staged

/// The line under the Night Shift item, as the core writes it for each thing that decides.
struct NightShiftItemWordingTests {
    @Test func whileNightShiftIsUnavailableTheLineOnlySaysSo() throws {
        #expect(try staged("2026-10-16T12:00:00+02:00").line == words.unavailable)
    }

    @Test func withNoScheduleTheLineSaysSo() throws {
        #expect(try staged("2026-10-16T12:00:00+02:00", reported(false, .off)).line == words.noSchedule)
    }

    @Test func theScheduleIsFollowedUntilItTurnsOrWithoutTheSunTimesAsMacOSAppliesIt() throws {
        #expect(
            try onByTheSchedule().line == words.followingTheSchedule(until: at("2026-10-17T07:00:00+02:00")))
        #expect(
            try staged("2026-10-16T23:00:00+02:00", reported(true, .sunsetToSunrise)).line
                == words.followingTheSchedule)
    }

    @Test func aChangeEverywhereSaysUntilWhenOrUntilYouTurnItBack() throws {
        let off = try onByTheSchedule(.command(.set(place: .everywhere, state: .off, length: minutes(30))))
        let on = try offByTheSchedule(.command(.set(place: .everywhere, state: .on, length: minutes(30))))

        #expect(off.line == words.change(.off, until: at("2026-10-16T23:30:00+02:00")))
        #expect(on.line == words.change(.on, until: at("2026-10-16T20:30:00+02:00")))
        #expect(try onByTheSchedule(.command(.off)).line == words.change(.off, until: nil))
        #expect(try offByTheSchedule(.command(.on)).line == words.change(.on, until: nil))
    }

    @Test func aChangeAtAnAppOrSiteSaysWhereAndUntilWhen() throws {
        let app = try onByTheSchedule(
            front("com.apple.mail", "Mail"), .command(.set(place: mailApp, state: .off, length: minutes(30))))
        let site = try onByTheSchedule(
            config: "websites = true\n", front("com.apple.Safari", browser: true), host("example.com"),
            .command(.set(place: .domain("example.com"), state: .off, length: minutes(30))))
        let snoozedApp = try forced(.command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns)))
        let snoozedSite = try onByTheSchedule(
            config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n", accessibilityGranted,
            front("com.apple.Safari", browser: true), host("www.figma.com"), reported(false),
            .command(.set(place: .domain("figma.com"), state: .on, length: .untilScheduleTurns)))
        let halfPast = at("2026-10-16T23:30:00+02:00")
        let morning = at("2026-10-17T07:00:00+02:00")

        #expect(app.line == words.change(.off, at: .app(name: "Mail"), until: halfPast))
        #expect(site.line == words.change(.off, at: .site(domain: "example.com"), until: halfPast))
        #expect(snoozedApp.line == words.change(.on, at: .app(name: "Figma"), until: morning))
        #expect(snoozedSite.line == words.change(.on, at: .site(domain: "figma.com"), until: morning))
    }

    /// Without the sun times, an outside change holds the app or site in front with no end.
    @Test func anOutsideChangeWithNoEndAtAnAppOrSiteLastsUntilYouTurnItBack() throws {
        let app = try staged(
            "2026-10-16T23:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n",
            reported(true, .sunsetToSunrise), front("com.figma.Desktop", "Figma"), reported(false, .sunsetToSunrise),
            reported(true, .sunsetToSunrise))
        let site = try staged(
            "2026-10-16T12:00:00+02:00", config: "websites = true\n[rules.on]\ndomain = [\"figma.com\"]\n",
            accessibilityGranted, reported(false, .sunsetToSunrise), front("com.apple.Safari", browser: true),
            host("figma.com"), reported(true, .sunsetToSunrise), reported(false, .sunsetToSunrise))

        #expect(app.line == words.change(.on, at: .app(name: "Figma"), until: nil))
        #expect(site.line == words.change(.off, at: .site(domain: "figma.com"), until: nil))
    }

    @Test func aRuleSaysWhereAndWhoseItIs() throws {
        let yours = try onByTheSchedule(
            front("com.apple.mail", "Mail"), .command(.addRule(identity: .app("com.apple.mail"), says: .off)))
        let saysOn = try staged(
            "2026-10-16T12:00:00+02:00", config: "[rules.on]\napp = [\"com.apple.iBooksX\"]\n", reported(false),
            front("com.apple.iBooksX", "Books"))
        let site = try onByTheSchedule(
            config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n", accessibilityGranted,
            front("com.apple.Safari", browser: true), host("www.figma.com"))
        let tours = try onByTheSchedule(
            tourStarted, front(tourSample, "Tour Sample"),
            .command(.addRule(identity: .app(tourSample), says: .off)))

        #expect(try forced().line == words.rule(.off, at: .app(name: "Figma"), whose: .config))
        #expect(yours.line == words.rule(.off, at: .app(name: "Mail"), whose: .yours))
        #expect(saysOn.line == words.rule(.on, at: .app(name: "Books"), whose: .config))
        #expect(site.line == words.rule(.off, at: .site(domain: "figma.com"), whose: .config))
        #expect(tours.line == words.rule(.off, at: .app(name: "Tour Sample"), whose: .tour))
    }

    @Test func theTourStandingInForTheScheduleSaysSo() throws {
        #expect(try offByTheSchedule(tourStarted, reported(true)).line == words.onForTheTour)
    }
}
