import Foundation
import NightcapCore

/// The tint a write moves Night Shift along, from 0, none, to 1, the whole strength: straight
/// lines between the core's points, staying at the first before it and the last after it. The
/// adapter moves along it in steps, as macOS moves along its own fades, since CoreBrightness
/// eases every fade it's given and one long fade would stray from the line (ADR-0021).
struct TintCurve {
    /// How often the tint moves along a curve, each move a fade of its own as long.
    static let interval: TimeInterval = 2

    private let points: [TintPoint]

    /// None without points.
    init?(_ points: [TintPoint]) {
        guard !points.isEmpty else { return nil }
        self.points = points
    }

    func tint(at date: Date) -> Float {
        guard let after = points.firstIndex(where: { $0.at > date }) else { return points.last!.share }
        guard after > 0 else { return points.first!.share }
        let from = points[after - 1]
        let to = points[after]
        let progress = Float(date.timeIntervalSince(from.at) / to.at.timeIntervalSince(from.at))
        return from.share + (to.share - from.share) * progress
    }

    /// The move at `now`: a fade to where the curve will be a step on, or to its end where that
    /// comes sooner, which is then the last.
    func step(at now: Date) -> (tint: Float, fade: TimeInterval, last: Bool) {
        let end = points.last!.at
        guard now + Self.interval < end else {
            return (points.last!.share, max(0, end.timeIntervalSince(now)), true)
        }
        return (tint(at: now + Self.interval), Self.interval, false)
    }

    /// One fade to the next point after `now` in the time left before it, where nothing will
    /// step on: past it, macOS's own fades take over. None past the last point.
    func handOver(at now: Date) -> (tint: Float, fade: TimeInterval)? {
        points.first { $0.at > now }.map { ($0.share, $0.at.timeIntervalSince(now)) }
    }
}

extension TintPoint {
    /// The tint as a share of the whole strength, from 0 to 1.
    fileprivate var share: Float { Float(tint) / 100 }
}
