import Foundation
import NightcapCore
import Testing

/// A managed release at noon in Zurich, with `config` and `rules` as its files, after `steps`.
func atNoon(config: String? = nil, rules: String? = nil, _ steps: Step...) throws -> Core {
    try Core.staged(
        scenario: Scenario(now: at("2026-10-16T12:00:00+02:00"), config: config, rules: rules, steps: steps))
}

/// A self-managed release at noon in Zurich that has found Nightcap's login item `item`, after
/// `steps`.
func selfManaged(_ item: LoginItem, _ steps: Step...) throws -> Core {
    try Core.staged(
        scenario: Scenario(
            now: at("2026-10-16T12:00:00+02:00"), bundleWritable: true,
            steps: [.fact(.loginItemChanged(item: item))] + steps))
}

/// Accessibility and notifications as macOS reports them.
func permissions(_ accessibility: Bool, _ notifications: NotificationPermission) -> Step {
    .fact(.permissionsChanged(accessibility: accessibility, notifications: notifications))
}

/// Night Shift as CoreBrightness reports it while macOS has sunset to sunrise set aside.
func reportedSetAside(_ enabled: Bool, _ schedule: Schedule) -> Step {
    .fact(.statusChanged(enabled: enabled, schedule: schedule, sunScheduleSetAside: true))
}

extension Core {
    /// What the check of `kind` found, as the checks query answers it.
    @MainActor func found(_ kind: CheckKind) -> Check? {
        checks().checks.first { $0.detail.kind == kind }
    }
}

/// The check of `kind` where it stands, in English with a 24-hour clock.
private func check(_ core: Core, _ kind: CheckKind) -> (CheckState, String)? {
    core.found(kind).map { ($0.state, Wording.staged.check($0.detail)) }
}

/// What each check found, in the words Settings shows it in, from a staged core's checks.
struct CheckWordingTests {
    @Test func accessibilityIsAProblemOnlyWhileWebsiteCheckingIsOn() throws {
        let core = try atNoon(permissions(false, .granted))
        #expect(check(core, .accessibility)! == (.off, "Not needed while website checking is off."))

        core.send(command: .websites(.on))
        #expect(
            check(core, .accessibility)!
                == (.drifted, "Accessibility isn’t allowed, so website checking can’t read the front tab."))

