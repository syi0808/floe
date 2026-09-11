// swift-tools-version: 6.0

import PackageDescription

let package = Package(
  name: "FloeFeasibilityProvider",
  platforms: [
    .iOS(.v16),
    .macOS(.v13),
  ],
  products: [
    .library(name: "FloeFeasibilityProvider", targets: ["FloeFeasibilityProvider"]),
  ],
  targets: [
    .target(name: "FloeFeasibilityProvider"),
    .testTarget(
      name: "FloeFeasibilityProviderTests",
      dependencies: ["FloeFeasibilityProvider"]
    ),
  ]
)
