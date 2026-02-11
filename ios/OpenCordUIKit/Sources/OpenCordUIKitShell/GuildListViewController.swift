import Foundation
import OpenCordClientCore

#if canImport(UIKit)
import UIKit

@MainActor
final class GuildListViewController: UITableViewController {
    private let apiClient: OpenCordAPIClient
    private let onGuildSelected: (GuildDTO) -> Void
    private let onLogoutRequested: () -> Void
    private var guilds: [GuildDTO] = []

    init(
        apiClient: OpenCordAPIClient,
        onGuildSelected: @escaping (GuildDTO) -> Void,
        onLogoutRequested: @escaping () -> Void
    ) {
        self.apiClient = apiClient
        self.onGuildSelected = onGuildSelected
        self.onLogoutRequested = onLogoutRequested
        super.init(style: .plain)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        title = "Guilds"
        tableView.register(UITableViewCell.self, forCellReuseIdentifier: "GuildCell")
        navigationItem.rightBarButtonItem = UIBarButtonItem(
            title: "Logout",
            style: .plain,
            target: self,
            action: #selector(logoutTapped)
        )

        Task {
            await loadGuilds()
        }
    }

    private func loadGuilds() async {
        do {
            guilds = try await apiClient.listGuilds()
            tableView.reloadData()
        } catch {
            guilds = []
            tableView.reloadData()
        }
    }

    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        guilds.count
    }

    override func tableView(
        _ tableView: UITableView,
        cellForRowAt indexPath: IndexPath
    ) -> UITableViewCell {
        let cell = tableView.dequeueReusableCell(withIdentifier: "GuildCell", for: indexPath)
        cell.textLabel?.text = guilds[indexPath.row].name
        cell.accessoryType = .disclosureIndicator
        return cell
    }

    override func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
        tableView.deselectRow(at: indexPath, animated: true)
        onGuildSelected(guilds[indexPath.row])
    }

    @objc private func logoutTapped() {
        onLogoutRequested()
    }
}

#endif
