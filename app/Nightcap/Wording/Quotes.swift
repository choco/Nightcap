import Foundation
import NightcapCore

/// How the tour's steps quote the menu (ADR-0016): an item by its place row and its title, or a
/// row alone.
extension Wording {
    /// A menu item as the tour quotes it: the place row it's in, then the item.
    struct Quote: Equatable {
        let row: String
        let item: String
    }

    /// The item as a step quotes it: “Turn Off in Tour Sample” › “Always”.
    func quoted(_ quote: Quote) -> String {
        text("“\(quote.row)” › “\(quote.item)”")
    }

    /// A row as a step quotes it: “Turn On in Tour Sample”.
    func quoted(row: String) -> String {
        text("“\(row)”")
    }
}
