import AppKit

/// A strength slider, 0 to 100, for the menu and onboarding. Its `mouseDown` tracks the whole
/// drag and returns only once the mouse goes up, so moves inside it are a drag, which previews,
/// and its return is the release, which saves.
final class StrengthSlider: NSSlider {
    private(set) var dragging = false
    var onRelease: () -> Void = {}

    override func mouseDown(with event: NSEvent) {
        dragging = true
        super.mouseDown(with: event)
        dragging = false
        onRelease()
    }
}
