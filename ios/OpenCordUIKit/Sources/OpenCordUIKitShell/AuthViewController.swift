import Foundation

#if canImport(UIKit)
import UIKit

@MainActor
final class AuthViewController: UIViewController {
    private let sessionField = UITextField()
    private let continueButton = UIButton(type: .system)
    private let oauthButton = UIButton(type: .system)
    private let statusLabel = UILabel()
    private let onSessionSubmitted: (String) -> Void
    private let onOAuthRequested: () -> Void

    init(
        onSessionSubmitted: @escaping (String) -> Void,
        onOAuthRequested: @escaping () -> Void
    ) {
        self.onSessionSubmitted = onSessionSubmitted
        self.onOAuthRequested = onOAuthRequested
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        title = "OpenCord"

        sessionField.placeholder = "Session ID"
        sessionField.borderStyle = .roundedRect
        sessionField.autocapitalizationType = .none

        continueButton.setTitle("Continue", for: .normal)
        continueButton.addTarget(self, action: #selector(continueTapped), for: .touchUpInside)

        oauthButton.setTitle("Sign In with Discord", for: .normal)
        oauthButton.addTarget(self, action: #selector(oauthTapped), for: .touchUpInside)

        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 0
        statusLabel.font = .preferredFont(forTextStyle: .footnote)
        statusLabel.textColor = .secondaryLabel
        statusLabel.text = "Enter an existing session ID or start OAuth sign-in."

        let stack = UIStackView(arrangedSubviews: [sessionField, continueButton, oauthButton, statusLabel])
        stack.axis = .vertical
        stack.spacing = 16
        stack.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -24),
            stack.centerYAnchor.constraint(equalTo: view.centerYAnchor)
        ])
    }

    @objc private func continueTapped() {
        let sessionID = sessionField.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard !sessionID.isEmpty else { return }
        statusLabel.text = nil
        onSessionSubmitted(sessionID)
    }

    @objc private func oauthTapped() {
        statusLabel.text = "Launching Discord sign-in..."
        onOAuthRequested()
    }

    func setStatus(_ status: String?) {
        statusLabel.text = status
    }
}

#endif
