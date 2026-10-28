import AppKit

extension NSControl {
    /// Runs `body` as the control's action.
    func onAction(_ body: @escaping () -> Void) {
        let target = ClosureTarget(body)
        self.target = target
        action = #selector(ClosureTarget.run)
        objc_setAssociatedObject(self, &ClosureTarget.key, target, .OBJC_ASSOCIATION_RETAIN)
    }
}

/// A control's target that runs a closure; the control keeps it alive.
private final class ClosureTarget: NSObject {
    nonisolated(unsafe) static var key = 0
    private let body: () -> Void

    init(_ body: @escaping () -> Void) {
        self.body = body
    }

    @objc func run() {
        body()
    }
}
