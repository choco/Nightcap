import Foundation
import NightcapCore
import Testing

/// The line under the Night Shift item, for each thing that decides.
struct NightShiftItemWordingTests {
    @Test func whileNightShiftIsUnavailableTheLineOnlySaysSo() throws {
        #expect(try staged("2026-10-16T12:00:00+02:00").line == "Unavailable")
    }

    @Test func withNoScheduleTheLineSaysSo() throws {
        #expect(try staged("2026-10-16T12:00:00+02:00", reported(false, .off)).line == "No schedule")
    }

    @Test func theScheduleIsFollowedUntilItTurnsOrWithoutTheSunTimesAsMacOSAppliesIt() throws {
        #expect(try onByTheSchedule().line == "Following the schedule until 07:00")
        #expect(
            try staged("2026-10-16T23:00:00+02:00", reported(true, .sunsetToSunrise)).line == "Following the schedule")
    }

    @Test func aChangeEverywhereSaysUntilWhenOrUntilYouTurnItBack() throws {
        let off = try onByTheSchedule(.command(.set(place: .everywhere, state: .off, length: minutes(30))))
        let on = try offByTheSchedule(.command(.set(place: .everywhere, state: .on, length: minutes(30))))

        #expect(off.line == "Off until 23:30")
        #expect(on.line == "On until 20:30")
        #expect(try onByTheSchedule(.command(.off)).line == "Off until you turn it on")
        #expect(try offByTheSchedule(.command(.on)).line == "On until you turn it off")
    }

    @Test func aChangeAtAnAppOrSiteSaysWhereAndUntilWhen() throws {
        let app = try onByTheSchedule(
            front("com.apple.mail"), .command(.set(place: mailApp, state: .off, length: minutes(30))))
        let site = try onByTheSchedule(
            config: "websites = true\n", front("com.apple.Safari", browser: true), host("example.com"),
            .command(.set(place: .domain("example.com"), state: .off, length: minutes(30))))
        let snoozedApp = try forced(.command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns)))
        let snoozedSite = try onByTheSchedule(
            config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n", accessibilityGranted,
            front("com.apple.Safari", browser: true), host("www.figma.com"), reported(false),
            .command(.set(place: .domain("figma.com"), state: .on, length: .untilScheduleTurns)))

        #expect(app.line == "Off in Mail until 23:30")
        #expect(site.line == "Off at example.com until 23:30")
        #expect(snoozedApp.line == "On in Figma until 07:00")
        #expect(snoozedSite.line == "On at figma.com until 07:00")
    }

    /// Without the sun times, an outside change holds the app or site in front with no end.
    @Test func anOutsideChangeWithNoEndAtAnAppOrSiteLastsUntilYouTurnItBack() throws {
        let app = try staged(
            "2026-10-16T23:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n",
            reported(true, .sunsetToSunrise), front("com.figma.Desktop"), reported(false, .sunsetToSunrise),
            reported(true, .sunsetToSunrise))
        let site = try staged(
            "2026-10-16T12:00:00+02:00", config: "websites = true\n[rules.on]\ndomain = [\"figma.com\"]\n",
            accessibilityGranted, reported(false, .sunsetToSunrise), front("com.apple.Safari", browser: true),
            host("figma.com"), reported(true, .sunsetToSunrise), reported(false, .sunsetToSunrise))

        #expect(app.line == "On in Figma until you turn it off")
        #expect(site.line == "Off at figma.com until you turn it on")
    }

    @Test func aRuleSaysWhereAndWhoseItIs() throws {
        let yours = try onByTheSchedule(
            front("com.apple.mail"), .command(.addRule(identity: .app("com.apple.mail"), says: .off)))
        let saysOn = try staged(
            "2026-10-16T12:00:00+02:00", config: "[rules.on]\napp = [\"com.apple.iBooksX\"]\n", reported(false),
            front("com.apple.iBooksX"))
        let site = try onByTheSchedule(
            config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n", accessibilityGranted,
            front("com.apple.Safari", browser: true), host("www.figma.com"))
        let tours = try onByTheSchedule(
            tourStarted, front(tourSample), .command(.addRule(identity: .app(tourSample), says: .off)))

        #expect(try forced().line == "Off in Figma: your config’s rule")
        #expect(yours.line == "Off in Mail: your rule")
        #expect(saysOn.line == "On in Books: your config’s rule")
        #expect(site.line == "Off at figma.com: your config’s rule")
        #expect(tours.line == "Off in Tour Sample: the tour’s rule")
    }

    @Test func theTourStandingInForTheScheduleSaysSo() throws {
        #expect(try offByTheSchedule(tourStarted, reported(true)).line == "On for the tour")
    }
}
