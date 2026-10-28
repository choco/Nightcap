import Foundation
import NightcapCore

/// The sentences the menu, the tour, the notifications, Settings and onboarding show, from the
/// String Catalog beside this file: in the user's language, with its plural rules, and times in
/// the user's clock. Each takes what its sentence is about, such as a place by its name, on or
/// off, a length or a time, never a snapshot of the core.
struct Wording {
    /// Formats times and numbers, and picks a plural's form.
    let locale: Locale
    /// The zone a time is told in.
    let timeZone: TimeZone

    /// The user's language, clock and time zone, as they change.
    static let system = Wording(locale: .autoupdatingCurrent, timeZone: .autoupdatingCurrent)

    /// An app or a site, as a sentence names it. Everywhere's sentences are their own.
    enum Place: Equatable {
        /// By the name the user sees, or its bundle ID where the name isn't known.
        case app(name: String)
        case site(domain: String)
    }

    /// Whose a rule is. A trial's is the tour's, the only trial the app begins.
    enum Whose {
        case config, yours, tour, outsideChange
    }

    /// A temporary rule as the words tell it: on or off, until a time, or with no end until the
    /// user turns it back.
    struct TemporaryRule: Equatable {
        let state: OnOff
        let until: Date?
    }

    /// `value` from the catalog in this bundle: the app's, or the test bundle's, which compiles
    /// these files too.
    func text(_ value: String.LocalizationValue) -> String {
        String(localized: value, bundle: #bundle, locale: locale)
    }

    /// A time of day, in the user's clock: "07:00", or "7:00 AM".
    func time(_ date: Date) -> String {
        date.formatted(clock(in: timeZone))
    }

    /// A time of day, in the user's clock: "22:00", or "10:00 PM".
    func time(_ time: TimeOfDay) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = .gmt
        let date = calendar.date(
            from: DateComponents(year: 2001, month: 1, day: 1, hour: Int(time.hour), minute: Int(time.minute)))!
        return date.formatted(clock(in: .gmt))
    }

    /// How every time reads, in `zone`: two-digit hours on a 24-hour clock, "07:00", and AM or PM
    /// on a 12-hour one, "7:00 AM", as the Custom… window's picker and the weekday formats write
    /// them. The short time style writes "7:00" on some 24-hour clocks, such as English
    /// (Switzerland)'s.
    func clock(in zone: TimeZone) -> Date.FormatStyle {
        Date.FormatStyle(locale: locale, timeZone: zone).hour(.defaultDigits(amPM: .abbreviated)).minute()
    }

    /// Whose rule decides, as a line names it: "your config’s rule".
    func whose(_ whose: Whose) -> String {
        switch whose {
        case .config: text("your config’s rule")
        case .yours: text("your rule")
        case .tour: text("the tour’s rule")
        case .outsideChange: text("the outside change’s rule")
        }
    }
}
