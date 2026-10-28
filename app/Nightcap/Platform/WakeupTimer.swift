import Foundation

/// The core's wakeups, on a wall-clock timer, so one fires on time after sleep or a clock change
/// (ADR-0002).
final class WakeupTimer: WakeupScheduling {
    private let fired: () -> Void
    private var timer: DispatchSourceTimer?

    /// `fired` runs on the main queue as a wakeup comes due.
    init(fired: @escaping () -> Void) {
        self.fired = fired
    }

    func schedule(at date: Date) {
        timer?.cancel()
        let seconds = date.timeIntervalSince1970.rounded(.down)
        let deadline = DispatchWallTime(
            timespec: timespec(
                tv_sec: Int(seconds),
                tv_nsec: Int((date.timeIntervalSince1970 - seconds) * 1_000_000_000)
            ))
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(wallDeadline: deadline, leeway: .seconds(1))
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            cancel()
            fired()
        }
        timer.resume()
        self.timer = timer
    }

    func cancel() {
        timer?.cancel()
        timer = nil
    }
}
