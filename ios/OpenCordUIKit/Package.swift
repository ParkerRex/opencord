// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "OpenCordUIKit",
    platforms: [
        .iOS(.v16),
        .macOS(.v13)
    ],
    products: [
        .library(name: "OpenCordClientCore", targets: ["OpenCordClientCore"]),
        .library(name: "OpenCordNetworking", targets: ["OpenCordNetworking"]),
        .library(name: "OpenCordUIKitShell", targets: ["OpenCordUIKitShell"])
    ],
    targets: [
        .target(
            name: "OpenCordClientCore"
        ),
        .target(
            name: "OpenCordNetworking",
            dependencies: ["OpenCordClientCore"]
        ),
        .target(
            name: "OpenCordUIKitShell",
            dependencies: ["OpenCordClientCore", "OpenCordNetworking"]
        ),
        .testTarget(
            name: "OpenCordClientCoreTests",
            dependencies: ["OpenCordClientCore"]
        ),
        .testTarget(
            name: "OpenCordNetworkingTests",
            dependencies: ["OpenCordNetworking", "OpenCordClientCore"]
        )
    ]
)
