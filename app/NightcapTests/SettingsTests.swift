import Foundation
import NightcapCore
import Testing

/// Settings as `core` answers its queries, in English with a 24-hour clock.
private func settings(_ core: Core) -> Settings {
    Settings(core.snapshot(), words: .staged)
}

private func fix(_ title: String, _ fix: Settings.Fix) -> Settings.FixButton {
    Settings.FixButton(title: title, fix: fix)
}

/// What the settings window shows for a staged core: its switches, the checks and their fixes,
/// the Rules tab, and which tab a problem opens.
struct SettingsTests {
    @Test func websiteCheckingSwitchesUnlessTheConfigSetsIt() throws {
        let core = try onByTheSchedule()
        #expect(settings(core).websiteChecking == Settings.Switch(on: false, locked: false, command: .websites(.on)))

        core.send(command: .websites(.on))
        #expect(settings(core).websiteChecking == Settings.Switch(on: true, locked: false, command: .websites(.off)))

        let locked = try onByTheSchedule(config: "websites = false\n")
        #expect(settings(locked).websiteChecking == Settings.Switch(on: false, locked: true, command: .websites(.on)))
    }

    @Test func accessibilityGoesUnmentionedWhileWebsiteCheckingDoesntNeedIt() throws {
        let core = try atNoon(permissions(false, .granted))
        #expect(settings(core).check(.accessibility) == nil)

        core.send(command: .websites(.on))
        #expect(settings(core).check(.accessibility)?.state == .drifted)
    }