        core.report(fact: .permissionsChanged(accessibility: true, notifications: .granted))
        #expect(check(core, .accessibility)! == (.ok, "Accessibility is allowed."))
    }

    @Test func notificationsSwitchedOffAfterBeingAllowedAreAProblem() throws {
        let core = try atNoon(permissions(true, .denied))
        #expect(check(core, .notifications)! == (.off, "Notifications are off."))

        core.report(fact: .permissionsChanged(accessibility: true, notifications: .granted))
        #expect(check(core, .notifications)! == (.ok, "Allowed."))

        core.report(fact: .permissionsChanged(accessibility: true, notifications: .denied))
        #expect(
            check(core, .notifications)!
                == (.drifted, "Notifications were allowed, but they’re switched off in System Settings."))
    }

    @Test func theConfigFileCheckSaysWhetherTheFileIsMissingValidOrBroken() throws {
        #expect(check(try atNoon(), .configFile)! == (.off, "No config file."))
        #expect(
            check(try atNoon(config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n"), .configFile)!
                == (.ok, "Valid, with 1 rule."))
        #expect(
            check(
                try atNoon(config: "[rules.off]\napp = [\"com.figma.Desktop\", \"com.apple.Photos\"]\n"), .configFile)!
                == (.ok, "Valid, with 2 rules."))
        #expect(check(try atNoon(config: "websites = true\n"), .configFile)! == (.ok, "Valid, with no rules."))
        #expect(
            check(try atNoon(config: "[rules.off]\ndomian = [\"figma.com\"]\n"), .configFile)!
                == (.broken, "Line 2: unknown field `domian`, expected `app` or `domain`."))
        #expect(
            check(try atNoon(config: "[night_shift]\nstrength = 101\n"), .configFile)!
                == (.broken, "Line 2: night_shift.strength is 0 to 100, not 101."))
    }

    @Test func anInvalidConfigNamesTheEntryOrValueOnItsLine() throws {
        let invalid = { (config: String) in try check(atNoon(config: config), .configFile)! }

        #expect(
            try invalid("websites = true\n[rules]\napp = [\"com.figma.Desktop\"]\n")
                == (.broken, "Line 3: rules are listed by what they say, under [rules.off] and [rules.on]."))
        #expect(
            try invalid("[rules.off]\napp = [\n  \"com.figma.Desktop\",\n  \"com.figma.Desktop \",\n]\n")
                == (
                    .broken,
                    "Line 4: “com.figma.Desktop ” isn’t a bundle ID like com.figma.Desktop: only letters, digits, "
                        + "hyphens and periods."
                ))
        #expect(
            try invalid("[rules.on]\ndomain = [\"figma.com\"]\n[rules.off]\ndomain = [\"https://figma.com\"]\n")
                == (
                    .broken,
                    "Line 4: “https://figma.com” isn’t a domain like figma.com: no scheme, path, port or wildcard."
                ))
        #expect(
            try invalid("[rules.off]\napp = [\"com.figma.Desktop\"]\n[rules.on]\napp = [\"com.figma.desktop\"]\n")
                == (.broken, "Line 4: com.figma.Desktop is in both rules.off and rules.on: an app has one rule."))
        #expect(
            try invalid("[rules.off]\ndomain = [\"figma.com\"]\n[rules.on]\ndomain = [\"Figma.com\"]\n")
                == (.broken, "Line 4: figma.com is in both rules.off and rules.on: a domain has one rule."))
        #expect(
            try invalid("[night_shift]\nschedule = \"dusk\"\n")
                == (
                    .broken,
                    "Line 2: night_shift.schedule is “off”, “sunset-to-sunrise” or custom hours such as "
                        + "“22:00-07:00”, not “dusk”."
                ))
        #expect(
            try invalid("\nlengths = [\"45m\", \"forever\"]\n")
                == (
                    .broken,
                    "Line 2: lengths are like “30m”, “1h” or “1h30m”, each from a minute to a day, and there’s at "
                        + "least one."
                ))
        #expect(
            try invalid("websites = true\n\n[menu]\nlengths = [\"45m\"]\n")
                == (.broken, "Line 3: lengths is a key of its own, before any table, not one under [menu]."))
    }

    @Test func theAdHocRulesCheckCountsThemOrSaysWhyTheyAreOff() throws {
        let core = try atNoon()
        #expect(check(core, .adHocRules)! == (.ok, "No ad-hoc rules."))
        core.send(command: .addRule(identity: .app("com.figma.Desktop"), says: .off))
        #expect(check(core, .adHocRules)! == (.ok, "1 ad-hoc rule."))
        core.send(command: .addRule(identity: .domain("figma.com"), says: .off))
        #expect(check(core, .adHocRules)! == (.ok, "2 ad-hoc rules."))

        let newer = try atNoon(rules: "version = 3\n")
        guard case .adHocRules(.off(.newer(let path, _))) = newer.found(.adHocRules)?.detail else {
            Issue.record("expected rules.toml from a newer Nightcap")
            return
        }
        #expect(
            check(newer, .adHocRules)!
                == (.broken, "Ad-hoc rules are off: \(path) is version 3, from a newer Nightcap than this one."))

        let malformed = try atNoon(rules: "version = 2\n[rules.off]\napp = \"com.figma.Desktop\"\n")
        #expect(
            check(malformed, .adHocRules)!
                == (
                    .broken,
                    "Ad-hoc rules are off: \(path): line 3: invalid type: string \"com.figma.Desktop\", "
                        + "expected a sequence."
                ))

        let invalid = try atNoon(rules: "version = 2\n[rules.on]\ndomain = [\"figma.com\", \"*.figma.com\"]\n")
        #expect(
            check(invalid, .adHocRules)!
                == (
                    .broken,
                    "Ad-hoc rules are off: \(path): line 3: “*.figma.com” isn’t a domain like figma.com: no "
                        + "scheme, path, port or wildcard."
                ))
    }

    @Test func nightShiftMissingOnThisMacIsBroken() throws {
        let core = try atNoon()
        #expect(check(core, .nightShift)! == (.broken, "Night Shift isn’t available on this Mac."))

        core.report(fact: .statusChanged(enabled: false, schedule: .off, sunScheduleSetAside: false))
        #expect(check(core, .nightShift)! == (.ok, "Available, with no schedule."))

        core.report(fact: .statusChanged(enabled: false, schedule: customHours, sunScheduleSetAside: false))
        #expect(check(core, .nightShift)! == (.ok, "Available, following its schedule: 22:00 to 07:00."))

        core.report(fact: .statusChanged(enabled: false, schedule: .sunsetToSunrise, sunScheduleSetAside: false))
        #expect(check(core, .nightShift)! == (.ok, "Available, following its schedule: sunset to sunrise."))
    }

    @Test func customHoursFollowTheUsersClock() throws {
        let core = try atNoon(reported(false, customHours))
        var american = Locale.Components(languageCode: .english, languageRegion: .unitedStates)
        american.hourCycle = .oneToTwelve
        let twelveHour = Wording(locale: Locale(components: american), timeZone: Wording.staged.timeZone)

        #expect(
            twelveHour.check(core.found(.nightShift)!.detail)
                == "Available, following its schedule: 10:00\u{202F}PM to 7:00\u{202F}AM.")
    }

    @Test func sunsetToSunriseSetAsideNeedsLocationServices() throws {
        let core = try Core.staged(
            scenario: Scenario(now: at("2026-10-16T01:40:00+02:00"), steps: [reportedSetAside(false, .off)]))

        #expect(
            check(core, .nightShift)!
                == (
                    .drifted,
                    "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on."
                ))
    }

    /// A staged core's clock stands still, so it never waits ten minutes for the sun times: this
    /// one check is worded from what the core reports after it has.
    @Test func sunsetToSunriseWithoutSunTimesSaysCustomHoursWork() {
        #expect(
            Wording.staged.check(.nightShift(.noSunTimes(waited: 600)))
                == "macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule. "
                + "Custom hours work without them.")
    }

    @Test func openingAtLoginSaysWhetherNightcapOpens() throws {
        let core = try selfManaged(.notRegistered)
        #expect(check(core, .openAtLogin)! == (.off, "Nightcap doesn’t open at login."))

        core.send(command: .openAtLogin(.on))
        core.report(fact: .loginItemChanged(item: .enabled))
        #expect(check(core, .openAtLogin)! == (.ok, "Nightcap opens when you log in."))

        core.report(fact: .loginItemChanged(item: .notRegistered))
        #expect(
            check(core, .openAtLogin)!
                == (.drifted, "Nightcap no longer opens at login: it was removed in System Settings."))

        core.report(fact: .loginItemChanged(item: .requiresApproval))
        #expect(check(core, .openAtLogin)! == (.drifted, "macOS is waiting for your approval in Login Items."))
    }
}
