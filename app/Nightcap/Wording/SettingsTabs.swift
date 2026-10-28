import Foundation
import NightcapCore

/// The settings window's words: its tabs, each setting's title and what it does, and the Rules
/// tab's rules, what's on or off for now, and their buttons.
extension Wording {
    func title(of tab: Settings.Tab) -> String {
        switch tab {
        case .general: text("General")
        case .rules: text("Rules")
        }
    }

    // MARK: General

    var websites: String { text("Websites") }
    var checkWebsites: String { text("Check websites") }

    /// What website checking does, and what it keeps, which the config can set.
    func checkWebsitesDetail(setInConfig: Bool) -> String {
        let detail =
            text(
                "Also keep Night Shift off, or on, on the websites you choose, in Safari, Chrome, Firefox, Edge, Arc, Brave, Dia, Zen and Orion."
            )
            + " "
            + text(
                "Nightcap reads the front tab’s address with Accessibility and keeps only the site’s name, and asks the site for its icon."
            )
        return setInConfig ? detail + " " + text("Set in your config.") : detail
    }

    var scheduleAndStrength: String { text("Schedule and strength") }
    var scheduleAndStrengthDetail: String {
        text("Kept by macOS.") + " " + text("The menu’s slider sets the strength, and your config can set both.")
    }
    var openNightShiftSettings: String { text("Open Night Shift Settings…") }
    var shortcut: String { text("Shortcut") }
    var shortcutDetail: String {
        text("Tap it to turn Night Shift on or off.") + " " + text("Hold it to see true colour until you let go.")
    }
    var notifications: String { text("Notifications") }
    var notificationsDetail: String {
        text("Nightcap tells you when a change made outside it overrides one of your rules.")
    }
    var nightcap: String { text("Nightcap") }
    var openAtLogin: String { text("Open at login") }
    var tourDetail: String { text("A rule at work, the peek and the menu, in about a minute.") }

    func version(_ version: String) -> String {
        text("Version \(version)")
    }

    /// How this install updates: by itself, or from its installer (ADR-0011).
    func updates(byItself: Bool) -> String {
        byItself
            ? text("Checks for updates once a day and installs them when you quit.")
            : text("Updates come from your installer.")
    }

    var checkForUpdates: String { text("Check for Updates…") }
    var supportNightcap: String { text("Support Nightcap") }
    var supportNightcapDetail: String {
        text("It’s free and open source.") + " " + text("If it helps you, you can sponsor it on GitHub.")
    }
    var sponsor: String { text("Sponsor…") }

    // MARK: Rules

    var configFile: String { text("Config file") }
    var adHocRules: String { text("Ad-hoc rules") }
    var noRulesYet: String { text("No rules yet.") }
    var eachRule: String { text("Each keeps Night Shift off, or on, while its app or site is in front.") }
    var adHocRulesStay: String { text("Ad-hoc rules stay in Nightcap until you copy them into your config.") }
    var forNow: String { text("For now") }

    /// Under a rule's app or site: what it says, then what the rule names, "com.figma.Desktop".
    func rule(_ says: OnOff, _ names: String) -> String {
        switch says {
        case .on: text("Night Shift on · \(names)")
        case .off: text("Night Shift off · \(names)")
        }
    }

    /// An app rule's app, where this Mac has no such app.
    var notInstalled: String { text("Not installed on this Mac") }
    /// What a domain rule covers.
    var websiteWithSubdomains: String { text("Website, with its subdomains") }

    /// Where a rule comes from: the config's.
    var fromYourConfig: String { text("From your config") }
    /// Where a rule comes from: the user's, until it graduates (ADR-0007).
    var adHoc: String { text("Ad-hoc") }

    /// A rule's snooze, which says `state` until `end`: "Snoozed until 07:00", or with no end
    /// "Snoozed until you turn it off".
    func snoozed(_ state: OnOff, until end: Date?) -> String {
        switch (state, end) {
        case (_, let end?): text("Snoozed until \(time(end))")
        case (.on, nil): text("Snoozed until you turn it off")
        case (.off, nil): text("Snoozed until you turn it on")
        }
    }

    /// Ends a rule's snooze.
    var putBack: String { text("Put Back") }
    /// Removes the user's rule.
    var remove: String { text("Remove") }
    /// Ends what's on or off for now at a place.
    var end: String { text("End") }

    /// What a place's temporary rule says: "Night Shift on until 23:30".
    func forNow(_ temporary: TemporaryRule) -> String {
        switch (temporary.state, temporary.until) {
        case (.on, let until?): text("Night Shift on until \(time(until))")
        case (.off, let until?): text("Night Shift off until \(time(until))")
        case (.on, nil): text("Night Shift on until you turn it off")
        case (.off, nil): text("Night Shift off until you turn it on")
        }
    }

    var addApp: String { text("Add App…") }
    var addWebsite: String { text("Add Website…") }
    var copyForConfigAs: String { text("Copy for config as") }

    func name(of format: ExportFormat) -> String {
        switch format {
        case .nix: text("Nix")
        case .toml: text("TOML")
        }
    }

    var copy: String { text("Copy") }
    var copied: String { text("Copied") }
    var add: String { text("Add") }
    var chooseAnAppForARule: String { text("Choose an app for a rule.") }

    /// What a new rule says.
    func keepNightShift(_ state: OnOff) -> String {
        switch state {
        case .on: text("Keep Night Shift on")
        case .off: text("Keep Night Shift off")
        }
    }

    var addAWebsite: String { text("Add a Website") }
    var ruleCoversSubdomains: String { text("The rule covers it and its subdomains.") }
    var domainExample: String { text("figma.com") }

    /// What adding the typed domain adds.
    func adds(_ domain: String) -> String {
        text("Adds \(domain) and its subdomains.")
    }
}
