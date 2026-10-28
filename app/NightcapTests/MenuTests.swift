import Foundation
import NightcapCore
import Testing

/// At 23:00 with Night Shift on by the custom hours, website checking on and a config rule for
/// figma.com, then Safari in front on www.figma.com, with Night Shift forced off.
private func onFigma(_ steps: Step...) throws -> Core {
    try Core.staged(
        scenario: Scenario(
            now: at("2026-10-16T23:00:00+02:00"), config: "websites = true\n[rules.off]\ndomain = [\"figma.com\"]\n",
            steps: [
                accessibilityGranted, reported(true), front("com.apple.Safari", browser: true), host("www.figma.com"),
                reported(false),
            ] + steps))
}

private func choice(_ title: String, _ command: Command) -> Menu.Item {
    .choice(Menu.Choice(title: title, command: command))
}

/// A site row's title, and the site it names under it.
private func siteTitle(_ row: Menu.PlaceRow?) -> (String?, String?) {
    (row?.title, row?.site)
}

/// What the menu shows and the commands its items send (ADR-0018): the moon, the Night Shift
/// item, the problem line, the strength and the place rows.
struct MenuTests {
    @Test func withNoScheduleTheNightShiftItemSaysSo() throws {
        let core = try staged("2026-10-16T12:00:00+02:00", reported(false, .off))

        #expect(core.menu().nightShift.line == "No schedule")
    }

    @Test func theMenuSaysWhatNightShiftIsDoingAndWhy() throws {
        let core = try forced()
        let item = { (on: Bool, line: String, command: Command) in
            Menu.NightShiftItem(on: on, line: line, command: command)
        }

        #expect(core.menu().moon == .forcedOff)
        #expect(core.menu().nightShift == item(true, "Off in Figma: your config’s rule", .off))
        core.send(command: .set(place: .everywhere, state: .off, length: minutes(30)))
        #expect(core.menu().nightShift == item(false, "Off in Figma: your config’s rule", .on))
        core.bringToFront("com.apple.Terminal")
        #expect(core.menu().moon == .off)
        #expect(core.menu().nightShift == item(false, "Off until 23:30", .on))
        core.send(command: .on)
        #expect(core.menu().nightShift == item(true, "Following the schedule until 07:00", .off))
        core.send(command: .off)
        #expect(core.menu().nightShift == item(false, "Off until you turn it on", .on))
    }

    @Test func whileNightShiftIsUnavailableTheMenuOnlySaysSo() throws {
        let menu = try staged("2026-10-16T12:00:00+02:00").menu()

        #expect(menu.moon == .unavailable)
        #expect(menu.nightShift == Menu.NightShiftItem(on: false, line: "Unavailable", command: nil))
        #expect(menu.entries.isEmpty)
    }

    @Test func theMenuHasEverywheresRowThenTheAppsAndTheSites() throws {
        let core = try onFigma(.fact(.strengthChanged(value: 60)))

        let kinds = core.menu().entries.map { entry in
            switch entry {
            case .strength: "strength"
            case .separator: "—"
            case .row(let row):
                switch row.place {
                case .everywhere: "everywhere"
                case .app: "app"
                case .domain: "site"
                }
            }
        }

        #expect(kinds == ["strength", "everywhere", "—", "app", "site"])
        // A site's row is drawn under its browser's.
        #expect(core.siteRow?.place == .domain("figma.com"))
    }

