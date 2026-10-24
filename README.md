<p align="center">
  <img src="assets/sshto.svg" alt="sshto — SSH terminal menu" width="480">
</p>

<p align="center">A lightweight SSH terminal menu for macOS, Linux, and Windows with configurable groups and double-click connections.</p>

Pick a host from your own SSH configuration and connect with Enter or a double-click. No server inventory to maintain separately, no passwords stored by sshto, and no additional runtime packages to install.

## Features

- **Grouped navigation:** an `All` view with group headings, plus individual group tabs.
- **Your own rules:** classify aliases or resolved hostnames using ordered glob patterns.
- **Keyboard and mouse:** arrows, numeric shortcuts, clickable tabs, scrolling, and double-click connections.
- **Small-terminal support:** scrolling lists and a compact header when tabs do not fit.
- **Normal SSH behavior:** forward command-line arguments directly to OpenSSH.
- **Local configuration:** no bundled hosts, keys, credentials, or telemetry.

## Requirements

An OpenSSH client on `PATH`, built into macOS, most Linux distributions, and Windows 10/11. Enable the optional OpenSSH Client feature on Windows if `ssh` is unavailable. Mouse support depends on the terminal; keyboard navigation remains available otherwise.

## Installation

### Homebrew

```sh
brew install thisishugow/tap/sshto
```

