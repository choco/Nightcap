import Foundation
import NightcapCore

/// The core's answers as the menu-bar UI reads them.
extension OnOff {
    var other: OnOff { self == .on ? .off : .on }
}

extension Length {
    /// When a rule of this length ends, if it has an end.
    var end: Date? {
        switch self {
        case .indefinite: nil
        case .untilScheduleTurns(let until): until
        case .until(let until): until
        }
    }
}
