import Foundation
import NightcapCore
import Testing

/// The place rows' words, each in the situation that shows it.
struct PlaceRowWordingTests {
    @Test func untilTheScheduleTurnsNamesSunriseOrSunsetOrTheCustomHoursEndOrStart() throws {
        let cases: [(now: String, enabled: Bool, schedule: Schedule, state: OnOff, title: String, moment: String)] = [
            ("2026-10-16T01:40:00+02:00", true, .sunsetToSunrise, .off, "Until 18:31", "At sunset"),
            ("2026-10-16T12:00:00+02:00", false, .sunsetToSunrise, .on, "Until 07:52", "At sunrise"),
            ("2026-10-16T23:00:00+02:00", true, customHours, .off, "Until 22:00", "When the schedule starts"),
            ("2026-10-16T12:00:00+02:00", false, customHours, .on, "Until 07:00", "When the schedule ends"),
        ]
        for (now, enabled, schedule, state, title, moment) in cases {
            let core = try staged(now, zurichSun, reported(enabled, schedule))

            let row = try #require(core.everywhereRow)

            let until = try #require(
                row.choice(sending: .set(place: .everywhere, state: state, length: .untilScheduleTurns)))
            #expect(until.title == title)
            #expect(until.subtitle == moment)
        }
    }

    @Test func theConfigsLengthsAreWordedWithTheirPlurals() throws {
        let preset: [(text: String, seconds: Int64, title: String)] = [
            ("1m", 60, "For 1 Minute"), ("15m", 900, "For 15 Minutes"), ("1h", 3600, "For 1 Hour"),
            ("1h1m", 3660, "For 1 Hour 1 Minute"), ("1h30m", 5400, "For 1 Hour 30 Minutes"),
            ("2h", 7200, "For 2 Hours"), ("2h1m", 7260, "For 2 Hours 1 Minute"), ("24h", 86400, "For 24 Hours"),
        ]
        let core = try onByTheSchedule(config: lengths(preset.map(\.text)))

        let row = try #require(core.everywhereRow)

        for (_, seconds, title) in preset {
            let item = row.choice(sending: .set(place: .everywhere, state: .off, length: .for(seconds)))
            #expect(item?.title == title)
        }
    }

    @Test func everywheresRowNamesTheAppOrSiteInFrontThatKeepsTheOtherWay() throws {
        let app = try staged(
            "2026-10-16T12:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n", reported(false),
            front("com.figma.Desktop"))
        let site = try staged(
            "2026-10-16T12:00:00+02:00", config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n",
            accessibilityGranted, reported(false), front("com.apple.Safari", browser: true), host("www.figma.com"))

        let appRow = try #require(app.everywhereRow)
        let siteRow = try #require(site.everywhereRow)

        #expect(appRow.title == "Turn On")
        #expect(appRow.subtitle == "Everywhere but in Figma")
        #expect(appRow.timePickerTitle == "Turn On Everywhere")
        #expect(siteRow.subtitle == "Everywhere but at figma.com")
    }

    /// The switch keeps everywhere's rule until the schedule turns, which says its way, and going
    /// back to the schedule takes that off too.
    @Test func everywheresRowGoesBackToTheScheduleFromChangesOfItsOwn() throws {
        let core = try onByTheSchedule(
            .command(.set(place: .everywhere, state: .on, length: .untilScheduleTurns)),
            .command(.set(place: .everywhere, state: .off, length: minutes(30))))

        let row = try #require(core.everywhereRow)

        #expect(row.title == "Turn On")
        #expect(row.choice(sending: .reset(.everywhere))?.title == "Back to the Schedule")
    }

    @Test func anAppsRowTurnsItTheOtherWayAndGivesItARuleThenRemovesIt() throws {
        let core = try onByTheSchedule(front("com.figma.Desktop"))
        let rule = Command.addRule(identity: .app("com.figma.Desktop"), says: .off)

        var row = try #require(core.appRow)
        #expect(row.title == "Turn Off in Figma")
        #expect(row.subtitle == nil)
        #expect(row.choice(sending: .set(place: figmaApp, state: .off, length: minutes(30)))?.title == "For 30 Minutes")
        #expect(row.timePickerTitle == "Turn Off in Figma")
        #expect(
            row.choice(sending: .set(place: figmaApp, state: .off, length: .untilScheduleTurns))?.title
                == "Until 22:00")
        #expect(row.choice(sending: rule)?.title == "Always")
        core.send(command: rule)

        row = try #require(core.appRow)
        #expect(row.title == "Turn On in Figma")
        #expect(row.subtitle == "Always off: your rule")
        #expect(row.choice(sending: .removeRule(.app("com.figma.Desktop")))?.title == "Remove Your Rule")
    }

    @Test func anAppWhoseNameIsntKnownGoesByItsBundleId() throws {
        let core = try onByTheSchedule(front("com.figma.Desktop"))

        #expect(core.menu(names: [:]).rows.last?.title == "Turn Off in com.figma.Desktop")
    }

    @Test func aConfigRulesAlwaysIsGreyedBecauseTheConfigDecides() throws {
        let row = try #require(try forced().appRow)

        #expect(row.title == "Turn On in Figma")
        #expect(row.subtitle == "Always off: your config’s rule")
        let always = try #require(row.choice(sending: .addRule(identity: .app("com.figma.Desktop"), says: .on)))
        #expect(always.title == "Always")
        #expect(always.subtitle == "Your config decides")
        #expect(!always.enabled)
    }

    @Test func aSnoozedRulesRowPutsItBack() throws {
        let core = try forced(.command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns)))

        let row = try #require(core.appRow)

        #expect(row.title == "Turn Off in Figma")
        #expect(row.subtitle == "On until 07:00, then your config’s rule")
        #expect(row.choice(sending: .reset(figmaApp))?.title == "Put the Rule Back")
    }

    @Test func aChangeOverAnotherIsToldTopDownThenTheRule() throws {
        let off = try forced(
            .command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns)),
            .command(.set(place: figmaApp, state: .off, length: minutes(30))))
        let on = try staged(
            "2026-10-16T12:00:00+02:00", config: "[rules.on]\napp = [\"com.apple.iBooksX\"]\n", reported(false),
            front("com.apple.iBooksX"), reported(true),
            .command(.set(place: .app("com.apple.iBooksX"), state: .off, length: .untilScheduleTurns)),
            .command(.set(place: .app("com.apple.iBooksX"), state: .on, length: minutes(30))))
        // Without the sun times, an outside change holds the site in front with no end.
        let untilTurnedBack = try staged(
            "2026-10-16T12:00:00+02:00", config: "websites = true\n[rules.on]\ndomain = [\"figma.com\"]\n",
            accessibilityGranted, reported(false, .sunsetToSunrise), front("com.apple.Safari", browser: true),
            host("figma.com"), reported(true, .sunsetToSunrise), reported(false, .sunsetToSunrise),
            .command(.set(place: .domain("figma.com"), state: .on, length: minutes(30))))

        #expect(
            try #require(off.appRow).subtitle == "Off until 23:30, then on until 07:00, then your config’s rule")
        #expect(
            try #require(on.appRow).subtitle == "On until 12:30, then off until 22:00, then your config’s rule")
        #expect(
            try #require(untilTurnedBack.siteRow).subtitle
                == "On until 12:30, then off until you turn it on, then your config’s rule")
    }

    @Test func aChangeAtAnAppWithNoRuleGoesBackLikeEverywhereElse() throws {
        let core = try onByTheSchedule(
            front("com.apple.mail"), .command(.set(place: mailApp, state: .off, length: minutes(30))))

        let row = try #require(core.appRow)

        #expect(row.title == "Turn On in Mail")
        #expect(row.subtitle == "Off until 23:30")
        #expect(row.choice(sending: .reset(mailApp))?.title == "Like Everywhere Else")
    }

    @Test func aSitesRowSaysThisSiteWhileItsPickerNamesTheDomain() throws {
        let core = try onByTheSchedule(
            config: "websites = true\n", front("com.apple.Safari", browser: true), host("docs.figma.com"))

        let row = try #require(core.siteRow)

        #expect(row.title == "Turn Off at This Site")
        #expect(row.site == "docs.figma.com")
        #expect(row.timePickerTitle == "Turn Off at docs.figma.com")
        #expect(row.choice(sending: .addRule(identity: .domain("docs.figma.com"), says: .off))?.title == "Always")
    }

    @Test func aSitesRowTellsItsRuleAndItsSnooze() throws {
        let figma = Place.domain("figma.com")
        let core = try onByTheSchedule(
            config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n", accessibilityGranted,
            front("com.apple.Safari", browser: true), host("www.figma.com"), reported(false))
        let before = try #require(core.siteRow)
        #expect(before.title == "Turn On at This Site")
        #expect(before.subtitle == "Always off: your config’s rule")
        let untilMorning = try before.choice(titled: "Until 07:00").command
        #expect(untilMorning == .set(place: figma, state: .on, length: .untilScheduleTurns))

        core.send(command: untilMorning)

        let row = try #require(core.siteRow)
        #expect(row.title == "Turn Off at This Site")
        #expect(row.subtitle == "On until 07:00, then your config’s rule")
        #expect(row.choice(sending: .reset(figma))?.title == "Put the Rule Back")
    }

    @Test func aSiteGoesBackToItsBrowsersSayByTheBrowsersName() throws {
        let example = Place.domain("example.com")
        let core = try onByTheSchedule(
            config:
                "websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n",
            front("com.apple.Safari", browser: true), host("example.com"),
            .command(.set(place: example, state: .on, length: minutes(30))))

        let row = try #require(core.siteRow)

        #expect(row.title == "Turn Off at This Site")
        #expect(row.subtitle == "On until 23:30")
        #expect(row.choice(sending: .reset(example))?.title == "Like the Rest of Safari")
    }

    /// Without the sun times, an outside change holds the app or site in front, and everywhere, with
    /// no end, and a change over that hold leaves its row nothing to offer.
    @Test func aRowWithNothingToOfferIsGreyedWithWhatItsPlaceSays() throws {
        let app = try staged(
            "2026-10-16T23:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n",
            reported(true, .sunsetToSunrise), front("com.figma.Desktop"), reported(false, .sunsetToSunrise),
            reported(true, .sunsetToSunrise), .command(.set(place: figmaApp, state: .off, length: minutes(30))))
        let site = try staged(
            "2026-10-16T23:00:00+02:00", config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n",
            accessibilityGranted, reported(true, .sunsetToSunrise), front("com.apple.Safari", browser: true),
            host("figma.com"), reported(false, .sunsetToSunrise), reported(true, .sunsetToSunrise),
            .command(.set(place: .domain("figma.com"), state: .off, length: minutes(30))))
        let says = "Off until 23:30, then on until you turn it off, then your config’s rule"

        let appRow = try #require(app.appRow)
        let siteRow = try #require(site.siteRow)

        #expect(appRow.items.isEmpty)
        #expect(appRow.title == "In Figma")
        #expect(appRow.subtitle == says)
        #expect(siteRow.items.isEmpty)
        #expect(siteRow.title == "At This Site")
        #expect(siteRow.subtitle == says)
    }

}
