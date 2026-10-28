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
    /// setting, its path: "Line 4, column 12 isn’t valid TOML."
    private func inTheFile(_ error: FileError) -> String {
        switch error {
        case .unreadable(_, let failure): unreadable(failure)
        case .malformed(_, let at?): text("Line \(Int(at.line)), column \(Int(at.column)) isn’t valid TOML.")
        case .malformed(_, nil): text("It isn’t valid TOML.")
        case .invalid(_, let line, let problem): text("Line \(Int(line)): \(self.problem(problem)).")
        case .newer(_, let version): text("It’s version \(Int(version)), from a newer Nightcap than this one.")
        }
    }

    /// Why macOS couldn't read a file, in a sentence about it: "It’s a folder, not a file."
    private func unreadable(_ failure: IoFailure) -> String {
        switch failure {
        case .notFound: text("It, or a folder on its path, isn’t there.")
        case .permissionDenied: text("Nightcap isn’t allowed to read it.")
        case .isAFolder: text("It’s a folder, not a file.")
        case .notAFolder: text("One of the folders on its path is a file.")
        case .notUtf8: text("It isn’t UTF-8 text.")
        case .other: text("Nightcap couldn’t read it.")
        }
    }

    /// What's wrong with a file, starting from its path: "/…/rules.toml: line 3, column 7 isn’t
    /// valid TOML".
    func located(_ error: FileError) -> String {
        switch error {
        case .unreadable(let path, let failure): unreadable(path, failure)
        case .malformed(let path, let at?):
            text("\(path): line \(Int(at.line)), column \(Int(at.column)) isn’t valid TOML")
        case .malformed(let path, nil): text("\(path) isn’t valid TOML")
        case .invalid(let path, let line, let problem):
            text("\(path): line \(Int(line)): \(self.problem(problem))")
        case .newer(let path, let version):
            text("\(path) is version \(Int(version)), from a newer Nightcap than this one")
        }
    }

    /// Why macOS couldn't read the file at `path`, starting from it: "/…/config.toml is a folder,
    /// not a file".
    private func unreadable(_ path: String, _ failure: IoFailure) -> String {
        switch failure {
        case .notFound: text("\(path), or a folder on its path, isn’t there")
        case .permissionDenied: text("\(path): Nightcap isn’t allowed to read it")
        case .isAFolder: text("\(path) is a folder, not a file")
        case .notAFolder: text("\(path): one of the folders on its path is a file")
        case .notUtf8: text("\(path) isn’t UTF-8 text")
        case .other: text("\(path) couldn’t be read")
        }
    }

    /// What makes a key, an entry or a value in a file invalid, from the key or entry as the file
    /// writes it, which keeps its first letter: "night_shift.strength is 0 to 100, not 101".
    private func problem(_ problem: Problem) -> String {
        switch problem {
        case .unknownKey(_, let key, let expected) where expected.isEmpty: text("`\(key)` isn’t a key here")
        case .unknownKey(_, let key, let expected): text("`\(key)` isn’t a key here; use \(either(expected))")
        case .wrongType(let table, let key, let found, let expected):
            text("`\(dotted(table, key))` is \(valueType(expected)), not \(valueType(found))")
        case .missingKey(let table, let key): text("`\(dotted(table, key))` is missing")
        case .unsaidRules: text("rules are listed by what they say, under [rules.off] and [rules.on]")
        case .notABundleId(let entry):
            text("“\(entry)” isn’t a bundle ID like com.figma.Desktop: only letters, digits, hyphens and periods")
        case .notADomain(let entry):
            text("“\(entry)” isn’t a domain like figma.com: no scheme, path, port or wildcard")
        case .bothWays(.app(let bundleId)):
            text("\(bundleId) is in both rules.off and rules.on: an app has one rule")
        case .bothWays(.domain(let domain)):
            text("\(domain) is in both rules.off and rules.on: a domain has one rule")
        case .strengthOutOfRange(let strength): text("night_shift.strength is 0 to 100, not \(Int(strength))")
        case .notASchedule(let schedule):
            text(
                """
                night_shift.schedule is “off”, “sunset-to-sunrise” or custom hours such as “22:00-07:00”, \
                not “\(schedule)”
                """)
        case .notLengths:
            text("lengths are like “30m”, “1h” or “1h30m”, each from a minute to a day, and there’s at least one")
        case .lengthsUnderMenu: text("lengths is a key of its own, before any table, not one under [menu]")
        }
    }

    /// A key with the keys that lead to its table, as the file would write it: "rules.off.app".
    private func dotted(_ table: [String], _ key: String) -> String {
        (table + [key]).joined(separator: ".")
    }

    /// Keys to pick one of, in the user's language: "`app` or `domain`".
    private func either(_ keys: [String]) -> String {
        keys.map { "`\($0)`" }.formatted(.list(type: .or).locale(locale))
    }

    /// What a value is, as a problem names it: "a list of strings".
    private func valueType(_ type: ValueType) -> String {
        switch type {
        case .string: text("a string")
        case .strings: text("a list of strings")
        case .list: text("a list that isn’t all strings")
        case .integer: text("an integer")
        case .float: text("a float")
        case .boolean: text("true or false")
        case .datetime: text("a date or a time")
        case .table: text("a table")
        }
    }

    /// The button of one of the app's own fixes.
    func title(of fix: AppFix) -> String {
        switch fix {
        case .askForAccessibility: text("Allow…")
        case .allowNotifications: text("Allow Notifications…")
        case .openLoginItems: text("Open Login Items…")
        case .openLocationServices: text("Open Location Services…")
        }
    }

    // The buttons of the fixes that are commands.
    var stopCheckingWebsites: String { text("Stop Checking Websites") }
    var keepNotificationsOff: String { text("Keep Notifications Off") }
    var keepScheduleOff: String { text("Keep the Schedule Off") }
    var openAtLoginAgain: String { text("Open at Login") }
    var dontOpenAtLogin: String { text("Don’t Open at Login") }
    var reload: String { text("Reload") }

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
