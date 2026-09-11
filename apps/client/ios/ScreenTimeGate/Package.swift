// swift-tools-version: 6.0

import PackageDescription

let package = Package(
  name: "FloeScreenTimeGate",
  platforms: [
    .iOS(.v16),
    .macOS(.v13),
  ],
  products: [
    .library(name: "FloeScreenTimeGate", targets: ["FloeScreenTimeGate"]),
  ],
  targets: [
    .target(name: "FloeScreenTimeGate"),
    .testTarget(
      name: "FloeScreenTimeGateTests",
      dependencies: ["FloeScreenTimeGate"],
      resources: [.copy("Fixtures")]
    ),
  ]
)
