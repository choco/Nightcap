import Foundation
import NightcapCore

/// Whose write each Night Shift status report is the first to show (ADR-0004). CoreBrightness
/// marks every write, Nightcap's and Control Center's alike, and none of the schedule's
/// switches; only the adapter knows which writes it made. Its mark can come after `setEnabled:`
/// returns, so the write is told by the report that first shows it.
struct NightShiftWrites {
    /// The write mark the last report showed.
    private var lastMark: Date?
    /// What Nightcap last wrote, until a report shows a new write.
    private var onItsWay: Bool?

    /// Whether writing `enabled` would add anything to Night Shift as it is, `current`: not while
    /// it's already so, which a write would leave unreported, nor while the same write is on its
    /// way.
    func needs(_ enabled: Bool, current: Bool?) -> Bool {
        current != enabled && onItsWay != enabled
    }

    /// Nightcap wrote `enabled`.
    mutating func wrote(_ enabled: Bool) {
        onItsWay = enabled
    }

    /// Whose write a report of Night Shift `enabled`, marked as written at `writtenAt`, is the
    /// first to show.
    mutating func report(enabled: Bool, writtenAt: Date?) -> SwitchedBy {
        defer { lastMark = writtenAt }
        guard let writtenAt, writtenAt != lastMark else { return .schedule }
        defer { onItsWay = nil }
        return onItsWay == enabled ? .nightcap : .outside
    }
}
