import Foundation
import os

/// The strength operations behind the CoreBrightness adapter (ADR-0014). Reads validate and
/// convert its fraction to 0...100. A committed write reports what a fresh read actually says;
/// previews never report, since they are not the saved strength.
struct NightShiftStrength {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "nightshift")
    private let read: () -> Float?
    private let write: (Float, Bool) -> Bool
    private let onStrength: (UInt8) -> Void

    init(
        read: @escaping () -> Float?,
        write: @escaping (Float, Bool) -> Bool,
        onStrength: @escaping (UInt8) -> Void
    ) {
        self.read = read
        self.write = write
        self.onStrength = onStrength
    }

    var value: UInt8? {
        guard let value = read(), (0...1).contains(value) else {
            log.error("CoreBrightness didn't report a usable strength")
            return nil
        }
        return UInt8((value * 100).rounded())
    }

    func set(_ value: UInt8, commit: Bool) {
        if !write(Float(value) / 100, commit) {
            log.error("CoreBrightness refused setStrength(\(value), commit: \(commit))")
        }
        if commit, let observed = self.value { onStrength(observed) }
    }
}
