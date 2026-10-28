import AppKit

/// A step's ring: hollow and grey until it's reached, the brand's gold with a pulsing dot while
/// current.
final class StepRing: NSView {
    private let current: Bool
    private let ring = CAShapeLayer()
    private let dot = CALayer()

    init(current: Bool) {
        self.current = current
        super.init(frame: NSRect(x: 0, y: 0, width: 18, height: 18))
        wantsLayer = true
        widthAnchor.constraint(equalToConstant: 18).isActive = true
        heightAnchor.constraint(equalToConstant: 18).isActive = true
        ring.path = CGPath(ellipseIn: CGRect(x: 1.6, y: 1.6, width: 14.8, height: 14.8), transform: nil)
        ring.fillColor = nil
        ring.lineWidth = 1.5
        layer?.addSublayer(ring)
        if current {
            dot.frame = CGRect(x: 4.9, y: 4.9, width: 8.2, height: 8.2)
            dot.cornerRadius = 4.1
            layer?.addSublayer(dot)
            if !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
                let pulse = CABasicAnimation(keyPath: "opacity")
                pulse.fromValue = 1
                pulse.toValue = 0.45
                pulse.duration = 0.8
                pulse.autoreverses = true
                pulse.repeatCount = .infinity
                pulse.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
                dot.add(pulse, forKey: "pulse")
            }
        }
        setAccessibilityElement(true)
        setAccessibilityRole(.image)
        setAccessibilityLabel(Wording.system.step(current: current))
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            ring.strokeColor = (current ? NSColor.nightcapMoon : NSColor.nightcapGrey.withAlphaComponent(0.7)).cgColor
            dot.backgroundColor = NSColor.nightcapMoon.cgColor
        }
    }
}
