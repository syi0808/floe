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
        .library(name: "FloeAppleWellbeing", targets: ["FloeAppleWellbeing"]),
    ],
    dependencies: [.package(path: "../../native/FloeNative")],
    targets: [
        .target(name: "FloeAppleHealth"),
        .target(name: "FloeAppleWellbeing", dependencies: [
            "FloeAppleHealth",
            .product(name: "FloeHealthTransform", package: "FloeNative"),
            .product(name: "FloeHealthTransformBridge", package: "FloeNative"),
        ]),
    ]
)
