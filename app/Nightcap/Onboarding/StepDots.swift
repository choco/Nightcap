import AppKit

/// Onboarding's step markers: a dot per step in groups, the current one a gold pill. Steps
/// already reached can be clicked.
final class StepDots: NSView {
    struct Dot {
        let state: OnboardingFlow.Dot.State
        let label: String
        let go: (() -> Void)?
    }

    init(groups: [[Dot]]) {
        super.init(frame: .zero)
        var views: [NSView] = []
        for (index, group) in groups.enumerated() {
            if index > 0 {
                let gap = NSView()
                gap.widthAnchor.constraint(equalToConstant: 10).isActive = true
                views.append(gap)
            }
            views += group.map(DotView.init)
        }
        addPinned(NSStackView(.horizontal, views, spacing: 0))
    }

    required init?(coder: NSCoder) {
        fatalError("not built from a nib")
    }

    /// One dot, with room around it to click.
    private final class DotView: NSView {
        private let dot: Dot

        init(_ dot: Dot) {
            self.dot = dot
            super.init(frame: .zero)
            widthAnchor.constraint(equalToConstant: dot.state == .current ? 24 : 12).isActive = true
            heightAnchor.constraint(equalToConstant: 22).isActive = true
            setAccessibilityElement(true)
            setAccessibilityRole(dot.go == nil ? .staticText : .button)
            setAccessibilityLabel(dot.state == .current ? Wording.system.current(dot.label) : dot.label)
        }

        required init?(coder: NSCoder) {
            fatalError("not built from a nib")
        }

        override func draw(_ dirtyRect: NSRect) {
            let width: CGFloat = dot.state == .current ? 18 : 6
            let pill = NSRect(x: (bounds.width - width) / 2, y: (bounds.height - 6) / 2, width: width, height: 6)
            let colour: NSColor =
                switch dot.state {
                case .current: .nightcapMoon
                case .done: .nightcapSecondary
                case .todo: .nightcapControlBorder
                }
            colour.setFill()
            NSBezierPath(roundedRect: pill, xRadius: 3, yRadius: 3).fill()
        }

        override func resetCursorRects() {
            if dot.go != nil { addCursorRect(bounds, cursor: .pointingHand) }
        }

        override func mouseDown(with event: NSEvent) {
            dot.go?()
        }

        override func accessibilityPerformPress() -> Bool {
            dot.go?()
            return dot.go != nil
        }
    }
}
