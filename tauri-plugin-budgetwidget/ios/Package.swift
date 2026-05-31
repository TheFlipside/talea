// swift-tools-version:5.3
import PackageDescription

let package = Package(
  name: "tauri-plugin-budgetwidget",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "tauri-plugin-budgetwidget",
      type: .static,
      targets: ["tauri-plugin-budgetwidget"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-budgetwidget",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
