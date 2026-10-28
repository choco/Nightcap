import Foundation
import NightcapCore

/// The checks' words: what each found, as Settings shows it under its setting, the buttons of
/// the fixes it offers, and what's wrong with one of Nightcap's files.
extension Wording {
    /// What a check found, in a sentence: "Valid, with 1 rule."
    func check(_ detail: CheckDetail) -> String {
        switch detail {
        case .nightShift(let found): nightShift(found)
        case .accessibility(.allowed): text("Accessibility is allowed.")
        case .accessibility(.notAllowed):
            text("Accessibility isn’t allowed, so website checking can’t read the front tab.")
        case .accessibility(.notNeeded): text("Not needed while website checking is off.")
        case .notifications(.allowed): text("Allowed.")
        case .notifications(.switchedOff):
            text("Notifications were allowed, but they’re switched off in System Settings.")
        case .notifications(.off): text("Notifications are off.")
        case .openAtLogin(.opens): text("Nightcap opens when you log in.")
        case .openAtLogin(.awaitingApproval): text("macOS is waiting for your approval in Login Items.")
        case .openAtLogin(.removed): text("Nightcap no longer opens at login: it was removed in System Settings.")
        case .openAtLogin(.off): text("Nightcap doesn’t open at login.")
        case .configFile(.valid(let rules)):
            rules == 0 ? text("Valid, with no rules.") : text("Valid, with \(Int(rules)) rules.")
        case .configFile(.missing): text("No config file.")
        case .configFile(.error(let error)): inTheFile(error)
        case .adHocRules(.on(let rules)):
            rules == 0 ? text("No ad-hoc rules.") : text("\(Int(rules)) ad-hoc rules.")
        case .adHocRules(.off(let error)): text("Ad-hoc rules are off: \(located(error)).")
        }
    }

    /// Night Shift's check, with its schedule's hours in the user's clock.
    private func nightShift(_ found: NightShiftDetail) -> String {
        switch found {
        case .available(nil): text("Available.")
        case .available(.off?): text("Available, with no schedule.")
        case .available(.sunsetToSunrise?): text("Available, following its schedule: sunset to sunrise.")
        case .available(.custom(let from, let to)?):
            text("Available, following its schedule: \(time(from)) to \(time(to)).")
        case .unavailable: text("Night Shift isn’t available on this Mac.")
        case .setAside:
            text("Sunset to sunrise needs Location Services, so macOS turned the schedule off until it’s back on.")
        case .noSunTimes:
            text("macOS hasn’t worked out sunrise and sunset, so Nightcap can’t follow the schedule.") + " "
                + text("Custom hours work without them.")
        }
    }

    /// Where in the file and what, without the path, which Settings shows under the config's
    /// setting, its path: "Line 4: unknown field `domian`, expected `app` or `domain`." An
    /// invalid entry or value with no line keeps its first letter: it starts with the key or
    /// entry it names, as the file writes it.
    private func inTheFile(_ error: FileError) -> String {
        switch error {
        case .unreadable(_, let message): text("Couldn't read it: \(message).")
        case .malformed(_, let line?, let message), .invalid(_, let line?, let message):
            text("Line \(Int(line)): \(message).")
        case .malformed(_, nil, let message): text("\(message.prefix(1).uppercased() + message.dropFirst()).")
        case .invalid(_, nil, let message): text("\(message).")
        case .newer(_, let version): text("It’s version \(Int(version)), from a newer Nightcap than this one.")
        }
    }

    /// What's wrong with a file, starting from its path: "/…/rules.toml: line 3: invalid type".
    func located(_ error: FileError) -> String {
        switch error {
        case .unreadable(let path, let message):
            text("couldn't read \(path): \(message)")
        case .malformed(let path, let line?, let message), .invalid(let path, let line?, let message):
            text("\(path): line \(Int(line)): \(message)")
        case .malformed(let path, nil, let message), .invalid(let path, nil, let message):
            text("\(path): \(message)")
        case .newer(let path, let version):
            text("\(path) is version \(Int(version)), from a newer Nightcap than this one")
        }
    }

    /// A fix's button.
    func title(of fix: Fix) -> String {
        switch fix {
        case .askForAccessibility: text("Allow…")
        case .allowNotifications: text("Allow Notifications…")
        case .openLoginItems: text("Open Login Items…")
        case .openLocationServices: text("Open Location Services…")
        case .stopCheckingWebsites: text("Stop Checking Websites")
        case .keepNotificationsOff: text("Keep Notifications Off")
        case .keepScheduleOff: text("Keep the Schedule Off")
        case .openAtLogin: text("Open at Login")
        case .dontOpenAtLogin: text("Don’t Open at Login")
        case .reload: text("Reload")
        }
    }

    /// Where a check stands, as its mark tells VoiceOver.
    func mark(_ state: CheckState) -> String {
        switch state {
        case .ok: text("OK")
        case .off: text("Off")
        case .drifted: text("Needs attention")
        case .broken: text("Broken")
        }
    }
}
