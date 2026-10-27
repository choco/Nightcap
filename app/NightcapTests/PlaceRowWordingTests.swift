import Foundation
import NightcapCore
import Testing

private let words = Wording.staged

/// The place rows' words, each as the core writes it in the situation that shows it.
struct PlaceRowWordingTests {
    @Test func everywheresRowTurnsItTheOtherWayForEachLengthOrUntilTheScheduleStarts() throws {
        let core = try onByTheSchedule()

        let row = try #require(core.everywhereRow)

        #expect(row.title == words.turn(.off))
        #expect(row.subtitle == nil)
        for length: Int64 in [30, 60, 120] {
            let item = row.item(sending: .set(place: .everywhere, state: .off, length: minutes(length)))
            #expect(item?.title == words.forLength(length * 60))
        }
        #expect(row.timePickerTitle == words.timePickerTitle(.off))
        let until = try #require(row.item(sending: .set(place: .everywhere, state: .off, length: .untilScheduleTurns)))
        #expect(until.title == words.until(at("2026-10-17T22:00:00+02:00")))
        #expect(until.subtitle == words.moment(of: customHours, turning: .on))
    }

    @Test func untilTheScheduleTurnsNamesSunriseOrSunsetOrTheCustomHoursEndOrStart() throws {
        let cases: [(now: String, enabled: Bool, schedule: Schedule, state: OnOff, turn: String)] = [
            ("2026-10-16T01:40:00+02:00", true, .sunsetToSunrise, .off, "2026-10-16T16:31:32Z"),
            ("2026-10-16T12:00:00+02:00", false, .sunsetToSunrise, .on, "2026-10-17T05:52:14Z"),
            ("2026-10-16T23:00:00+02:00", true, customHours, .off, "2026-10-17T22:00:00+02:00"),
            ("2026-10-16T12:00:00+02:00", false, customHours, .on, "2026-10-17T07:00:00+02:00"),
        ]
        for (now, enabled, schedule, state, turn) in cases {
            let core = try staged(now, zurichSun, reported(enabled, schedule))

            let row = try #require(core.everywhereRow)

            let until = try #require(
                row.item(sending: .set(place: .everywhere, state: state, length: .untilScheduleTurns)))
            #expect(until.title == words.until(at(turn)))
            #expect(until.subtitle == words.moment(of: schedule, turning: state == .on ? .off : .on))
        }
    }

    @Test func theConfigsLengthsAreWordedWithTheirPlurals() throws {
        let preset: [(text: String, seconds: Int64)] = [
            ("1m", 60), ("15m", 900), ("1h", 3600), ("1h1m", 3660), ("1h30m", 5400), ("2h", 7200), ("2h1m", 7260),
            ("24h", 86400),
        ]
        let core = try onByTheSchedule(config: lengths(preset.map(\.text)))

        let row = try #require(core.everywhereRow)

        for (_, seconds) in preset {
            let item = row.item(sending: .set(place: .everywhere, state: .off, length: .for(seconds)))
            #expect(item?.title == words.forLength(seconds))
        }
    }

    @Test func everywheresRowNamesTheAppOrSiteInFrontThatKeepsTheOtherWay() throws {
        let app = try staged(
            "2026-10-16T12:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n", reported(false),
            front("com.figma.Desktop", "Figma"))
        let site = try staged(
            "2026-10-16T12:00:00+02:00", config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n",
            accessibilityGranted, reported(false), front("com.apple.Safari", "Safari", browser: true),
            host("www.figma.com"))

        let appRow = try #require(app.everywhereRow)
        let siteRow = try #require(site.everywhereRow)

        #expect(appRow.title == words.turn(.on))
        #expect(appRow.subtitle == words.everywhereBut(.app(name: "Figma")))
        #expect(appRow.timePickerTitle == words.timePickerTitle(.on))
        #expect(siteRow.subtitle == words.everywhereBut(.site(domain: "figma.com")))
    }

    /// The switch would leave everywhere's hold, which going back to the schedule takes off too.
    @Test func everywheresRowGoesBackToTheScheduleFromChangesOfItsOwn() throws {
        let core = try onByTheSchedule(
            .command(.set(place: .everywhere, state: .on, length: .untilScheduleTurns)),
            .command(.set(place: .everywhere, state: .off, length: minutes(30))))

        let row = try #require(core.everywhereRow)

        #expect(row.title == words.turn(.on))
        #expect(row.item(sending: .reset(.everywhere))?.title == words.backToTheSchedule)
    }

    @Test func anAppsRowTurnsItTheOtherWayAndGivesItARuleThenRemovesIt() throws {
        let core = try onByTheSchedule(front("com.figma.Desktop", "Figma"))
        let figma = Wording.Place.app(name: "Figma")
        let rule = Command.addRule(identity: .app("com.figma.Desktop"), says: .off)

        var row = try #require(core.appRow)
        #expect(row.title == words.turn(.off, at: figma))
        #expect(row.subtitle == words.placeSays([], then: nil))
        #expect(
            row.item(sending: .set(place: figmaApp, state: .off, length: minutes(30)))?.title == words.forLength(1800))
        #expect(row.timePickerTitle == words.timePickerTitle(.off, at: figma))
        #expect(
            row.item(sending: .set(place: figmaApp, state: .off, length: .untilScheduleTurns))?.title
                == words.until(at("2026-10-17T22:00:00+02:00")))
        #expect(row.item(sending: rule)?.title == words.always)
        core.send(command: rule)

        row = try #require(core.appRow)
        #expect(row.title == words.turn(.on, at: figma))
        #expect(row.subtitle == words.placeSays([], then: (.off, .yours)))
        #expect(row.item(sending: .removeRule(.app("com.figma.Desktop")))?.title == words.removeYourRule)
    }

    @Test func anAppWhoseNameIsntKnownGoesByItsBundleId() throws {
        let core = try onByTheSchedule(front("com.figma.Desktop"))

        #expect(try #require(core.appRow).title == words.turn(.off, at: .app(name: "com.figma.Desktop")))
    }

    @Test func aConfigRulesAlwaysIsGreyedBecauseTheConfigDecides() throws {
        let row = try #require(try forced().appRow)

        #expect(row.title == words.turn(.on, at: .app(name: "Figma")))
        #expect(row.subtitle == words.placeSays([], then: (.off, .config)))
        let always = try #require(row.item(sending: .addRule(identity: .app("com.figma.Desktop"), says: .on)))
        #expect(always.title == words.always)
        #expect(always.subtitle == words.yourConfigDecides)
        #expect(!always.enabled)
    }

    @Test func aSnoozedRulesRowPutsItBack() throws {
        let core = try forced(.command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns)))

        let row = try #require(core.appRow)

        #expect(row.title == words.turn(.off, at: .app(name: "Figma")))
        #expect(
            row.subtitle
                == words.placeSays([.init(state: .on, until: at("2026-10-17T07:00:00+02:00"))], then: (.off, .config)))
        #expect(row.item(sending: .reset(figmaApp))?.title == words.putTheRuleBack)
    }

    @Test func aChangeOverAnotherIsToldTopDownThenTheRule() throws {
        let off = try forced(
            .command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns)),
            .command(.set(place: figmaApp, state: .off, length: minutes(30))))
        let on = try staged(
            "2026-10-16T12:00:00+02:00", config: "[rules.on]\napp = [\"com.apple.iBooksX\"]\n", reported(false),
            front("com.apple.iBooksX", "Books"), reported(true),
            .command(.set(place: .app("com.apple.iBooksX"), state: .off, length: .untilScheduleTurns)),
            .command(.set(place: .app("com.apple.iBooksX"), state: .on, length: minutes(30))))
        // Without the sun times, an outside change holds the site in front with no end.
        let untilTurnedBack = try staged(
            "2026-10-16T12:00:00+02:00", config: "websites = true\n[rules.on]\ndomain = [\"figma.com\"]\n",
            accessibilityGranted, reported(false, .sunsetToSunrise), front("com.apple.Safari", browser: true),
            host("figma.com"), reported(true, .sunsetToSunrise), reported(false, .sunsetToSunrise),
            .command(.set(place: .domain("figma.com"), state: .on, length: minutes(30))))
        let halfPast = at("2026-10-16T12:30:00+02:00")

        #expect(
            try #require(off.appRow).subtitle
                == words.placeSays(
                    [
                        .init(state: .off, until: at("2026-10-16T23:30:00+02:00")),
                        .init(state: .on, until: at("2026-10-17T07:00:00+02:00")),
                    ], then: (.off, .config)))
        #expect(
            try #require(on.appRow).subtitle
                == words.placeSays(
                    [.init(state: .on, until: halfPast), .init(state: .off, until: at("2026-10-16T22:00:00+02:00"))],
                    then: (.on, .config)))
        #expect(
            try #require(untilTurnedBack.siteRow).subtitle
                == words.placeSays(
                    [.init(state: .on, until: halfPast), .init(state: .off, until: nil)], then: (.on, .config)))
    }

    @Test func aChangeAtAnAppWithNoRuleGoesBackLikeEverywhereElse() throws {
        let core = try onByTheSchedule(
            front("com.apple.mail", "Mail"), .command(.set(place: mailApp, state: .off, length: minutes(30))))

        let row = try #require(core.appRow)

        #expect(row.title == words.turn(.on, at: .app(name: "Mail")))
        #expect(
            row.subtitle == words.placeSays([.init(state: .off, until: at("2026-10-16T23:30:00+02:00"))], then: nil))
        #expect(row.item(sending: .reset(mailApp))?.title == words.likeEverywhereElse)
    }

    @Test func aSitesRowSaysThisSiteWhileItsPickerNamesTheDomain() throws {
        let core = try onByTheSchedule(
            config: "websites = true\n", front("com.apple.Safari", browser: true), host("docs.figma.com"))
        let site = Wording.Place.site(domain: "docs.figma.com")

        let row = try #require(core.siteRow)

        #expect(row.title == words.turn(.off, at: site))
        #expect(row.site == "docs.figma.com")
        #expect(row.timePickerTitle == words.timePickerTitle(.off, at: site))
        #expect(row.item(sending: .addRule(identity: .domain("docs.figma.com"), says: .off))?.title == words.always)
    }

    @Test func aSitesRowTellsItsRuleAndItsSnooze() throws {
        let figma = Place.domain("figma.com")
        let core = try onByTheSchedule(
            config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n", accessibilityGranted,
            front("com.apple.Safari", browser: true), host("www.figma.com"), reported(false))
        #expect(try #require(core.siteRow).subtitle == words.placeSays([], then: (.off, .config)))

        core.send(command: .set(place: figma, state: .on, length: .untilScheduleTurns))

        let row = try #require(core.siteRow)
        #expect(row.title == words.turn(.off, at: .site(domain: "figma.com")))
        #expect(
            row.subtitle
                == words.placeSays([.init(state: .on, until: at("2026-10-17T07:00:00+02:00"))], then: (.off, .config)))
        #expect(row.item(sending: .reset(figma))?.title == words.putTheRuleBack)
    }

    @Test func aSiteGoesBackToItsBrowsersSayByTheBrowsersName() throws {
        let example = Place.domain("example.com")
        let core = try onByTheSchedule(
            config:
                "websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n",
            front("com.apple.Safari", "Safari", browser: true), host("example.com"),
            .command(.set(place: example, state: .on, length: minutes(30))))

        let row = try #require(core.siteRow)

        #expect(row.title == words.turn(.off, at: .site(domain: "example.com")))
        #expect(row.subtitle == words.placeSays([.init(state: .on, until: at("2026-10-16T23:30:00+02:00"))], then: nil))
        #expect(row.item(sending: .reset(example))?.title == words.likeTheRest(of: "Safari"))
    }

    /// Without the sun times, an outside change holds the app or site in front, and everywhere, with
    /// no end, and a change over that hold leaves its row nothing to offer.
    @Test func aRowWithNothingToOfferIsGreyedWithWhatItsPlaceSays() throws {
        let app = try staged(
            "2026-10-16T23:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n",
            reported(true, .sunsetToSunrise), front("com.figma.Desktop", "Figma"), reported(false, .sunsetToSunrise),
            reported(true, .sunsetToSunrise), .command(.set(place: figmaApp, state: .off, length: minutes(30))))
        let site = try staged(
            "2026-10-16T23:00:00+02:00", config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n",
            accessibilityGranted, reported(true, .sunsetToSunrise), front("com.apple.Safari", browser: true),
            host("figma.com"), reported(false, .sunsetToSunrise), reported(true, .sunsetToSunrise),
            .command(.set(place: .domain("figma.com"), state: .off, length: minutes(30))))
        let changes = [
            Wording.Change(state: .off, until: at("2026-10-16T23:30:00+02:00")), .init(state: .on, until: nil),
        ]

        let appRow = try #require(app.appRow)
        let siteRow = try #require(site.siteRow)

        #expect(appRow.items.isEmpty)
        #expect(appRow.title == words.nothingToOffer(at: .app(name: "Figma")))
        #expect(appRow.subtitle == words.placeSays(changes, then: (.off, .config)))
        #expect(siteRow.items.isEmpty)
        #expect(siteRow.title == words.nothingToOffer(at: .site(domain: "figma.com")))
        #expect(siteRow.subtitle == words.placeSays(changes, then: (.off, .config)))
    }

    @Test func customIsTheItemThatAsksForATime() {
        #expect(words.custom == "Custom…")
    }
}
