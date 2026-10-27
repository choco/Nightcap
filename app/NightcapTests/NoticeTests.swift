import Foundation
import NightcapCore
import Testing

private let zurich = TimeZone(identifier: "Europe/Zurich")!
private let figma = Notice.FrontApp(bundleId: "com.figma.Desktop", name: "Figma")

/// 07:00 on 17 October 2026 in Zurich.
private let morning = Date(timeIntervalSince1970: 1_792_213_200)

/// The notice for a check that became a problem.
private func problem(_ detail: CheckDetail, _ state: CheckState) -> Notice? {
    Notice(.problemAppeared(check: Check(detail: detail, state: state)), frontApp: nil, timeZone: zurich)
}

/// What each event from the core says as a notification.
struct NoticeTests {
    @Test func anExternalChangeNamesTheSnoozedRuleByItsAppInFrontAndItsEnd() {
        let adopted = Event.adopted(
            place: .app("com.figma.Desktop"), snoozed: .app("com.figma.Desktop"), state: .on, until: morning)

        let notice = Notice(adopted, frontApp: figma, timeZone: zurich)

        #expect(notice?.title == "Night Shift turned on outside Nightcap")
        #expect(notice?.body == "Snoozed the Figma rule until 07:00.")
    }

    @Test func aRuleWhoseAppIsntInFrontGoesByItsBundleIdAndADomainRuleByItsDomain() {
        let app = Event.adopted(
            place: .app("com.apple.mail"), snoozed: .app("com.apple.mail"), state: .off, until: nil)
        let site = Event.adopted(
            place: .domain("figma.com"), snoozed: .domain("figma.com"), state: .on, until: morning)

        #expect(
            Notice(app, frontApp: figma, timeZone: zurich)?.body
                == "Snoozed the com.apple.mail rule until you turn it on.")
        #expect(Notice(site, frontApp: figma, timeZone: zurich)?.body == "Snoozed the figma.com rule until 07:00.")
    }

    @Test func anExternalChangeThatSnoozedNoRuleTellsNothing() {
        let adopted = Event.adopted(place: .everywhere, snoozed: nil, state: .on, until: morning)

        #expect(Notice(adopted, frontApp: figma, timeZone: zurich) == nil)
    }

    @Test func anUpdateOpensItsVersionInTheChangelog() {
        let notice = Notice(.updated(from: "1.1.0", to: "1.2.0"), frontApp: nil, timeZone: zurich)

        #expect(notice?.title == "Nightcap updated to 1.2.0")
        #expect(notice?.body == "Click to see what’s new.")
        #expect(notice?.opens == URL(string: "https://nightcap.choco.me/changelog/#v1.2.0"))
    }

    @Test func aProblemSaysWhatAppeared() {
        let night = problem(.nightShift(.setAside), .drifted)
        #expect(night?.title == "Sunset to sunrise needs Location Services")
        #expect(
            night?.body
                == "macOS turned Night Shift’s schedule off. Open Nightcap’s settings to turn Location Services "
                + "back on, or to keep the schedule off.")
        let sun = problem(.nightShift(.noSunTimes(waited: 600)), .broken)
        #expect(sun?.title == "Nightcap can’t follow sunset to sunrise")
        #expect(
            sun?.body
                == "macOS hasn’t worked out sunrise and sunset. Custom hours in Night Shift’s settings work "
                + "without them.")
        let accessibility = problem(.accessibility(.notAllowed), .drifted)
        #expect(accessibility?.title == "Website checking needs Accessibility")
        #expect(accessibility?.body == "Open Nightcap’s settings to allow it, or to stop checking websites.")
        let login = problem(.openAtLogin(.removed), .drifted)
        #expect(login?.title == "Nightcap no longer opens at login")
        #expect(login?.body == "Open Nightcap’s settings to turn it back on, or to keep it off.")
    }

    @Test func aFilesProblemSaysWhatsWrongFromItsPath() {
        let config = { (error: FileError) in problem(.configFile(.error(error)), .broken) }
        let malformed = config(
            .malformed(
                path: "/n/config.toml", line: 2, message: "unknown field `domian`, expected `app` or `domain`"))
        #expect(malformed?.title == "Nightcap’s config is invalid")
        #expect(malformed?.body == "/n/config.toml: line 2: unknown field `domian`, expected `app` or `domain`")
        #expect(
            config(.invalid(path: "/n/config.toml", line: 3, message: "night_shift.strength is 0 to 100, not 101"))?
                .body == "/n/config.toml: line 3: night_shift.strength is 0 to 100, not 101")
        #expect(
            config(.unreadable(path: "/n/config.toml", message: "Permission denied (os error 13)"))?.body
                == "couldn't read /n/config.toml: Permission denied (os error 13)")

        let rules = { (error: FileError) in problem(.adHocRules(.off(error)), .broken) }
        let newer = rules(.newer(path: "/n/rules.toml", version: 3))
        #expect(newer?.title == "Ad-hoc rules are off")
        #expect(newer?.body == "/n/rules.toml is version 3, from a newer Nightcap than this one")
        #expect(
            rules(.malformed(path: "/n/rules.toml", line: nil, message: "expected a table"))?.body
                == "/n/rules.toml: expected a table")
        #expect(
            rules(.invalid(path: "/n/rules.toml", line: nil, message: "app:a/b isn't a bundle ID"))?.body
                == "/n/rules.toml: app:a/b isn't a bundle ID")
    }

    @Test func whatTheCoreNeverAnnouncesTellsNothing() {
        #expect(problem(.nightShift(.unavailable), .broken) == nil)
        #expect(problem(.notifications(.switchedOff), .drifted) == nil)
        #expect(problem(.openAtLogin(.awaitingApproval), .drifted) == nil)
        #expect(problem(.configFile(.valid(rules: 1)), .ok) == nil)
    }

    @Test func aRefusalTellsNothing() {
        let refused = Event.refused(command: .off, why: ErrorReply(code: .nightShiftUnavailable, message: ""))

        #expect(Notice(refused, frontApp: nil, timeZone: zurich) == nil)
    }
}
