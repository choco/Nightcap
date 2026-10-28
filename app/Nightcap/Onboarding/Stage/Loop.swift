import AppKit

/// The scenes' loops, which run slowly and stop with Reduce Motion, each mock then showing how
/// its loop ends.
enum Loop {
    private static var reduced: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }

    /// Shown only in the middle of each loop, while the other app is in front.
    static func middle(_ view: NSView) {
        guard !reduced else { return }
        view.alphaValue = 0
        fades(view, values: [0, 0, 1, 1, 0, 0], times: [0, 0.28, 0.34, 0.66, 0.72, 1], duration: 7)
    }

    /// Shown at the loop's ends, gone in the middle; under Reduce Motion, gone for good.
    static func ends(_ view: NSView, lifting: Bool = false) {
        guard !reduced else {
            view.alphaValue = 0
            return
        }
        let times: [NSNumber] = lifting ? [0, 0.28, 0.36, 0.64, 0.72, 1] : [0, 0.28, 0.34, 0.66, 0.72, 1]
        fades(view, values: [1, 1, 0, 0, 1, 1], times: times, duration: 7)
    }

    static func fades(_ view: NSView, values: [CGFloat], times: [NSNumber], duration: CFTimeInterval) {
        guard !reduced else { return }
        view.wantsLayer = true
        let fade = CAKeyframeAnimation(keyPath: "opacity")
        fade.values = values
        fade.keyTimes = times
        fade.duration = duration
        fade.repeatCount = .infinity
        fade.calculationMode = .linear
        view.layer?.add(fade, forKey: "loop")
    }

    static func pulse(_ view: NSView) {
        guard !reduced else { return }
        let pulse = CABasicAnimation(keyPath: "opacity")
        pulse.fromValue = 1
        pulse.toValue = 0.15
        pulse.duration = 0.9
        pulse.autoreverses = true
        pulse.repeatCount = .infinity
        pulse.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        view.layer?.add(pulse, forKey: "pulse")
    }
}
