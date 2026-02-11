import Foundation
import OpenCordClientCore
import OpenCordNetworking

#if canImport(UIKit)
import UIKit

@MainActor
public final class OpenCordAppCoordinator {
    private let navigationController: UINavigationController
    private let apiClient: OpenCordAPIClient
    private let realtimeClient: OpenCordRealtimeClient
    private let sessionStore: OpenCordAuthSessionStore
    private let oauthCallbackURL = URL(string: "opencord://auth/callback")!

    public init(
        navigationController: UINavigationController,
        apiClient: OpenCordAPIClient,
        realtimeClient: OpenCordRealtimeClient,
        sessionStore: OpenCordAuthSessionStore
    ) {
        self.navigationController = navigationController
        self.apiClient = apiClient
        self.realtimeClient = realtimeClient
        self.sessionStore = sessionStore
    }

    public func start() {
        if sessionStore.currentSessionID == nil {
            showAuth()
        } else {
            showGuilds()
        }
    }

    @discardableResult
    public func handleIncomingURL(_ url: URL) -> Bool {
        guard
            url.scheme?.lowercased() == oauthCallbackURL.scheme?.lowercased(),
            url.host == oauthCallbackURL.host
        else {
            return false
        }

        guard
            let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
            let sessionID = components.queryItems?.first(where: { $0.name == "session_id" })?.value,
            !sessionID.isEmpty
        else {
            return false
        }

        sessionStore.setSessionID(sessionID)
        showGuilds()
        return true
    }

    private func showAuth() {
        let authViewController = AuthViewController(
            onSessionSubmitted: { [weak self] sessionID in
                self?.sessionStore.setSessionID(sessionID)
                self?.showGuilds()
            },
            onOAuthRequested: { [weak self] in
                guard let self else { return }
                Task { [weak self] in
                    guard let self else { return }
                    await self.beginOAuthFlow()
                }
            }
        )

        navigationController.setViewControllers([authViewController], animated: false)
    }

    private func beginOAuthFlow() async {
        guard let authViewController = currentAuthViewController() else {
            return
        }

        do {
            let authStart = try await apiClient.startDiscordAuth(clientRedirectURI: oauthCallbackURL)
            guard let authorizeURL = URL(string: authStart.authorizeURL) else {
                authViewController.setStatus("Invalid authorize URL returned by service.")
                return
            }

            let opened = await UIApplication.shared.open(authorizeURL)
            if !opened {
                authViewController.setStatus("Unable to open Discord OAuth URL.")
            } else {
                authViewController.setStatus("Complete sign-in in browser to return to app.")
            }
        } catch {
            authViewController.setStatus(error.localizedDescription)
        }
    }

    private func currentAuthViewController() -> AuthViewController? {
        navigationController.viewControllers.first as? AuthViewController
    }

    private func showGuilds() {
        let guildsViewController = GuildListViewController(
            apiClient: apiClient,
            onGuildSelected: { [weak self] guild in
                self?.showTimeline(for: guild)
            },
            onLogoutRequested: { [weak self] in
                Task { [weak self] in
                    guard let self else { return }
                    await self.logout()
                }
            }
        )

        navigationController.setViewControllers([guildsViewController], animated: false)
    }

    private func showTimeline(for guild: GuildDTO) {
        let timeline = ChannelTimelineViewController(
            guild: guild,
            apiClient: apiClient,
            realtimeClient: realtimeClient
        )
        navigationController.pushViewController(timeline, animated: true)
    }

    private func logout() async {
        do {
            try await apiClient.logout()
        } catch {
            // Keep logout resilient even if backend revoke fails.
        }

        await realtimeClient.disconnect()
        sessionStore.setSessionID(nil)
        showAuth()
    }
}

#endif
