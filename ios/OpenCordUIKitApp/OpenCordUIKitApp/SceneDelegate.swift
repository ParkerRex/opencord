import OpenCordClientCore
import OpenCordNetworking
import OpenCordUIKitShell
import UIKit

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    private let sessionStore = UserDefaultsAuthSessionStore()
    private var coordinator: OpenCordAppCoordinator?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else {
            return
        }

        let navigationController = UINavigationController()
        let baseURL = ServiceEndpointStore.shared.currentBaseURL
        let config = OpenCordServiceConfig(baseURL: baseURL)
        let apiClient = HTTPAPIClient(config: config, sessionStore: sessionStore)
        let realtimeClient = ServiceRealtimeClient(config: config, sessionStore: sessionStore)

        let coordinator = OpenCordAppCoordinator(
            navigationController: navigationController,
            apiClient: apiClient,
            realtimeClient: realtimeClient,
            sessionStore: sessionStore
        )
        self.coordinator = coordinator

        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = navigationController
        window.makeKeyAndVisible()
        self.window = window

        coordinator.start()

        if let incomingURL = connectionOptions.urlContexts.first?.url {
            _ = coordinator.handleIncomingURL(incomingURL)
        }
    }

    func scene(_ scene: UIScene, openURLContexts urlContexts: Set<UIOpenURLContext>) {
        guard let url = urlContexts.first?.url else {
            return
        }
        _ = coordinator?.handleIncomingURL(url)
    }
}

private final class ServiceEndpointStore {
    static let shared = ServiceEndpointStore()

    private let key = "opencord.service.base_url"
    private let defaults = UserDefaults.standard
    private let defaultURL = URL(string: "http://127.0.0.1:8080")!

    var currentBaseURL: URL {
        guard
            let raw = defaults.string(forKey: key),
            let url = URL(string: raw),
            url.scheme != nil,
            url.host != nil
        else {
            return defaultURL
        }
        return url
    }
}
