# Changelog

User-facing changes are recorded here. Versions follow semantic versioning.

## [0.2.0] - 2026-10-24

### Changed

- Rewrote sshto as a Rust binary and removed the shell implementation.
- Report group-configuration errors before checking whether the menu has a terminal.

### Added

- Linux and Windows support, including PowerShell and cmd.
- Prebuilt release binaries for macOS, Linux, and Windows on x86_64 and ARM64, with SHA256 checksums.
- Rust source archives and a Homebrew build dependency on Rust.

## [0.1.0] - 2026-10-16

### Added

- macOS SSH terminal menu compatible with Bash 3.2 and Bash 5.
- An All view with group headings and sorted host aliases.
- User-defined groups, ordered alias/hostname glob rules, and exclusions.
- XDG configuration paths and a `SSHTO_CONFIG` override.
- Keyboard shortcuts, clickable tabs, wheel navigation, and double-click connections.
- Responsive layouts and terminal restoration when quitting or connecting.
- Direct argument forwarding to SSH, plus help and version commands.
- Isolated PTY tests and an allowlisted release builder with SHA256 checksums.
- A public Homebrew tap, release archives with SHA256 checksums, and packaging documentation.
- English documentation and interface, an SVG logo, and the MIT license.

[Unreleased]: https://github.com/thisishugow/sshto/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/thisishugow/sshto/releases/tag/v0.2.0
[0.1.0]: https://github.com/thisishugow/sshto/releases/tag/v0.1.0
