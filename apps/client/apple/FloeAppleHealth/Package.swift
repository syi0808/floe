// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "FloeAppleHealth",
    platforms: [
        .iOS(.v15),
        .macOS(.v13),
    ],
    products: [
        .library(name: "FloeAppleHealth", targets: ["FloeAppleHealth"]),
    ],
    targets: [
        .target(name: "FloeAppleHealth"),
        .testTarget(
            name: "FloeAppleHealthTests",
            dependencies: ["FloeAppleHealth"],
            resources: [.process("Fixtures")]
        ),
    ]
)