    @Test func everywheresRowHasEveryLengthButTheSwitchs() throws {
        let core = try onByTheSchedule()
        let off = { (length: Lasting) in Command.set(place: .everywhere, state: .off, length: length) }

        let menu = core.menu(at: at("2026-10-16T23:00:00+02:00"))

        #expect(menu.moon == .on)
        #expect(menu.nightShift.command == .off)
        #expect(
            menu.rows.first
                == Menu.PlaceRow(
                    place: .everywhere, title: "Turn Off", site: nil, subtitle: nil,
                    items: [
                        choice("For 30 Minutes", off(minutes(30))),
                        choice("For 1 Hour", off(minutes(60))),
                        choice("For 2 Hours", off(minutes(120))),
                        .pickTime(
                            Menu.PickTime(
                                item: "Custom…", title: "Turn Off Everywhere", place: .everywhere, state: .off,
                                start: at("2026-10-17T00:00:00+02:00"))),
                        .choice(
                            Menu.Choice(
                                title: "Until 22:00", subtitle: "When the schedule starts",
                                command: off(.untilScheduleTurns))),
                    ]))
    }

    /// The line under everywhere's item titled `title` at `time`, with Night Shift `enabled` as
    /// the schedule has it then.
    private func untilNote(_ time: String, _ enabled: Bool, _ schedule: Schedule, _ title: String) throws -> String? {
        try #require(try staged(time, zurichSun, reported(enabled, schedule)).everywhereRow).choice(titled: title)
            .subtitle
    }

    @Test func untilTheScheduleTurnsNamesSunriseOrSunset() throws {
        #expect(try untilNote("2026-10-16T01:40:00+02:00", true, .sunsetToSunrise, "Until 18:31") == "At sunset")
        #expect(try untilNote("2026-10-16T12:00:00+02:00", false, .sunsetToSunrise, "Until 07:52") == "At sunrise")
    }

    @Test func untilTheScheduleTurnsNamesCustomHoursStartOrEnd() throws {
        #expect(
            try untilNote("2026-10-16T23:00:00+02:00", true, customHours, "Until 22:00") == "When the schedule starts")
        #expect(
            try untilNote("2026-10-16T12:00:00+02:00", false, customHours, "Until 07:00") == "When the schedule ends")
    }

    @Test func theMenusLengthsComeFromTheConfig() throws {
        let core = try onByTheSchedule(config: "lengths = [\"15m\", \"1h30m\"]\n")

        #expect(
            try #require(core.everywhereRow).titles
                == ["For 15 Minutes", "For 1 Hour 30 Minutes", "Custom…", "Until 22:00"])
    }

    @Test func everywheresRowNamesThePlaceThatKeepsTheOtherWay() throws {
        let config = "[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.apple.iBooksX\"]\n"
        let day = try staged("2026-10-16T12:00:00+02:00", config: config, reported(false))
        let night = try staged("2026-10-16T23:00:00+02:00", config: config, reported(true))
        let row = { (core: Core, bundleId: String) in
            core.bringToFront(bundleId)
            let row = core.everywhereRow
            return [row?.title, row?.subtitle]
        }

        // By day, turning Night Shift on everywhere leaves Figma off, and Books on as it is.
        #expect(row(day, "com.figma.Desktop") == ["Turn On", "Everywhere but in Figma"])
        #expect(row(day, "com.apple.iBooksX") == ["Turn On", nil])
        // At night, turning it off everywhere leaves Books on, and Figma off as it is.
        #expect(row(night, "com.apple.iBooksX") == ["Turn Off", "Everywhere but in Books"])
        #expect(row(night, "com.figma.Desktop") == ["Turn Off", nil])
    }

    @Test func byDayTheRowsTurnOnAndByNightOff() throws {
        let day = try staged("2026-10-16T12:00:00+02:00", reported(false), front("com.apple.mail"))
        let night = try staged("2026-10-16T23:00:00+02:00", reported(true), front("com.apple.mail"))

        #expect(day.rows.map(\.title) == ["Turn On", "Turn On in Mail"])
        #expect(night.rows.map(\.title) == ["Turn Off", "Turn Off in Mail"])
    }

    @Test func withoutTheSunTimesAnAppsRowTurnsNightShiftTheOtherWayFromHowItIs() throws {
        let core = try staged("2026-10-16T23:00:00+02:00", front("com.googlecode.iterm2"))

        core.report(fact: .statusChanged(enabled: true, schedule: .sunsetToSunrise, sunScheduleSetAside: false))
        #expect(core.appRow?.title == "Turn Off in iTerm2")
        core.report(fact: .statusChanged(enabled: false, schedule: .sunsetToSunrise, sunScheduleSetAside: false))
        #expect(core.appRow?.title == "Turn On in iTerm2")
    }

    @Test func withoutTheSunTimesTheMenuFollowsTheScheduleAsMacOSAppliesIt() throws {
        let core = try staged("2026-10-16T23:00:00+02:00", reported(true, .sunsetToSunrise))

        let menu = core.menu()

        #expect(menu.nightShift.on)
        #expect(menu.nightShift.line == "Following the schedule")
        let row = try #require(core.everywhereRow)
        #expect(row.title == "Turn Off")
        #expect(row.titles == ["For 30 Minutes", "For 1 Hour", "For 2 Hours", "Custom…"])
        core.send(command: try row.choice(titled: "For 30 Minutes").command)
        #expect(core.snapshot().front().decision.state == .off)
    }

    @Test func undoingIsOfferedWhereItGoesTheRightWayAndARuleOfYoursCanAlwaysGo() throws {
        let core = try forced()
        core.send(command: .set(place: figmaApp, state: .on, length: .untilScheduleTurns))
        #expect(core.appRow?.items.first == choice("Put the Rule Back", .reset(figmaApp)))

        core.bringToFront("com.apple.mail")
        core.send(command: .set(place: mailApp, state: .off, length: minutes(30)))
        #expect(core.appRow?.items.first == choice("Like Everywhere Else", .reset(mailApp)))

        core.send(command: .reset(mailApp))
        core.send(command: .addRule(identity: .app("com.apple.mail"), says: .off))
        #expect(
            try #require(core.appRow).choice(titled: "Remove Your Rule").command == .removeRule(.app("com.apple.mail")))
        // By day removing it changes nothing on screen, but the rule is still the user's to remove.
        let day = try staged(
            "2026-10-17T12:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n", reported(false),
            front("com.apple.mail"), .command(.addRule(identity: .app("com.apple.mail"), says: .off)))
        #expect(
            try #require(day.appRow).choice(titled: "Remove Your Rule").command == .removeRule(.app("com.apple.mail")))
    }

    @Test func lengthsThatComeToTheSameAsUndoingAreLeftOut() throws {
        let core = try forced(.command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns)))

        #expect(
            core.appRow?.titles
                == ["Put the Rule Back", "—", "For 30 Minutes", "For 1 Hour", "For 2 Hours", "Custom…"])
        core.bringToFront("com.apple.mail")
        core.send(command: .set(place: mailApp, state: .off, length: minutes(30)))
        #expect(core.appRow?.titles == ["Like Everywhere Else", "—", "Always"])
    }

    @Test func aRuleUntilATimeOverWhatSaysTheSameKeepsTheLengths() throws {
        let core = try onByTheSchedule(
            front("com.apple.mail"), .command(.set(place: mailApp, state: .off, length: minutes(120))), .command(.off))

        #expect(
            core.appRow?.titles
                == ["For 30 Minutes", "For 1 Hour", "For 2 Hours", "Custom…", "Until 07:00", "—", "Always"])
    }

    /// Only the CLI stacks a rule until a time over a longer one at its place that says the same,
    /// with the other way under both. The menu decides from what the place's rules and what's
    /// under them say, not from what the place would give once its rule until a time ends, so it
    /// leaves the lengths out there, though one would turn the place the row's way for a while
    /// before the longer rule came back.
    @Test func overTwoRulesThatSayTheSameOverTheOtherWayTheMenuLeavesTheLengthsOut() throws {
        // `nightcap on --app com.figma.Desktop --until-schedule`, then `--for 30m`, over the
        // config's rule.
        let figma = try forced(
            .command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns)),
            .command(.set(place: figmaApp, state: .on, length: minutes(30))))
        #expect(figma.appRow?.titles == ["Put the Rule Back"])
        // `nightcap off`, then `nightcap off --for 1h`, at 23:00 over the schedule's on.
        let everywhere = try onByTheSchedule(
            .command(.off), .command(.set(place: .everywhere, state: .off, length: minutes(60))))
        #expect(everywhere.everywhereRow == nil)
    }

    @Test func backToTheScheduleIsOfferedOnlyWhereTheSwitchWouldKeepARuleOfEverywheres() throws {
        // The switch takes the hour off, as Back to the Schedule would.
        let offForAnHour = try onByTheSchedule(.command(.set(place: .everywhere, state: .off, length: minutes(60))))
        #expect(offForAnHour.everywhereRow == nil)

        let core = try onByTheSchedule(
            .command(.set(place: .everywhere, state: .on, length: .indefinite)),
            .command(.set(place: .everywhere, state: .off, length: minutes(60))))
        let row = try #require(core.everywhereRow)
        #expect(row.titles == ["Back to the Schedule"])
        core.send(command: try row.choice(titled: "Back to the Schedule").command)
        #expect(core.snapshot().front().everywhere.rules.isEmpty)
    }

    /// Each item that resets a place, in the situations the menu's tests stage, does what its
    /// title says: Put the Rule Back leaves the place giving its rule's say, Like Everywhere Else
    /// or Like the Rest of Safari what it inherits, and Back to the Schedule the schedule deciding
    /// everywhere, each the way its row turns the place.
    @Test func theItemsThatUndoDoWhatTheirTitlesSay() throws {
        let safariAndYouTube =
            "websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n"
        let situations: [() throws -> Core] = [
            { try forced() },
            { try forced(.command(.set(place: figmaApp, state: .on, length: .untilScheduleTurns))) },
            { try forced(.command(.set(place: figmaApp, state: .on, length: minutes(30)))) },
            { try forced(.command(.set(place: .everywhere, state: .off, length: minutes(30)))) },
            { try forced(front("com.apple.mail"), .command(.set(place: mailApp, state: .off, length: minutes(30)))) },
            { try onFigma() },
            { try onFigma(.command(.set(place: .domain("figma.com"), state: .on, length: .untilScheduleTurns))) },
            {
                try onByTheSchedule(
                    config: safariAndYouTube, front("com.apple.Safari", browser: true), host("example.com"),
                    .command(.set(place: .domain("example.com"), state: .on, length: minutes(30))))
            },
            {
                try onByTheSchedule(
                    .command(.set(place: .everywhere, state: .on, length: .indefinite)),
                    .command(.set(place: .everywhere, state: .off, length: minutes(60))))
            },
            {
                try onByTheSchedule(
                    front("com.apple.mail"), .command(.set(place: mailApp, state: .off, length: minutes(120))),
                    .command(.off))
            },
            {
                try offByTheSchedule(
                    tourBegunStandingIn, reported(true),
                    .command(.set(place: .everywhere, state: .on, length: .untilScheduleTurns)),
                    .command(.set(place: .everywhere, state: .off, length: minutes(30))))
            },
            // An outside change snoozes Figma's rule by day, and sets everywhere's on beside it.
            {
                try staged(
                    "2026-10-16T12:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n",
                    reported(false), front("com.figma.Desktop"), reported(true))
            },
        ]
        var offered: Set<String> = []

        for situation in situations {
            for row in try situation().rows {
                for case .choice(let choice) in row.items {
                    guard case .reset(let place) = choice.command else { continue }
                    let core = try situation()
                    let standing = { () throws -> PlaceInFront in
                        let front = core.snapshot().front()
                        return try #require((front.places + [front.everywhere]).first { $0.place == place })
                    }
                    let before = try standing()
                    let state = before.gives.other
                    core.send(command: choice.command)
                    let after = try standing()
                    #expect(after.gives == state, "\(choice.title) at \(place)")
                    #expect(after.rules.isEmpty, "\(choice.title) at \(place)")
                    switch choice.title {
                    case "Put the Rule Back":
                        let rule: Rule? = if case .rule(let rule) = before.base { rule } else { nil }
                        #expect(rule?.says == state)
                    case "Like Everywhere Else", "Like the Rest of Safari":
                        #expect(before.inherits?.says == state)
                    case "Back to the Schedule":
                        guard case .schedule(let scheduled, _) = after.base else {
                            Issue.record("no schedule under everywhere")
                            continue
                        }
                        #expect(scheduled == state)
                    default:
                        Issue.record("\(choice.title) resets \(place)")
                    }
                    offered.insert(choice.title)
                }
            }
        }
        #expect(
            offered == ["Put the Rule Back", "Like Everywhere Else", "Like the Rest of Safari", "Back to the Schedule"])
    }

    @Test func aConfigRulesAlwaysIsGreyed() throws {
        let row = try #require(try forced().menu(names: [:]).rows.last)

        #expect(row.title == "Turn On in com.figma.Desktop")
        #expect(row.subtitle == "Always off: your config’s rule")
        let always = try row.choice(titled: "Always")
        #expect(!always.enabled)
        #expect(always.subtitle == "Your config decides")
    }

    @Test func customIsOfferedBesideTheLengthsWithItsPlaceAndWay() throws {
        let row = try #require(try forced().menu(at: at("2026-10-16T23:10:00+02:00")).rows.last)

        #expect(
            row.items.contains(
                .pickTime(
                    Menu.PickTime(
                        item: "Custom…", title: "Turn On in Figma", place: figmaApp, state: .on,
                        start: at("2026-10-17T00:15:00+02:00")))))
    }

    @Test func theSiteRowIsTheTabDomainOrTheMostSpecificCoveringRules() throws {
        let core = try onByTheSchedule(config: "websites = true\n", front("com.apple.Safari", browser: true))
        core.reportHost("docs.figma.com")
        #expect(siteTitle(core.siteRow) == ("Turn Off at This Site", "docs.figma.com"))
        #expect(
            try #require(core.siteRow).choice(titled: "Always").command
                == .addRule(identity: .domain("docs.figma.com"), says: .off))
        core.reportHost("www.figma.com")
        #expect(siteTitle(core.siteRow) == ("Turn Off at This Site", "figma.com"))

        core.reportHost("docs.figma.com")
        core.send(command: .addRule(identity: .domain("figma.com"), says: .off))
        #expect(siteTitle(core.siteRow) == ("Turn On at This Site", "figma.com"))
        core.send(command: .addRule(identity: .domain("docs.figma.com"), says: .off))
        #expect(siteTitle(core.siteRow) == ("Turn On at This Site", "docs.figma.com"))
    }

    @Test func aSiteGoesBackToItsBrowsersSayByTheBrowsersName() throws {
        let example = Place.domain("example.com")
        let core = try onByTheSchedule(
            config:
                "websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n",
            front("com.apple.Safari", browser: true), host("example.com"),
            .command(.set(place: example, state: .on, length: minutes(30))))

        #expect(try #require(core.siteRow).choice(titled: "Like the Rest of Safari").command == .reset(example))
    }

    @Test func aSiteThatSaysNothingTurnsTheOtherWayFromItsBrowser() throws {
        let core = try onByTheSchedule(
            config:
                "websites = true\n[rules.off]\napp = [\"com.apple.Safari\"]\n[rules.on]\ndomain = [\"youtube.com\"]\n",
            front("com.apple.Safari", browser: true), host("example.com"))

        let row = try #require(core.siteRow)

        #expect(siteTitle(row) == ("Turn On at This Site", "example.com"))
        #expect(try row.choice(titled: "Always").command == .addRule(identity: .domain("example.com"), says: .on))
    }

    @Test func theSiteRowSnoozesAConfigRule() throws {
        let core = try onFigma()
        let figmaSite = Place.domain("figma.com")

        var row = try #require(core.siteRow)
        #expect(siteTitle(row) == ("Turn On at This Site", "figma.com"))
        #expect(row.subtitle == "Always off: your config’s rule")
        let untilMorning = try row.choice(titled: "Until 07:00").command
        #expect(untilMorning == .set(place: figmaSite, state: .on, length: .untilScheduleTurns))
        core.send(command: untilMorning)
        row = try #require(core.siteRow)
        #expect(siteTitle(row) == ("Turn Off at This Site", "figma.com"))
        #expect(row.subtitle == "On until 07:00, then your config’s rule")
        #expect(row.items.first == choice("Put the Rule Back", .reset(figmaSite)))
    }

    @Test func theMenuOpensWithAStrengthSliderOnceTheStrengthIsKnown() throws {
        let core = try staged("2026-10-16T12:00:00+02:00", reported(false, .off))
        #expect(!core.menu().entries.contains { if case .strength = $0 { true } else { false } })

        core.report(fact: .strengthChanged(value: 60))

        #expect(core.menu().entries.first == .strength(60))
    }

    @Test func theMenuAddsAndRemovesAnAdHocRuleForTheFrontmostApp() throws {
        let core = try onByTheSchedule(front("com.figma.Desktop"))
        let figma = RuleIdentity.app("com.figma.Desktop")

        var row = try #require(core.appRow)
        #expect(row.title == "Turn Off in Figma")
        #expect(row.subtitle == nil)
        #expect(try row.choice(titled: "Always").command == .addRule(identity: figma, says: .off))
        core.send(command: .addRule(identity: figma, says: .off))
        row = try #require(core.appRow)
        #expect(row.title == "Turn On in Figma")
        #expect(row.subtitle == "Always off: your rule")
        #expect(try row.choice(titled: "Remove Your Rule").command == .removeRule(figma))
    }

    @Test func theMenuShowsAProblemLineWhileAnyProblemExists() throws {
        let core = try staged("2026-10-16T12:00:00+02:00", config: "websites = true\n", reported(false, .off))
        let line = { (title: String, subtitle: String) in Menu.ProblemLine(title: title, subtitle: subtitle) }

        #expect(core.menu().problemLine == nil)
        core.report(fact: .permissionsChanged(accessibility: false, notifications: .granted))
        #expect(core.menu().problemLine == line("1 Thing Needs Attention…", "Accessibility"))
        core.report(fact: .permissionsChanged(accessibility: false, notifications: .denied))
        #expect(core.menu().problemLine == line("2 Things Need Attention…", "Accessibility and notifications"))
        core.report(fact: .permissionsChanged(accessibility: true, notifications: .granted))
        #expect(core.menu().problemLine == nil)
    }

    @Test func aScheduleSetAsideIsAProblemWithNightShift() throws {
        let core = try staged("2026-10-16T01:40:00+02:00", zurichSun, reported(true, .sunsetToSunrise))

        core.report(fact: .statusChanged(enabled: false, schedule: .off, sunScheduleSetAside: true))

        #expect(core.menu().problemLine == Menu.ProblemLine(title: "1 Thing Needs Attention…", subtitle: "Night Shift"))
    }

    @Test func aRuleFromTheTabKeepsWwwWhereNothingButASuffixWouldBeLeft() throws {
        let core = try onByTheSchedule(
            config: "websites = true\n", front("com.apple.Safari", browser: true), host("www.com"))

        let row = try #require(core.siteRow)

        #expect(siteTitle(row) == ("Turn Off at This Site", "www.com"))
        #expect(try row.choice(titled: "Always").command == .addRule(identity: .domain("www.com"), says: .off))
    }

    @Test func whileTheTourStandsInEverywheresRowOffersNoWayBackToTheSchedule() throws {
        let core = try offByTheSchedule(
            tourBegunStandingIn, reported(true),
            .command(.set(place: .everywhere, state: .on, length: .untilScheduleTurns)),
            .command(.set(place: .everywhere, state: .off, length: minutes(30))))

        // With nothing else to offer, the row is left out.
        #expect(core.everywhereRow.map { !$0.titles.contains("Back to the Schedule") } ?? true)
    }

    @Test func theMenuOffersToAddTheSamplesRuleAndThenToRemoveIt() throws {
        let core = try onByTheSchedule(tourBegun, front(tourSample))
        let sample = RuleIdentity.app(tourSample)

        var row = try #require(core.appRow)
        #expect(row.title == "Turn Off in Tour Sample")
        #expect(try row.choice(titled: "Always").command == .addRule(identity: sample, says: .off))
        core.send(command: .addRule(identity: sample, says: .off))
        row = try #require(core.appRow)
        #expect(row.title == "Turn On in Tour Sample")
        #expect(row.subtitle == "Always off: the tour’s rule")
        #expect(try row.choice(titled: "Remove Your Rule").command == .removeRule(sample))
    }

    @Test func anAppWhoseBundleIdNoRuleCouldNameHasNoRow() throws {
        let core = try onByTheSchedule(front("unity.Team Cherry.Hollow Knight"))

        #expect(core.appRow == nil)
    }

    /// With nothing else deciding, the moon is Night Shift as last reported.
    @Test func withTheScheduleUnknownTheLineSaysNothing() throws {
        let outOfRange = Schedule.custom(from: TimeOfDay(hour: 25, minute: 0), to: TimeOfDay(hour: 7, minute: 60))

        let menu = try onByTheSchedule(reported(true, outOfRange)).menu()

        #expect(menu.nightShift.line == nil)
        #expect(menu.moon == .on)
    }

    @Test func theMenuLeavesWebsiteCheckingToSettings() throws {
        #expect(try !onByTheSchedule().rows.contains { $0.title == "Check Websites" })
    }
}
