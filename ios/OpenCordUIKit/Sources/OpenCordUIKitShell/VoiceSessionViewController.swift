import Foundation
import OpenCordClientCore

#if canImport(UIKit)
import UIKit

@MainActor
public final class VoiceSessionViewController: UIViewController {
    private let apiClient: OpenCordAPIClient
    private let guildID: Snowflake
    private let channelID: Snowflake

    private var voiceSession: VoiceSessionDTO?
    private let statusLabel = UILabel()
    private let toggleSpeakingButton = UIButton(type: .system)

    public init(apiClient: OpenCordAPIClient, guildID: Snowflake, channelID: Snowflake) {
        self.apiClient = apiClient
        self.guildID = guildID
        self.channelID = channelID
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    public override func viewDidLoad() {
        super.viewDidLoad()
        title = "Voice"
        view.backgroundColor = .systemBackground

        statusLabel.textAlignment = .center
        statusLabel.text = "Disconnected"

        toggleSpeakingButton.setTitle("Toggle Speaking", for: .normal)
        toggleSpeakingButton.addTarget(self, action: #selector(toggleSpeakingTapped), for: .touchUpInside)

        let stack = UIStackView(arrangedSubviews: [statusLabel, toggleSpeakingButton])
        stack.axis = .vertical
        stack.spacing = 16
        stack.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -24),
            stack.centerYAnchor.constraint(equalTo: view.centerYAnchor)
        ])

        Task {
            await joinVoice()
        }
    }

    deinit {
        guard let voiceSession else { return }

        Task {
            try? await apiClient.deleteVoiceSession(voiceSessionID: voiceSession.id)
        }
    }

    private func joinVoice() async {
        do {
            voiceSession = try await apiClient.createVoiceSession(
                request: CreateVoiceSessionRequestDTO(guildID: guildID, channelID: channelID)
            )
            statusLabel.text = "Connected"
        } catch {
            statusLabel.text = "Failed to connect"
        }
    }

    @objc private func toggleSpeakingTapped() {
        guard let voiceSession else { return }

        Task {
            do {
                let updated = try await apiClient.updateVoiceSpeaking(
                    voiceSessionID: voiceSession.id,
                    request: UpdateVoiceSpeakingRequestDTO(speaking: !voiceSession.speaking)
                )
                self.voiceSession = updated
                statusLabel.text = updated.speaking ? "Speaking" : "Connected"
            } catch {
                statusLabel.text = "Voice update failed"
            }
        }
    }
}

#endif
