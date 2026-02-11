import Foundation
import OpenCordClientCore

#if canImport(UIKit)
import UIKit

@MainActor
final class ChannelTimelineViewController: UIViewController, UITableViewDataSource {
    private let guild: GuildDTO
    private let apiClient: OpenCordAPIClient
    private let realtimeClient: OpenCordRealtimeClient

    private var channels: [ChannelDTO] = []
    private var selectedChannel: ChannelDTO?
    private var messages: [MessageDTO] = []
    private var realtimeTask: Task<Void, Never>?

    private let tableView = UITableView(frame: .zero, style: .plain)
    private let composerField = UITextField()
    private let sendButton = UIButton(type: .system)

    init(guild: GuildDTO, apiClient: OpenCordAPIClient, realtimeClient: OpenCordRealtimeClient) {
        self.guild = guild
        self.apiClient = apiClient
        self.realtimeClient = realtimeClient
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        title = guild.name
        view.backgroundColor = .systemBackground

        tableView.dataSource = self
        tableView.register(UITableViewCell.self, forCellReuseIdentifier: "MessageCell")
        tableView.translatesAutoresizingMaskIntoConstraints = false

        composerField.placeholder = "Send a message"
        composerField.borderStyle = .roundedRect

        sendButton.setTitle("Send", for: .normal)
        sendButton.addTarget(self, action: #selector(sendTapped), for: .touchUpInside)

        let composerStack = UIStackView(arrangedSubviews: [composerField, sendButton])
        composerStack.axis = .horizontal
        composerStack.spacing = 8
        composerStack.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(tableView)
        view.addSubview(composerStack)

        NSLayoutConstraint.activate([
            tableView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            tableView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            tableView.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            tableView.bottomAnchor.constraint(equalTo: composerStack.topAnchor, constant: -8),

            composerStack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
            composerStack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -12),
            composerStack.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -12)
        ])

        Task {
            await loadChannelsAndMessages()
            await startRealtime()
        }
    }

    deinit {
        realtimeTask?.cancel()
        Task {
            await realtimeClient.disconnect()
        }
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        guard isMovingFromParent else { return }

        realtimeTask?.cancel()
        realtimeTask = nil

        Task {
            await realtimeClient.disconnect()
        }
    }

    private func loadChannelsAndMessages() async {
        do {
            channels = try await apiClient.listChannels(guildID: guild.id)
            selectedChannel = channels.first

            if let selectedChannel {
                messages = try await apiClient.listMessages(channelID: selectedChannel.id, limit: 50)
            }

            tableView.reloadData()
        } catch {
            channels = []
            messages = []
            tableView.reloadData()
        }
    }

    private func startRealtime() async {
        do {
            try await realtimeClient.connect()
            realtimeTask?.cancel()
            realtimeTask = Task { [weak self] in
                guard let self else { return }

                for await event in realtimeClient.events {
                    guard !Task.isCancelled else { break }

                    switch event {
                    case let .messageCreated(_, message):
                        if message.channelID == selectedChannel?.id {
                            messages.append(message)
                            tableView.reloadData()
                        }
                    case let .messageUpdated(_, message):
                        guard let index = messages.firstIndex(where: { $0.id == message.id }) else { continue }
                        messages[index] = message
                        tableView.reloadData()
                    case let .messageDeleted(_, channelID, messageID):
                        guard channelID == selectedChannel?.id else { continue }
                        messages.removeAll { $0.id == messageID }
                        tableView.reloadData()
                    default:
                        continue
                    }
                }
            }
        } catch {
            // Keep timeline functional even if realtime fails.
        }
    }

    @objc private func sendTapped() {
        let content = composerField.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard !content.isEmpty, let channelID = selectedChannel?.id else { return }

        Task {
            do {
                let sent = try await apiClient.createMessage(
                    channelID: channelID,
                    request: CreateMessageRequestDTO(content: content)
                )
                messages.append(sent)
                composerField.text = nil
                tableView.reloadData()
            } catch {
                // Keep UI simple: ignore send failures in shell.
            }
        }
    }

    func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        messages.count
    }

    func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
        let cell = tableView.dequeueReusableCell(withIdentifier: "MessageCell", for: indexPath)
        let message = messages[indexPath.row]
        cell.textLabel?.numberOfLines = 0
        cell.textLabel?.text = "\(message.author.username): \(message.content)"
        return cell
    }
}

#endif
