import Foundation
import NightcapCore

/// The menu's own words, beside the place rows' and the Night Shift item's line: its fixed items,
/// the moon as VoiceOver reads it, the problem line, Custom…'s window, and "Nightcap is up here"
/// under the moon.
extension Wording {
    /// Night Shift's name as a title: the Night Shift item's, its block's in Settings, and
    /// onboarding's for its part and for the step that keeps a schedule.
    var nightShift: String {
        String(
            localized: "Night Shift", bundle: #bundle, locale: locale,
            comment: """
                Night Shift’s name standing alone, as a title: the menu’s first item, its block in Settings, \
                and onboarding’s first part and its step for a Mac already on a schedule.
                """)
    }

    /// The strength's slider, as VoiceOver reads it.
    var strength: String { text("Strength") }

    var settings: String { text("Settings…") }

    var quitNightcap: String { text("Quit Nightcap") }

    /// The moon, as VoiceOver reads it: "Night Shift forced off".
    func moon(_ moon: Menu.Moon) -> String {
        switch moon {
        case .on: text("Night Shift on")
        case .off: text("Night Shift off")
        case .forcedOff: text("Night Shift forced off")
        case .unavailable: text("Night Shift unavailable")
        }
    }

    /// The moon with the problem line's dot, as VoiceOver reads it: "Night Shift on, needs
    /// attention".
    func needingAttention(_ moon: Menu.Moon) -> String {
        text("\(self.moon(moon)), needs attention")
    }

    /// The problem line's triangle, as VoiceOver reads it.
    var needsAttention: String { text("Needs attention") }

    /// The problem line, which counts the problems: "2 Things Need Attention…".
    func thingsNeedAttention(_ count: Int) -> String {
        text("\(count) Things Need Attention…")
    }

    /// What the problem line counts, on the line under it: "Accessibility and notifications".
    func needing(_ problems: [CheckDetail]) -> String {
        let names = problems.map { problem in
            switch problem {
            case .nightShift:
                String(
                    localized: "Night Shift, among the problems", defaultValue: "Night Shift", bundle: #bundle,
                    locale: locale,
                    comment: """
                        Night Shift’s check, as the line under the menu’s problem line lists it among the others, \
                        mid-sentence, with only the line’s first word capitalized: “Night Shift and notifications”.
                        """)
            case .accessibility: text("Accessibility")
            case .notifications: text("notifications")
            case .openAtLogin: text("opening at login")
            case .configFile: text("the config file")
            case .adHocRules: text("ad-hoc rules")
            }
        }
        let line = listed(names)
        return line.prefix(1).uppercased(with: locale) + line.dropFirst()
    }

    /// "a", "a and b", "a, b and c".
    private func listed(_ parts: [String]) -> String {
        guard let first = parts.first, let last = parts.last, parts.count > 1 else { return parts.first ?? "" }
        let rest = parts.dropFirst().dropLast().reduce(first) { list, part in text("\(list), \(part)") }
        return text("\(rest) and \(last)")
    }

    /// Custom…'s window: the label beside the time.
    var untilTime: String { text("Until") }

    var cancel: String { text("Cancel") }

    /// Under the time Custom…'s window shows, how long until then: "In 1 hour, 15 minutes".
    func inTime(_ seconds: TimeInterval) -> String {
        let formatter = DateComponentsFormatter()
        formatter.allowedUnits = [.hour, .minute]
        formatter.unitsStyle = .full
        var calendar = Calendar.current
        calendar.locale = locale
        formatter.calendar = calendar
        let length = formatter.string(from: seconds) ?? ""
        // A key of its own: "In Figma" shares the English, but not every language says both alike.
        return String(
            localized: "In %@, a length of time", defaultValue: "In \(length)", bundle: #bundle, locale: locale)
    }

    /// "Nightcap is up here", under the moon once onboarding is done.
    var upHere: (title: String, body: String) {
        (
            text("Nightcap is up here"),
            text(
                """
                Its menu turns Night Shift on or off, everywhere or just in the app in front, for as long as you \
                choose, and changes its strength: click OK, then the moon.
                """) + " " + text("New to Nightcap? The tour shows a rule at work, in about a minute.")
        )
    }

    /// In place of "Nightcap is up here" while macOS hides the moon.
    var noRoomForTheMoon: (title: String, body: String) {
        (
            text("No room for Nightcap’s moon"),
            text("The menu bar is full, so macOS hides the moon, and with it Nightcap’s menu.") + " "
                + text("Quit an app with an icon up there and the moon appears.") + " "
                + text("Until then, open Nightcap again for its Settings.")
        )
    }

    var ok: String { text("OK") }

    var takeTheTour: String { text("Take the Tour") }
}
