import Foundation
import NightcapCore
import Testing

extension Wording {
    /// English with a 24-hour clock, in the staged cores' time zone, so a time reads "07:00".
    static let staged: Wording = {
        var english = Locale.Components(languageCode: .english, languageRegion: .unitedStates)
        english.hourCycle = .zeroToTwentyThree
        return Wording(locale: Locale(components: english), timeZone: TimeZone(identifier: "Europe/Zurich")!)
    }()
}

/// The instant `instant` names, such as "2026-10-16T23:00:00+02:00".
nonisolated func at(_ instant: String) -> Date {
    try! Date(instant, strategy: .iso8601)
}

/// Runs `test` with user defaults of its own, never the app's, empty until it writes them and
/// gone once it's done.
func withScratchDefaults(_ test: (UserDefaults) throws -> Void) rethrows {
    let suite = "NightcapTests.\(UUID().uuidString)"
    let defaults = UserDefaults(suiteName: suite)!
    defer { defaults.removePersistentDomain(forName: suite) }
    try test(defaults)
}

/// 22:00 to 07:00, local time.
let customHours = Schedule.custom(from: TimeOfDay(hour: 22, minute: 0), to: TimeOfDay(hour: 7, minute: 0))

/// Night Shift as CoreBrightness reports it, with sunset to sunrise not set aside.
func reported(_ enabled: Bool, _ schedule: Schedule = customHours) -> Step {
    .fact(.statusChanged(enabled: enabled, schedule: schedule, sunScheduleSetAside: false))
}

/// The names of the apps the tests bring to the front, as macOS tells them to the menu.
nonisolated let appNames = [
    "com.figma.Desktop": "Figma", "com.apple.mail": "Mail", "com.apple.iBooksX": "Books",
    "com.apple.Safari": "Safari", "com.googlecode.iterm2": "iTerm2", tourSample: "Tour Sample",
]

/// An app comes to the front.
func front(_ bundleId: String, browser: Bool = false) -> Step {
    .fact(.appActivated(bundleId: bundleId, isBrowser: browser))
}

/// The watched browser's host.
func host(_ host: String) -> Step {
    .fact(.hostChanged(generation: 1, host: host))
}

let accessibilityGranted = Step.fact(.permissionsChanged(accessibility: true, notifications: .granted))

/// The sun times CoreBrightness reported in Zurich around the night of 15 October 2026.
let zurichSun = Step.fact(
    .sunTimesChanged(
        sunrises: ["2026-10-15T05:49:20Z", "2026-10-16T05:50:47Z", "2026-10-17T05:52:14Z"].map(at),
        sunsets: ["2026-10-15T16:33:25Z", "2026-10-16T16:31:32Z", "2026-10-17T16:29:41Z"].map(at)))

/// The tour's trial begins, as the tour begins it finding Night Shift on.
let tourBegun = Step.appCommand(.beginTrial(standsIn: false))

/// The tour's trial begins standing in for the schedule, as the tour begins it finding Night
/// Shift off.
let tourBegunStandingIn = Step.appCommand(.beginTrial(standsIn: true))

nonisolated let tourSample = "io.github.choco.nightcap.tour-sample"

/// A core staged in Zurich at `instant`, with `config` as its config, after `steps`.
func staged(_ instant: String, config: String? = nil, _ steps: Step...) throws -> Core {
    try Core.staged(scenario: Scenario(now: at(instant), config: config, steps: steps))
}

/// At 23:00, with Night Shift on by the custom hours until 07:00.
func onByTheSchedule(config: String? = nil, _ steps: Step...) throws -> Core {
    try Core.staged(
        scenario: Scenario(now: at("2026-10-16T23:00:00+02:00"), config: config, steps: [reported(true)] + steps))
}

/// At 20:00, with Night Shift off by the custom hours until 22:00.
func offByTheSchedule(_ steps: Step...) throws -> Core {
    try Core.staged(scenario: Scenario(now: at("2026-10-16T20:00:00+02:00"), steps: [reported(false)] + steps))
}

/// At 23:00 with Night Shift on by the custom hours, then Figma in front, which a config rule
/// keeps off.
func forced(_ steps: Step...) throws -> Core {
    try Core.staged(
        scenario: Scenario(
            now: at("2026-10-16T23:00:00+02:00"), config: "[rules.off]\napp = [\"com.figma.Desktop\"]\n",
            steps: [reported(true), front("com.figma.Desktop"), reported(false)] + steps))
}

/// The config's lengths, as its `lengths` lists them.
func lengths(_ lengths: [String]) -> String {
    "lengths = [\(lengths.map { "\"\($0)\"" }.joined(separator: ", "))]\n"
}

let figmaApp = Place.app("com.figma.Desktop")
let mailApp = Place.app("com.apple.mail")

func minutes(_ count: Int64) -> Lasting {
    .for(count * 60)
}

@MainActor extension Core {
    /// The menu as the menu-bar UI decides it from this core's snapshot at `now`, in English with
    /// a 24-hour clock in Zurich, with each app in front by its name in `names`.
    func menu(at now: Date = Date(), names: [String: String] = appNames) -> Menu {
        Menu(snapshot(), now: now, appName: { names[$0] }, words: .staged)
    }

    /// An app comes to the front.
    func bringToFront(_ bundleId: String, browser: Bool = false) {
        report(fact: .appActivated(bundleId: bundleId, isBrowser: browser))
    }

    /// The watched browser's host, under the first watch.
    func reportHost(_ host: String) {
        report(fact: .hostChanged(generation: 1, host: host))
    }

    /// The line under the Night Shift item.
    var line: String? { menu().nightShift.line }

    /// The menu's place rows, top down.
    var rows: [Menu.PlaceRow] { menu().rows }

    var everywhereRow: Menu.PlaceRow? { rows.first { $0.place == .everywhere } }

    var appRow: Menu.PlaceRow? {
        rows.first { row in
            if case .app = row.place { true } else { false }
        }
    }

    var siteRow: Menu.PlaceRow? {
        rows.first { row in
            if case .domain = row.place { true } else { false }
        }
    }
}

extension Menu.PlaceRow {
    /// The row's item that sends `command`, if it offers one.
    func choice(sending command: Command) -> Menu.Choice? {
        items.lazy.compactMap { item in
            if case .choice(let choice) = item, choice.command == command { choice } else { nil }
        }.first
    }

    /// The title of Custom…'s window, where the row offers it.
    var timePickerTitle: String? {
        items.lazy.compactMap { item in
            if case .pickTime(let pick) = item { pick.title } else { nil }
        }.first
    }

    /// The row's items as they read: their titles, Custom…'s, and "—" between groups.
    var titles: [String] {
        items.map { item in
            switch item {
            case .choice(let choice): choice.title
            case .pickTime(let pick): pick.item
            case .separator: "—"
            }
        }
    }

    /// The item titled `title`, which the row must offer.
    func choice(titled title: String) throws -> Menu.Choice {
        try #require(
            items.lazy.compactMap { item in
                if case .choice(let choice) = item, choice.title == title { choice } else { nil }
            }.first, "no \(title) in \(titles)")
    }
}