Update with `brew upgrade thisishugow/tap/sshto`. The [public tap](https://github.com/thisishugow/homebrew-tap) installs the executable, documentation, license, and a generic configuration example without changing your personal configuration.

### Release binaries

Download an archive for your operating system and processor from [Releases](https://github.com/thisishugow/sshto/releases). Prebuilt binaries do not require Rust.

| Platform | Archive target |
| --- | --- |
| Linux, Intel/AMD 64-bit | `x86_64-unknown-linux-musl` |
| Linux, ARM64 | `aarch64-unknown-linux-musl` |
| macOS, Intel | `x86_64-apple-darwin` |
| macOS, Apple silicon | `aarch64-apple-darwin` |
| Windows, Intel/AMD 64-bit | `x86_64-pc-windows-msvc` |
| Windows, ARM64 | `aarch64-pc-windows-msvc` |

Compare the archive's SHA256 with the release's `SHA256SUMS` using `shasum -a 256` on macOS, `sha256sum` on Linux, or `Get-FileHash -Algorithm SHA256` in PowerShell. Extract the `.tar.gz` (macOS/Linux) or `.zip` (Windows). Place `sshto` or `sshto.exe` in a directory on your `PATH`, such as `~/.local/bin` on Unix or a user-owned tools directory on Windows.

### From source

With the Rust toolchain installed:

```sh
cargo install --git https://github.com/thisishugow/sshto
```

Ensure Cargo's binary directory (`~/.cargo/bin` on Unix, `%USERPROFILE%\.cargo\bin` on Windows) is on your `PATH`.

See [PACKAGING.md](PACKAGING.md) for the release procedure.

## Usage

```sh
sshto                    # Open the menu
sshto my-server          # Connect directly to an SSH alias
sshto -p 2222 my-server   # Forward arguments to ssh
sshto --help
sshto --version
```

Except for the exact `--help` and `--version` invocations, arguments are passed unchanged to `ssh`.

| Control | Action |
| --- | --- |
| `0` | Open All, grouped by category |
| `1`–`9` | Jump to the corresponding group |
| Left / Right, Tab / Shift+Tab | Cycle through groups, including groups beyond 9 |
| Up / Down | Select a host, skipping group headings |
| Enter | Connect to the selected host |
| Click a host | Select it |
| Double-click a host | Connect; clicks must be within 450 ms |
| Click a visible tab | Switch groups |
| Mouse wheel over the list | Move selection and scroll |
| `q`, Esc, Ctrl+C | Quit |

All shows nonempty groups in their configured order, with aliases sorted within each group. Narrow windows show only the current tab. Exiting or connecting restores terminal settings and disables mouse reporting.

## Configuration

Hosts come from `~/.ssh/config`. Group rules are optional: without them, every host appears under **Other**.

On macOS and Linux, the default group configuration path is:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/sshto/config
```

For first-time setup, copy the generic example from the source directory:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/sshto"
cp -n examples/config "${XDG_CONFIG_HOME:-$HOME/.config}/sshto/config"
```

For a Homebrew installation, copy the installed example instead:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/sshto"
cp -n "$(brew --prefix sshto)/share/sshto/config" \
  "${XDG_CONFIG_HOME:-$HOME/.config}/sshto/config"
```

Edit that file to match your own aliases. The example uses fictional names and documentation-only IP addresses:

```text
group|Tailscale
group|Work
group|Personal

match|Work|hostname|192.0.2.*
match|Tailscale|either|*.ts.net
match|Tailscale|alias|*-ts
match|Work|alias|work-*
match|Personal|alias|home-*

exclude|github.*
exclude|gitlab.*
```

- `group|name` declares a group and its display order. `All` and `Other` are reserved; Other is added automatically at the end.
- `match|group|field|glob` assigns a host to the **first matching rule**. Declare the group before referencing it.
- `field` is `alias`, `hostname`, or `either`. Hostnames are resolved by `ssh -G`.
- Patterns use Bash glob syntax without extglob: `*`, `?`, bracket expressions and ranges (`[abc]`, `[a-z]`), `!` or `^` negation, a leading literal `]`, POSIX classes such as `[[:alpha:]]` (`alpha`, `digit`, `alnum`, `upper`, `lower`, `space`, `blank`, `punct`, `print`, `graph`, `cntrl`, `xdigit`), and backslash escapes. An unclosed `[` is a literal character. ASCII letters in both the pattern and the compared text are lowercased before matching, and classes use ASCII semantics, so `[[:upper:]]` never matches a letter.
- `exclude|glob` hides matching aliases from the menu without affecting direct connections.

Use `|` as the field separator, with no surrounding whitespace or quotes. Empty lines and full-line comments beginning with `#` are ignored. Configuration is parsed as data: shell commands and variable expansion are not evaluated. Literal `|` and control characters are not supported in fields.

To use another file:

```sh
SSHTO_CONFIG=/path/to/config sshto
```

An explicitly selected missing file is an error. Installation and upgrades never create or overwrite your personal group rules or SSH configuration.

### Host discovery limits

Only explicit `Host` aliases in the main SSH configuration file are listed. Indentation, case-insensitive `Host` keywords, and multiple aliases on one line are supported; duplicate aliases are collapsed. Patterns containing `*`, `?`, or `!` are skipped. `Include` files are not expanded.

## Windows

sshto works from PowerShell and cmd in Windows Terminal and the classic console. Install a Windows release binary and ensure both `sshto.exe` and the OpenSSH client (`ssh.exe`) are on `PATH`.

- SSH aliases: `%USERPROFILE%\.ssh\config`.
- Group rules: `%APPDATA%\sshto\config`.
- `SSHTO_CONFIG` overrides the group-rule path.

Copy `examples/config` from the release archive to the group-rule path if you want custom groups; create the parent directory first. Installation does not create or replace either configuration file. In PowerShell, set an override with `$env:SSHTO_CONFIG = 'C:\path\to\config'`; in cmd, use `set "SSHTO_CONFIG=C:\path\to\config"`.

## Privacy

sshto keeps no credential database and sends no telemetry. Your hosts, usernames, keys, and authentication remain in your own SSH configuration. `ssh -G` and connections use your local OpenSSH; existing `Match exec`, `ProxyCommand`, and other OpenSSH configuration retain their normal behavior.

The release builder uses an explicit file allowlist and excludes personal configuration. Review source and archive contents before publishing; an allowlist cannot detect secrets manually added to an allowed file.

## Development

Install the Rust toolchain and Python 3.11 or newer for the Unix PTY suite and release builder.

```sh
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 tests/verify.py
```

Rust tests cover parsing, menu state, CLI behavior, and SSH argument forwarding. On macOS and Linux, `tests/verify.py` builds the binary and checks real pseudo-terminal interactions and release contents. On Windows, `tests/windows_menu.rs` drives the menu in the classic console and through ConPTY, launched from cmd and PowerShell. Tests use isolated home directories and SSH stubs without connecting to remote hosts. CI runs Rust checks on macOS, Linux, and Windows.

See [CONTRIBUTING.md](CONTRIBUTING.md), [CHANGELOG.md](CHANGELOG.md), and [PACKAGING.md](PACKAGING.md).

## License

[MIT](LICENSE). Copyright (c) 2026 thisishugow.
