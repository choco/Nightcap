import Foundation
import NightcapCore

/// The sentences the menu, the tour and the notifications show, from the String Catalog beside
/// this file: in the user's language, with its plural rules, and times in the user's clock. Each
/// takes what its sentence is about, such as a place by its name, on or off, a length or a time,
/// never a snapshot of the core.
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

    /// Whose a rule is.
    enum Whose {
        case config, yours, tour
    }

    /// A change at a place for a while: on or off, until a time, or with no end until the user
    /// turns it back.
    struct Change: Equatable {
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
        date.formatted(Date.FormatStyle(date: .omitted, time: .shortened, locale: locale, timeZone: timeZone))
    }

    /// Whose rule decides, as a line names it: "your config’s rule".
    func whose(_ whose: Whose) -> String {
        switch whose {
        case .config: text("your config’s rule")
        case .yours: text("your rule")
        case .tour: text("the tour’s rule")
        }
    }
}
