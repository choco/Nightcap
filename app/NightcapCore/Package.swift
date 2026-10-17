// swift-tools-version: 6.2

import PackageDescription

// The generated bindings get their own module because UniFFI's output breaks under
// main-actor default isolation, which the app target uses (ADR-0002).
let package = Package(
    name: "NightcapCore",
    platforms: [.macOS("15.0")],
    products: [
        .library(name: "NightcapCore", targets: ["NightcapCore"]),
    ],
    targets: [
        .binaryTarget(name: "NightcapCoreFFI", path: "NightcapCoreFFI.xcframework"),
        .target(name: "NightcapCore", dependencies: ["NightcapCoreFFI"]),
    ]
)
