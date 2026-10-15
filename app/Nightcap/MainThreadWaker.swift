import Dispatch
import NightcapCore
import Synchronization

/// Turns the core's wake-ups, which arrive on its own threads, into one main-thread tick at a
/// time: wakes that come in while a tick is already queued are merged into it (ADR-0002).
nonisolated final class MainThreadWaker: Waker {
    private let tickQueued = Mutex(false)
    private let onTick: @MainActor @Sendable () -> Void

    init(onTick: @escaping @MainActor @Sendable () -> Void) {
        self.onTick = onTick
    }

    func wake() {
        let alreadyQueued = tickQueued.withLock { queued in
            defer { queued = true }
            return queued
        }
        guard !alreadyQueued else { return }
        DispatchQueue.main.async { [self] in
            tickQueued.withLock { $0 = false }
            MainActor.assumeIsolated { onTick() }
        }
    }
}
