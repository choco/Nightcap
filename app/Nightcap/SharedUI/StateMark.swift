import AppKit
import NightcapCore

/// What a state mark says. Checks use the first four; onboarding's steps use all six.
enum StateMark {
    case ok, off, drifted, broken, todo, current

    init(_ state: CheckState) {
        self =
            switch state {
            case .ok: .ok
            case .off: .off
            case .drifted: .drifted
            case .broken: .broken
            }
    }

    /// An 18-point mark: a white sign on a filled circle or triangle, a grey dash, a hollow
    /// circle for a step not reached yet, or the current step's ring and pulsing dot.
    func view() -> NSView {
        let symbol: (name: String, colors: [NSColor], description: String)
        let words = Wording.system
        switch self {
        case .ok: symbol = ("checkmark.circle.fill", [.white, .nightcapGreen], words.mark(.ok))
        case .off: symbol = ("minus.circle", [.nightcapGrey], words.mark(.off))
        case .drifted: symbol = ("exclamationmark.triangle.fill", [.white, .nightcapOrange], words.mark(.drifted))
        case .broken: symbol = ("xmark.circle.fill", [.white, .nightcapRed], words.mark(.broken))
        case .todo: return StepRing(current: false)
        case .current: return StepRing(current: true)
        }
        let configuration = NSImage.SymbolConfiguration(pointSize: 15, weight: .semibold)
            .applying(NSImage.SymbolConfiguration(paletteColors: symbol.colors))
        let image = NSImageView(
            image: NSImage(systemSymbolName: symbol.name, accessibilityDescription: symbol.description)!
                .withSymbolConfiguration(configuration)!)
        image.widthAnchor.constraint(equalToConstant: 18).isActive = true
        image.heightAnchor.constraint(equalToConstant: 18).isActive = true
        return image
    }
}
