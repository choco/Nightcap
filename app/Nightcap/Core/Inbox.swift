import Foundation
import NightcapCore
import Synchronization

/// The core's one listener (ADR-0002). It takes each outcome on the core's thread and hands it to
/// `receive` on the main queue, in the order the core told them; the main queue keeps that order.
// A conformer to a Sendable protocol isn't inferred to the main actor, so it says so.
@MainActor final class Inbox: Listener {
    var receive: (Outcome) -> Void = { _ in }
    private let arrived = Mutex<[Outcome]>([])

    nonisolated func outcome(outcome: Outcome) {
        arrived.withLock { $0.append(outcome) }
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
        outcomes.forEach(receive)
    }
}
