import Foundation
import NightcapCore

extension Wording {
    /// English with a 24-hour clock, in the staged cores' time zone, so a time reads "07:00", as
    /// the core writes it.
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

/// 22:00 to 07:00, local time.
let customHours = Schedule.custom(from: TimeOfDay(hour: 22, minute: 0), to: TimeOfDay(hour: 7, minute: 0))

/// Night Shift as CoreBrightness reports it, with sunset to sunrise not set aside.
func reported(_ enabled: Bool, _ schedule: Schedule = customHours) -> Step {
    .fact(.statusChanged(enabled: enabled, schedule: schedule, sunScheduleSetAside: false))
}

/// An app comes to the front, by its name where it has one.
func front(_ bundleId: String, _ name: String? = nil, browser: Bool = false) -> Step {
    .fact(.appActivated(bundleId: bundleId, name: name, isBrowser: browser))
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

/// The tour started with Tour Sample, Nightcap's site in Safari and Mail as the example.
let tourStarted = Step.uiCommand(
    .startTour(
        sample: TourApp(bundleId: tourSample, name: "Tour Sample"),
        site: TourSite(domain: "nightcap.choco.me", browser: "com.apple.Safari"),
        example: TourApp(bundleId: "com.apple.mail", name: "Mail")))

let tourSample = "io.github.choco.nightcap.tour-sample"

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
            steps: [reported(true), front("com.figma.Desktop", "Figma"), reported(false)] + steps))
}

/// The config's lengths, as `[menu] lengths` lists them.
func lengths(_ lengths: [String]) -> String {
    "[menu]\nlengths = [\(lengths.map { "\"\($0)\"" }.joined(separator: ", "))]\n"
}

let figmaApp = Place.app("com.figma.Desktop")
let mailApp = Place.app("com.apple.mail")

func minutes(_ count: Int64) -> Lasting {
    .for(count * 60)
}

extension Core {
    /// The line under the Night Shift item.
    var line: String? { menu().nightShift.reason }

    /// The menu's place rows, top down.
    var rows: [PlaceRow] {
        menu().entries.compactMap { entry in
            if case .row(let row) = entry { row } else { nil }
        }
    }

    var everywhereRow: PlaceRow? { rows.first { $0.image == .everywhere } }

    var appRow: PlaceRow? {
        rows.first { row in
            if case .app = row.image { true } else { false }
        }
    }

    var siteRow: PlaceRow? { rows.first { $0.image == .site } }
}

extension PlaceRow {
    /// The row's item that sends `command`.
    func item(sending command: Command) -> MenuItem? {
        items.lazy.compactMap { entry in
            if case .choice(let item) = entry, item.command == command { item } else { nil }
        }.first
    }

    /// The title of Custom…'s window, where the row offers it.
    var timePickerTitle: String? {
        items.lazy.compactMap { entry in
            if case .pickTime(let title, _, _, _) = entry { title } else { nil }
        }.first
    }
}
