<p align="center">
  <img src="assets/sshto.svg" alt="sshto — SSH terminal menu" width="480">
</p>

<p align="center">A lightweight SSH terminal menu for macOS with configurable groups and double-click connections.</p>

Pick a host from your own SSH configuration and connect with Enter or a double-click. No server inventory to maintain separately, no passwords stored by sshto, and no additional runtime packages to install.

## Features

- **Grouped navigation:** an `All` view with group headings, plus individual group tabs.
- **Your own rules:** classify aliases or resolved hostnames using ordered glob patterns.
- **Keyboard and mouse:** arrows, numeric shortcuts, clickable tabs, scrolling, and double-click connections.
- **Small-terminal support:** scrolling lists and a compact header when tabs do not fit.
- **Normal SSH behavior:** forward command-line arguments directly to OpenSSH.
- **Local configuration:** no bundled hosts, keys, credentials, or telemetry.

## Requirements

macOS with its built-in Bash 3.2, OpenSSH, Perl, and standard command-line utilities. Bash 5 is also tested. Mouse support requires a terminal that supports and enables xterm SGR mouse reporting; keyboard navigation remains available otherwise.

## Installation

### From source

Download this repository, then run from its directory:

```sh
mkdir -p "$HOME/.local/bin"
install -m 755 sshto "$HOME/.local/bin/sshto"
```

Ensure `~/.local/bin` is on your `PATH`. For zsh, add this to `~/.zshrc` if needed and open a new terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

### Homebrew

Homebrew packaging is included and has been tested locally. A public tap and downloadable release have **not been published yet**, so there is no public `brew install` command to run today. See [PACKAGING.md](PACKAGING.md) for the release procedure.

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
| `q`, Esc, Ctrl+C | Quit; standalone Esc can take up to one second |

All shows nonempty groups in their configured order, with aliases sorted within each group. Narrow windows show only the current tab. Exiting or connecting restores terminal settings and disables mouse reporting.

## Configuration

Hosts come from `~/.ssh/config`. Group rules are optional: without them, every host appears under **Other**.

The default group configuration path is:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/sshto/config
```

For first-time setup, copy the generic example from the source directory:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/sshto"
cp -n examples/config "${XDG_CONFIG_HOME:-$HOME/.config}/sshto/config"
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
- Patterns use Bash glob syntax (`*`, `?`, `[abc]`), not regular expressions. ASCII matching is case-insensitive.
- `exclude|glob` hides matching aliases from the menu without affecting direct connections.

Use `|` as the field separator, with no surrounding whitespace or quotes. Empty lines and full-line comments beginning with `#` are ignored. Configuration is parsed as data: shell commands and variable expansion are not evaluated. Literal `|` and control characters are not supported in fields.

To use another file:

```sh
SSHTO_CONFIG=/path/to/config sshto
```

An explicitly selected missing file is an error. Installation and upgrades never create or overwrite your personal group rules or SSH configuration.

### Host discovery limits

Only explicit `Host` aliases in `~/.ssh/config` are listed. Indentation, case-insensitive `Host` keywords, and multiple aliases on one line are supported; duplicate aliases are collapsed. Patterns containing `*`, `?`, or `!` are skipped. `Include` files are not expanded.

## Privacy

sshto keeps no credential database and sends no telemetry. Your hosts, usernames, keys, and authentication remain in your own SSH configuration. `ssh -G` and connections use your local OpenSSH; existing `Match exec`, `ProxyCommand`, and other OpenSSH configuration retain their normal behavior.

The release builder uses an explicit file allowlist and excludes personal configuration. Review source and archive contents before publishing; an allowlist cannot detect secrets manually added to an allowed file.

## Development

```sh
python3 tests/verify.py
```

The tests use isolated home directories, real pseudo-terminals, and an SSH stub. They check configuration, navigation, mouse handling, terminal restoration, argument forwarding, and release contents without connecting to remote hosts. Available system and Homebrew Bash installations are tested.

See [CONTRIBUTING.md](CONTRIBUTING.md), [CHANGELOG.md](CHANGELOG.md), and [PACKAGING.md](PACKAGING.md).

## License

[MIT](LICENSE). Copyright (c) 2026 thisishugow.