    @Test func settingsOfferToAllowAccessibilityOrStopCheckingWebsites() throws {
        let core = try atNoon(config: "", permissions(false, .granted), .command(.websites(.on)))

        #expect(
            settings(core).check(.accessibility)
                == Settings.CheckStatus(
                    kind: .accessibility, state: .drifted,
                    text: "Accessibility isn’t allowed, so website checking can’t read the front tab.",
                    restore: fix("Allow…", .app(.askForAccessibility)),
                    accept: fix("Stop Checking Websites", .command(.websites(.off)))))
        core.send(command: .websites(.off))
        #expect(!settings(core).websiteChecking.on)
        #expect(settings(core).check(.accessibility) == nil)
    }

    @Test func onlyTheConfigStopsTheWebsiteCheckingItTurnedOn() throws {
        let core = try atNoon(config: "websites = true\n", permissions(false, .granted))

        let accessibility = try #require(settings(core).check(.accessibility))
        #expect(accessibility.restore == fix("Allow…", .app(.askForAccessibility)))
        #expect(accessibility.accept == nil)
    }

    @Test func settingsOfferToAllowNotificationsOrKeepThemOff() throws {
        let core = try atNoon(permissions(true, .granted), permissions(true, .denied))

        #expect(
            settings(core).check(.notifications)
                == Settings.CheckStatus(
                    kind: .notifications, state: .drifted,
                    text: "Notifications were allowed, but they’re switched off in System Settings.",
                    restore: fix("Allow Notifications…", .app(.allowNotifications)),
                    accept: fix("Keep Notifications Off", .command(.keepNotificationsOff))))
        core.send(command: .keepNotificationsOff)
        #expect(
            settings(core).check(.notifications)
                == Settings.CheckStatus(
                    kind: .notifications, state: .off, text: "Notifications are off.",
                    restore: fix("Allow Notifications…", .app(.allowNotifications)), accept: nil))
    }

    @Test func sunsetToSunriseSetAsideOffersLocationServicesOrKeepingTheScheduleOff() throws {
        let core = try Core.staged(
            scenario: Scenario(
                now: at("2026-10-16T01:40:00+02:00"), steps: [zurichSun, reported(true, .sunsetToSunrise)]))
        let following = Settings.CheckStatus(
            kind: .nightShift, state: .ok, text: "Available, following its schedule: sunset to sunrise.",
            restore: nil, accept: nil)
        #expect(settings(core).check(.nightShift) == following)

        core.report(fact: .statusChanged(enabled: false, schedule: .off, sunScheduleSetAside: true))
        #expect(
            settings(core).check(.nightShift)
                == Settings.CheckStatus(
                    kind: .nightShift, state: .drifted,
                    text:
                        "Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on.",
                    restore: fix("Open Location Services…", .app(.openLocationServices)),
                    accept: fix("Keep the Schedule Off", .command(.keepScheduleOff))))

        core.report(fact: .statusChanged(enabled: true, schedule: .sunsetToSunrise, sunScheduleSetAside: false))
        #expect(settings(core).check(.nightShift) == following)
    }

    @Test func nightcapOpeningWithSunsetToSunriseSetAsideShowsTheProblem() throws {
        let core = try Core.staged(
            scenario: Scenario(now: at("2026-10-16T01:40:00+02:00"), steps: [reportedSetAside(false, .off)]))

        let nightShift = try #require(settings(core).check(.nightShift))
        #expect(nightShift.state == .drifted)
        #expect(nightShift.restore == fix("Open Location Services…", .app(.openLocationServices)))
        #expect(nightShift.accept == fix("Keep the Schedule Off", .command(.keepScheduleOff)))
    }

    @Test func theScheduleKeptOffIsNoSchedule() throws {
        let core = try Core.staged(
            scenario: Scenario(
                now: at("2026-10-16T01:40:00+02:00"),
                steps: [reportedSetAside(false, .off), .command(.keepScheduleOff)]))

        #expect(
            settings(core).check(.nightShift)
                == Settings.CheckStatus(
                    kind: .nightShift, state: .ok, text: "Available, with no schedule.", restore: nil, accept: nil))
    }

    @Test func aScheduleChosenWhileSunsetToSunriseIsSetAsideIsNoProblem() throws {
        let core = try Core.staged(
            scenario: Scenario(
                now: at("2026-10-16T01:40:00+02:00"),
                steps: [reportedSetAside(false, .off), reportedSetAside(true, customHours)]))

        #expect(
            settings(core).check(.nightShift)
                == Settings.CheckStatus(
                    kind: .nightShift, state: .ok, text: "Available, following its schedule: 22:00 to 07:00.",
                    restore: nil, accept: nil))
    }

    @Test func settingsOfferToReloadABrokenConfig() throws {
        let broken = "[rules.off]\ndomian = [\"figma.com\"]\n"
        let core = try atNoon(config: broken)
        let check = try #require(settings(core).check(.configFile))
        #expect(check.state == .broken)
        #expect(check.restore == fix("Reload", .command(.reload)))
        #expect(check.accept == nil)

        let reloaded = try atNoon(
            config: broken, .config("[rules.off]\ndomain = [\"figma.com\"]\n"), .command(.reload))
        #expect(
            settings(reloaded).check(.configFile)
                == Settings.CheckStatus(
                    kind: .configFile, state: .ok, text: "Valid, with 1 rule.", restore: nil, accept: nil))
    }

    @Test func theAdHocRulesFileIsMentionedOnlyWhileSomethingsWrongWithIt() throws {
        #expect(settings(try atNoon()).check(.adHocRules) == nil)

        let newer = try #require(settings(try atNoon(rules: "version = 3\n")).check(.adHocRules))
        #expect(newer.state == .broken)
        #expect(newer.restore == nil)
        #expect(newer.accept == nil)
    }

    @Test func settingsSwitchOpeningAtLoginInASelfManagedInstall() throws {
        let core = try selfManaged(.notRegistered)
        #expect(
            settings(core).openAtLogin
                == Settings.Switch(on: false, locked: false, command: .openAtLogin(.on)))

        core.send(command: .openAtLogin(.on))
        core.report(fact: .loginItemChanged(item: .enabled))
        #expect(
            settings(core).openAtLogin
                == Settings.Switch(on: true, locked: false, command: .openAtLogin(.off)))

        core.report(fact: .loginItemChanged(item: .requiresApproval))
        #expect(
            settings(core).openAtLogin
                == Settings.Switch(on: true, locked: false, command: .openAtLogin(.off)))
    }

    @Test func aLoginItemRegisteredButWaitingForApprovalCountsAsOn() throws {
        // Registered without the user choosing it, so its check finds opening at login off.
        let core = try selfManaged(.requiresApproval)

        #expect(core.found(.openAtLogin)?.state == .off)
        #expect(
            settings(core).openAtLogin == Settings.Switch(on: true, locked: false, command: .openAtLogin(.off)))
    }

    @Test func aLoginItemThatIsntRegisteredIsOff() throws {
        let core = try selfManaged(.notRegistered, .command(.openAtLogin(.on)))

        #expect(core.found(.openAtLogin)?.state == .drifted)
        #expect(
            settings(core).openAtLogin == Settings.Switch(on: false, locked: false, command: .openAtLogin(.on)))
    }

    @Test func managedInstallsAndDevBuildsHaveNoOpenAtLoginSwitch() throws {
        let managed = try atNoon(.fact(.loginItemChanged(item: .notRegistered)))
        #expect(settings(managed).openAtLogin == nil)

        let dev = try Core.staged(
            scenario: Scenario(
                now: at("2026-10-16T12:00:00+02:00"), bundleId: "io.github.choco.nightcap.dev", bundleWritable: true,
                steps: [.fact(.loginItemChanged(item: .notRegistered))]))
        #expect(settings(dev).openAtLogin == nil)
    }

    @Test func openingAtLoginShowsItsCheckOnlyAsAProblem() throws {
        let core = try selfManaged(.notRegistered)
        #expect(settings(core).check(.openAtLogin) == nil)

        core.send(command: .openAtLogin(.on))
        core.report(fact: .loginItemChanged(item: .enabled))
        #expect(settings(core).check(.openAtLogin) == nil)

        core.report(fact: .loginItemChanged(item: .notRegistered))
        #expect(
            settings(core).check(.openAtLogin)
                == Settings.CheckStatus(
                    kind: .openAtLogin, state: .drifted,
                    text: "Nightcap no longer opens at login: it was removed in System Settings.",
                    restore: fix("Open at Login", .command(.openAtLogin(.on))),
                    accept: fix("Don’t Open at Login", .command(.openAtLogin(.off)))))
    }

    @Test func openingAtLoginWaitingForApprovalOffersLoginItems() throws {
        let core = try selfManaged(
            .notRegistered, .command(.openAtLogin(.on)), .fact(.loginItemChanged(item: .requiresApproval)))

        #expect(
            settings(core).check(.openAtLogin)
                == Settings.CheckStatus(
                    kind: .openAtLogin, state: .drifted, text: "macOS is waiting for your approval in Login Items.",
                    restore: fix("Open Login Items…", .app(.openLoginItems)),
                    accept: fix("Don’t Open at Login", .command(.openAtLogin(.off)))))
    }

    @Test func theRulesTabListsEveryRuleWithWhatCanBeDoneAboutIt() throws {
        let core = try onByTheSchedule(
            config: "[rules.off]\napp = [\"com.figma.Desktop\"]\ndomain = [\"figma.com\"]\n",
            .command(.addRule(identity: .app("com.apple.Photos"), says: .off)),
            .command(.addRule(identity: .domain("youtube.com"), says: .on)),
            .command(.set(place: .domain("figma.com"), state: .on, length: .untilScheduleTurns)))

        #expect(
            settings(core).rules == [
                Settings.RuleRow(
                    identity: .app("com.figma.Desktop"), says: .off, label: "From your config", snoozed: false,
                    action: nil),
                Settings.RuleRow(
                    identity: .domain("figma.com"), says: .off, label: "Snoozed until 07:00", snoozed: true,
                    action: Settings.Action(title: "Put Back", command: .reset(.domain("figma.com")))),
                Settings.RuleRow(
                    identity: .app("com.apple.Photos"), says: .off, label: "Ad-hoc", snoozed: false,
                    action: Settings.Action(title: "Remove", command: .removeRule(.app("com.apple.Photos")))),
                Settings.RuleRow(
                    identity: .domain("youtube.com"), says: .on, label: "Ad-hoc", snoozed: false,
                    action: Settings.Action(title: "Remove", command: .removeRule(.domain("youtube.com")))),
            ])
    }

    @Test func aRuleSnoozedWithNoEndSaysUntilYouTurnItBack() throws {
        let core = try staged(
            "2026-10-16T12:00:00+02:00", config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n", reported(false, .off),
            front("com.figma.Desktop"), reported(true, .off))

        #expect(settings(core).rules.first?.label == "Snoozed until you turn it off")
    }

    @Test func theRulesTabListsWhatTemporaryRulesSetWhereNoIndefiniteOneDoes() throws {
        let core = try onByTheSchedule(
            config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n",
            .command(.set(place: mailApp, state: .off, length: .untilScheduleTurns)),
            .command(.set(place: mailApp, state: .on, length: minutes(30))),
            // A snooze shows on its rule, and everywhere's own on the Night Shift item.
            .command(.set(place: figmaApp, state: .on, length: minutes(30))),
            .command(.off))

        #expect(
            settings(core).forNow == [
                Settings.ForNowRow(
                    identity: .app("com.apple.mail"), label: "Night Shift on until 23:30, then off until 22:00",
                    action: Settings.Action(title: "End", command: .reset(mailApp)))
            ])
    }

    @Test func theRulesTabNeverListsTheToursRules() throws {
        let core = try onByTheSchedule(
            tourBegun, front(tourSample),
            .command(.addRule(identity: .app(tourSample), says: .off)))

        #expect(core.snapshot().rules().contains { $0.setBy == .trial })
        #expect(settings(core).rules == [])
    }

    @Test func aProblemOpensTheTabItsCheckIsIn() throws {
        #expect(Settings.tab(for: try atNoon(reported(false, .off)).snapshot().checks().checks) == .general)
        let broken = try atNoon(config: "[rules.off]\ndomian = [\"figma.com\"]\n", reported(false, .off))
        #expect(Settings.tab(for: broken.snapshot().checks().checks) == .rules)
        #expect(settings(broken).hasProblem(in: .rules))
        #expect(!settings(broken).hasProblem(in: .general))

        let both = try atNoon(
            config: "[rules.off]\ndomian = [\"figma.com\"]\n", reported(false, .off), permissions(true, .granted),
            permissions(true, .denied))
        #expect(Settings.tab(for: both.snapshot().checks().checks) == .general)
        #expect(settings(both).hasProblem(in: .general))
    }

    @Test func settingsSayWhichNightcapAnswers() throws {
        let core = try atNoon()

        #expect(settings(core).version == core.snapshot().about().version)
        #expect(settings(core).configPath == core.snapshot().config().path)
    }
}
