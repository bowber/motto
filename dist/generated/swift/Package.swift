// swift-tools-version:5.9
// MOTTO GENERATED - Protocol Version: 0xA1

import PackageDescription

let package = Package(
    name: "MottoSDK",
    platforms: [
        .iOS(.v15),
        .macOS(.v12),
        .watchOS(.v8),
        .tvOS(.v15)
    ],
    products: [
        .library(
            name: "MottoSDK",
            targets: ["MottoSDK"]
        ),
    ],
    dependencies: [],
    targets: [
        .target(
            name: "MottoSDK",
            dependencies: [],
            swiftSettings: [
                .enableExperimentalFeature("StrictConcurrency")
            ]
        ),
        .testTarget(
            name: "MottoSDKTests",
            dependencies: ["MottoSDK"]
        ),
    ]
)
