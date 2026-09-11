// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "FloeAppleContacts",
    platforms: [
        .iOS(.v15),
        .macOS(.v12),
    ],
    products: [
        .library(name: "FloeAppleContacts", targets: ["FloeAppleContacts"]),
    ],
    targets: [
        .target(name: "FloeAppleContacts"),
        .testTarget(
            name: "FloeAppleContactsTests",
            dependencies: ["FloeAppleContacts"],
            resources: [.process("Fixtures")]
        ),
    ],
    swiftLanguageModes: [.v5]
)
