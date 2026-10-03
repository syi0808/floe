// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "FloeNative",
    platforms: [.iOS(.v15), .macOS(.v12)],
    products: [
        .library(name: "floe_local_model", type: .dynamic, targets: ["FloeNativeHost"]),
        .library(name: "FloeModelExecution", targets: ["FloeModelExecution"]),
        .library(name: "FloeTransforms", targets: ["FloeTransforms"]),
        .library(name: "FloeHealthTransform", targets: ["FloeHealthTransform"]),
        .library(name: "FloeHealthTransformBridge", targets: ["FloeHealthTransformBridge"]),
    ],
    targets: [
        .target(name: "FloeModelExecution"),
        .target(name: "FloeTransforms"),
        .target(name: "FloeHealthTransform", dependencies: ["FloeTransforms", "FloeModelExecution"]),
        .target(name: "FloeHealthTransformBridge", dependencies: ["FloeHealthTransform", "FloeModelExecution"]),
        .target(name: "FloeFoundationModels", dependencies: ["FloeModelExecution"]),
        .target(name: "FloeNativeHost", dependencies: [
            "FloeModelExecution", "FloeFoundationModels", "FloeHealthTransform", "FloeHealthTransformBridge",
        ]),
    ],
    swiftLanguageModes: [.v6]
)
