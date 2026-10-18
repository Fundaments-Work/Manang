# Security policy

Manang edits people's unpublished research, stores API keys for AI providers, runs third-party plugins and can relay live-editing sessions. We take reports seriously.

## Reporting a vulnerability

**Do not open a public issue.** Use GitHub's private reporting: the repository's **Security** tab, then **Report a vulnerability**. Include the version (Help > About), your platform, what you did, what happened, and what you expected. A proof of concept helps; please do not test against other people's data or services.

We aim to acknowledge a report within 5 working days, tell you whether we can reproduce it, and keep you updated until it is fixed. We will credit you in the release notes unless you prefer not to be named.

## Supported versions

During the private beta only the latest beta release is supported. After 1.0, the latest minor release gets fixes.

## What is in scope

* The desktop app: file handling, crash-recovery and safety copies, project registry, Trash.
* The plugin platform: escaping the WebAssembly sandbox, using a capability the manifest does not declare, bypassing the consent dialog.
* The AI agent: keys leaving the OS keychain, document text treated as instructions (prompt injection that triggers a tool without approval), data sent that the preview did not show.
* Live editing and the relay: anything that lets the relay or a non-invited person read or change a session, key handling in invite links.
* Updates and installers: tampering, downgrade, signature checks (when shipped).

## Out of scope

* Issues that need a malicious plugin the user installed and approved with the permissions it asked for.
* A compromised operating system, keychain or account.
* Denial of service through a document you chose to compile (Typst and TeX can loop); we treat unbounded resource use in our own code as a normal bug.
* Findings from automated scanners without a demonstrated impact.

## Our commitments

Keys live only in the operating system keychain. Telemetry does not exist. Plugins run in a sandbox with declared capabilities. Dependencies are checked for licences and known advisories in CI.
