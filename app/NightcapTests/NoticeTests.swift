import Foundation
import NightcapCore
import Testing

private let figma = Notice.FrontApp(bundleId: "com.figma.Desktop", name: "Figma")

/// 07:00 on 17 October 2026 in Zurich.
private let morning = Date(timeIntervalSince1970: 1_792_213_200)

/// The notice for a check that became a problem.
private func problem(_ detail: CheckDetail) -> Notice? {
    Notice(problem: detail, words: .staged)
}

/// What each notification says, about an event from the core, a problem or an update.
struct NoticeTests {
    @Test func anOutsideChangeNamesTheSnoozedRuleByItsAppInFrontAndItsEnd() {
        let adopted = Event.adopted(
            place: .app("com.figma.Desktop"), snoozed: .app("com.figma.Desktop"), state: .on, until: morning)

        let notice = Notice(adopted, frontApp: figma, words: .staged)

        #expect(notice?.title == "Night Shift turned on outside Nightcap")
        #expect(notice?.body == "Snoozed the Figma rule until 07:00.")
    }

    @Test func aRuleWhoseAppIsntInFrontGoesByItsBundleIdAndADomainRuleByItsDomain() {
        let app = Event.adopted(
            place: .app("com.apple.mail"), snoozed: .app("com.apple.mail"), state: .off, until: nil)
        let site = Event.adopted(
            place: .domain("figma.com"), snoozed: .domain("figma.com"), state: .on, until: morning)

        #expect(
            Notice(app, frontApp: figma, words: .staged)?.body
                == "Snoozed the com.apple.mail rule until you turn it on.")
        #expect(Notice(site, frontApp: figma, words: .staged)?.body == "Snoozed the figma.com rule until 07:00.")
    }

    @Test func anOutsideChangeWithNoEndLastsUntilYouTurnItBack() {
        let on = Event.adopted(
            place: .app("com.figma.Desktop"), snoozed: .app("com.figma.Desktop"), state: .on, until: nil)
        let off = Event.adopted(place: .domain("figma.com"), snoozed: .domain("figma.com"), state: .off, until: nil)

        #expect(Notice(on, frontApp: figma, words: .staged)?.body == "Snoozed the Figma rule until you turn it off.")
        #expect(Notice(off, frontApp: figma, words: .staged)?.title == "Night Shift turned off outside Nightcap")
    }

    @Test func everyTimeReadsWithTwoDigitHoursOnA24HourClockAndAmOrPmOnA12HourOne() {
        let zurich = TimeZone(identifier: "Europe/Zurich")!
        let swiss = Wording(locale: Locale(identifier: "en_CH"), timeZone: zurich)
        let american = Wording(locale: Locale(identifier: "en_US"), timeZone: zurich)
        let adopted = Event.adopted(
            place: .app("com.figma.Desktop"), snoozed: .app("com.figma.Desktop"), state: .on, until: morning)

        #expect(Notice(adopted, frontApp: figma, words: swiss)?.body == "Snoozed the Figma rule until 07:00.")
        #expect(
            Notice(adopted, frontApp: figma, words: american)?.body == "Snoozed the Figma rule until 7:00\u{202F}AM.")
        #expect(swiss.time(TimeOfDay(hour: 6, minute: 5)) == "06:05")
        #expect(american.time(TimeOfDay(hour: 22, minute: 0)) == "10:00\u{202F}PM")
        #expect(swiss.eveningClock == "21:42")
        #expect(american.eveningClock == "9:42\u{202F}PM")
        #expect(swiss.loginClock == "Tue 08:30")
        #expect(american.loginClock == "Tue 8:30\u{202F}AM")
    }

    @Test func anOutsideChangeThatSnoozedNoRuleTellsNothing() {
        let adopted = Event.adopted(place: .everywhere, snoozed: nil, state: .on, until: morning)

        #expect(Notice(adopted, frontApp: figma, words: .staged) == nil)
    }

    @Test func anUpdateOpensItsVersionInTheChangelog() {
        let notice = Notice(updatedTo: "1.2.0", words: .staged)

        #expect(notice.title == "Nightcap updated to 1.2.0")
        #expect(notice.body == "Click to see what’s new.")
        #expect(notice.opens == URL(string: "https://nightcap.choco.me/changelog/#v1.2.0"))
    }

    @Test func aProblemSaysWhatAppeared() {
        let night = problem(.nightShift(.setAside))
        #expect(night?.title == "Sunset to sunrise needs Location Services")
        #expect(
            night?.body
                == "macOS turned Night Shift’s schedule off. Open Nightcap’s settings to turn Location Services "
                + "back on, or to keep the schedule off.")
        let sun = problem(.nightShift(.noSunTimes(waited: 600)))
        #expect(sun?.title == "Nightcap can’t follow sunset to sunrise")
        #expect(
            sun?.body
                == "macOS hasn’t worked out sunrise and sunset. Custom hours in Night Shift’s settings work "
                + "without them.")
        let accessibility = problem(.accessibility(.notAllowed))
        #expect(accessibility?.title == "Website checking needs Accessibility")
        #expect(accessibility?.body == "Open Nightcap’s settings to allow it, or to stop checking websites.")
        let login = problem(.openAtLogin(.removed))
        #expect(login?.title == "Nightcap no longer opens at login")
        #expect(login?.body == "Open Nightcap’s settings to turn it back on, or to keep it off.")
    }

    @Test func aFilesProblemSaysWhatsWrongFromItsPath() {
        let config = { (error: FileError) in problem(.configFile(.error(error))) }
        let malformed = config(
            .malformed(
                path: "/n/config.toml", line: 2, message: "unknown field `domian`, expected `app` or `domain`"))
        #expect(malformed?.title == "Nightcap’s config is invalid")
        #expect(malformed?.body == "/n/config.toml: line 2: unknown field `domian`, expected `app` or `domain`")
        #expect(
            config(.invalid(path: "/n/config.toml", line: 3, problem: .strengthOutOfRange(strength: 101)))?
                .body == "/n/config.toml: line 3: night_shift.strength is 0 to 100, not 101")
        let unreadable: [(IoFailure, String)] = [
            (.notFound, "/n/config.toml, or a folder on its path, isn’t there"),
            (.permissionDenied, "/n/config.toml: Nightcap isn’t allowed to read it"),
            (.isAFolder, "/n/config.toml is a folder, not a file"),
            (.notAFolder, "/n/config.toml: one of the folders on its path is a file"),
            (.notUtf8, "/n/config.toml isn’t UTF-8 text"),
            (.other, "/n/config.toml couldn’t be read"),
        ]
        for (failure, body) in unreadable {
            #expect(config(.unreadable(path: "/n/config.toml", failure: failure))?.body == body)
        }

        let rules = { (error: FileError) in problem(.adHocRules(.off(error))) }
        let newer = rules(.newer(path: "/n/rules.toml", version: 3))
        #expect(newer?.title == "Ad-hoc rules are off")
        #expect(newer?.body == "/n/rules.toml is version 3, from a newer Nightcap than this one")
        #expect(
            rules(.malformed(path: "/n/rules.toml", line: nil, message: "expected a table"))?.body
                == "/n/rules.toml: expected a table")
        #expect(
            rules(.invalid(path: "/n/rules.toml", line: 3, problem: .notABundleId(entry: "a/b")))?.body
                == "/n/rules.toml: line 3: “a/b” isn’t a bundle ID like com.figma.Desktop: only letters, digits, "
                + "hyphens and periods")
    }

    @Test func whatTheAppNeverAnnouncesTellsNothing() {
        #expect(problem(.nightShift(.unavailable)) == nil)
        #expect(problem(.notifications(.switchedOff)) == nil)
        #expect(problem(.openAtLogin(.awaitingApproval)) == nil)
        #expect(problem(.configFile(.valid(rules: 1))) == nil)
    }

    @Test func aRefusalSaysWhyTheChangeWasntMade() {
        let notice = Notice(.refused(command: .on, why: .nightShiftUnavailable), frontApp: nil, words: .staged)

        #expect(notice?.title == "Nightcap couldn’t make that change")
        #expect(notice?.body == "Night Shift isn’t available on this Mac.")
    }

    @Test func aRefusalNamesTheRuleItsAboutAsTheOtherNoticesDo() {
        let refused = { (why: Refusal) in
            Notice(.refused(command: .toggle, why: why), frontApp: figma, words: .staged)?.body
        }

        #expect(
            refused(.configSays(identity: .app("com.figma.desktop"), says: .off))
                == "Your config’s rule for Figma keeps Night Shift off. Change it there.")
        #expect(
            refused(.configSays(identity: .domain("figma.com"), says: .on))
                == "Your config’s rule for figma.com keeps Night Shift on. Change it there.")
        #expect(
            refused(.configRule(identity: .app("com.apple.mail")))
                == "The rule for com.apple.mail is your config’s. Change it there.")
        #expect(refused(.noSuchRule(identity: .domain("figma.com"))) == "There’s no rule for figma.com.")
    }

    @Test func aRefusalSaysWhatItCarries() {
        let refused = { (why: Refusal) in
            Notice(.refused(command: .toggle, why: why), frontApp: nil, words: .staged)?.body
        }

        #expect(refused(.strengthUnavailable) == "Night Shift’s strength can’t be read on this Mac.")
        #expect(refused(.strengthOutOfRange(strength: 101)) == "A strength is 0 to 100, not 101.")
        #expect(refused(.notATimeOfDay(TimeOfDay(hour: 24, minute: 0))) == "Custom hours are times of day.")
        #expect(refused(.tooShort) == "A length is longer than zero.")
        #expect(refused(.tooLong(length: 3_600)) == "That length is too long.")
        #expect(refused(.passed) == "That time has passed.")
        #expect(refused(.noSchedule) == "There’s no schedule to wait for.")
        #expect(
            refused(.indefiniteIsAdded) == "Only everywhere is set for good: an app or a site gets a rule instead.")
        #expect(
            refused(.adHocRulesOff(.newer(path: "/n/rules.toml", version: 3)))
                == "Ad-hoc rules are off: /n/rules.toml is version 3, from a newer Nightcap than this one.")
        let notSaved: [(IoFailure, String)] = [
            (.notFound, "Couldn’t save /n/rules.toml: a folder on its path isn’t there."),
            (.permissionDenied, "Nightcap isn’t allowed to save /n/rules.toml."),
            (.isAFolder, "Couldn’t save /n/rules.toml: a folder is in its place."),
            (.notAFolder, "Couldn’t save /n/rules.toml: one of the folders on its path is a file."),
            (.notUtf8, "Couldn’t save /n/rules.toml: it isn’t UTF-8 text."),
            (.other, "Couldn’t save /n/rules.toml."),
        ]
        for (failure, body) in notSaved {
            #expect(refused(.adHocRulesNotSaved(path: "/n/rules.toml", failure: failure)) == body)
        }
        #expect(
            refused(.invalidConfig(.malformed(path: "/n/config.toml", line: 2, message: "expected a sequence")))
                == "Your config is invalid: /n/config.toml: line 2: expected a sequence.")
        #expect(refused(.websiteCheckingInConfig) == "Your config sets website checking, with `websites`.")
        #expect(refused(.noUpdater(.devBuild)) == "Dev builds don’t update themselves: rebuild instead.")
        #expect(refused(.noUpdater(.managedInstall)) == "This install is kept up to date by its installer.")
        #expect(refused(.noLoginItem(.devBuild)) == "Dev builds don’t open themselves at login.")
        #expect(
            refused(.noLoginItem(.managedInstall))
                == "This install doesn’t open itself at login: its installer starts it.")
        #expect(
            refused(.nothingToAccept(check: .notifications))
                == "Notifications are as you set them up, so there’s nothing to accept.")
        #expect(
            refused(.nothingToAccept(check: .nightShift))
                == "Night Shift’s schedule is as you set it up, so there’s nothing to accept.")
    }
}
