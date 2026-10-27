import Foundation
import NightcapCore

/// What a notification says about something the core told: an external change adopted as a
/// snooze, the first launch after an update, or a problem that appeared on its own.
struct Notice: Equatable {
    let title: String
    let body: String
    /// The page a click on the notification opens.
    var opens: URL?

    /// The app in front, which a snoozed rule of its own is named by.
    struct FrontApp {
        let bundleId: String
        let name: String?
    }

    /// The notice for `event`, or nil where the user isn't told: an adoption that snoozed no
    /// rule, or a refused command, which the user is looking at already. Times read in
    /// `timeZone`, as the core keeps them.
    init?(_ event: Event, frontApp: FrontApp?, timeZone: TimeZone) {
        switch event {
        case .adopted(_, let snoozed, let state, let until):
            guard let snoozed else { return nil }
            let rule: String =
                switch snoozed {
                case .app(let bundleId):
                    frontApp.flatMap { app in
                        app.bundleId.caseInsensitiveCompare(bundleId) == .orderedSame ? app.name : nil
                    } ?? bundleId
                case .domain(let domain): domain
                }
            self.init(
                title: "Night Shift turned \(Self.word(state)) outside Nightcap",
                body: "Snoozed the \(rule) rule \(Self.until(until, state, in: timeZone)).")
        case .updated(_, let to):
            self.init(
                title: "Nightcap updated to \(to)", body: "Click to see what’s new.",
                opens: URL(string: "https://nightcap.choco.me/changelog/#v\(to)"))
        case .problemAppeared(let check):
            self.init(problem: check.detail)
        case .refused:
            return nil
        }
    }

    private init(title: String, body: String, opens: URL? = nil) {
        self.title = title
        self.body = body
        self.opens = opens
    }

    /// A problem as the core announces it: never Night Shift unavailable, notifications switched
    /// off or a login item waiting for approval, nor a check that's no problem.
    private init?(problem detail: CheckDetail) {
        switch detail {
        case .nightShift(.setAside):
            self.init(
                title: "Sunset to sunrise needs Location Services",
                body: "macOS turned Night Shift’s schedule off. Open Nightcap’s settings to turn Location "
                    + "Services back on, or to keep the schedule off.")
        case .nightShift(.noSunTimes):
            self.init(
                title: "Nightcap can’t follow sunset to sunrise",
                body: "macOS hasn’t worked out sunrise and sunset. Custom hours in Night Shift’s settings "
                    + "work without them.")
        case .accessibility(.notAllowed):
            self.init(
                title: "Website checking needs Accessibility",
                body: "Open Nightcap’s settings to allow it, or to stop checking websites.")
        case .openAtLogin(.removed):
            self.init(
                title: "Nightcap no longer opens at login",
                body: "Open Nightcap’s settings to turn it back on, or to keep it off.")
        case .configFile(.error(let error)):
            self.init(title: "Nightcap’s config is invalid", body: Self.located(error))
        case .adHocRules(.off(let error)):
            self.init(title: "Ad-hoc rules are off", body: Self.located(error))
        case .nightShift(.available), .nightShift(.unavailable), .accessibility(.allowed),
            .accessibility(.notNeeded), .notifications, .openAtLogin(.opens), .openAtLogin(.awaitingApproval),
            .openAtLogin(.off), .configFile(.valid), .configFile(.missing), .adHocRules(.on):
            return nil
        }
    }

    /// What's wrong with a file, starting from its path: "/…/rules.toml: line 3: invalid type".
    private static func located(_ error: FileError) -> String {
        switch error {
        case .unreadable(let path, let message):
            "couldn't read \(path): \(message)"
        case .malformed(let path, let line?, let message), .invalid(let path, let line?, let message):
            "\(path): line \(line): \(message)"
        case .malformed(let path, nil, let message), .invalid(let path, nil, let message):
            "\(path): \(message)"
        case .newer(let path, let version):
            "\(path) is version \(version), from a newer Nightcap than this one"
        }
    }

    private static func word(_ state: OnOff) -> String {
        switch state {
        case .on: "on"
        case .off: "off"
        }
    }

    /// "until 07:00", or "until you turn it off" for a change with no end.
    private static func until(_ end: Date?, _ state: OnOff, in timeZone: TimeZone) -> String {
        guard let end else { return "until you turn it \(word(state == .on ? .off : .on))" }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = timeZone
        formatter.dateFormat = "HH:mm"
        return "until \(formatter.string(from: end))"
    }
}
