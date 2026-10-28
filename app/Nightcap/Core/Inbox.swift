import Foundation
import NightcapCore
import Synchronization

/// The core's one listener (ADR-0002). It takes each outcome, with the snapshot its input left, on
/// the core's thread and hands them to `receive` on the main queue, in the order the core told
/// them; the main queue keeps that order.
// A conformer to a Sendable protocol isn't inferred to the main actor, so it says so.
@MainActor final class Inbox: Listener {
    var receive: (Outcome, Snapshot) -> Void = { _, _ in }
    private let arrived = Mutex<[(Outcome, Snapshot)]>([])

    nonisolated func outcome(outcome: Outcome, snapshot: Snapshot) {
        arrived.withLock { $0.append((outcome, snapshot)) }
        DispatchQueue.main.async { [self] in
            MainActor.assumeIsolated { handOver() }
        }
    }

    /// Hands over every outcome that arrived, in order. Quitting takes them at once, ahead of the
    /// main queue's turn.
    func handOver() {
        let outcomes = arrived.withLock { arrived in
            defer { arrived.removeAll() }
            return arrived
        }
        for (outcome, snapshot) in outcomes { receive(outcome, snapshot) }
    }
}
