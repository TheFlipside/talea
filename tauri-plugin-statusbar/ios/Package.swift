// swift-tools-version:5.3
import PackageDescription

let package = Package(
  name: "tauri-plugin-statusbar",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "tauri-plugin-statusbar",
      type: .static,
      targets: ["tauri-plugin-statusbar"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-statusbar",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
