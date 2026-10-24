# AGENTS.md

`sshto` is a Rust binary that groups SSH Host aliases in a terminal menu on macOS, Linux, and Windows.

## Commands

```sh
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 tests/verify.py               # Unix PTY and packaging tests; Python 3.11+
python3 tools/package.py dist/preview # Output directory must not exist
```

## Modules

- `main`: load configuration, validate the terminal, and dispatch CLI or menu actions.
- `cli`: handle exact help/version invocations and preserve SSH arguments.
- `config`: parse group rules as data and resolve platform-specific paths.
- `hosts`: extract aliases, resolve SSH metadata, exclude, and classify hosts.
- `menu`: manage pure navigation, layout, selection, scrolling, and mouse state.
- `ui`: render frames, handle terminal events, and restore terminal state.
- `ssh`: execute SSH on Unix; spawn and wait with the child exit code on Windows.

## Constraints

- Runtime requires an OpenSSH client; use existing Rust dependencies before adding more.
- Parse group configuration as data; never evaluate shell commands or source files.
- Only exactly `--help` or `--version` is handled locally; all other nonempty arguments pass unchanged to SSH. No arguments opens the menu.
- Restore terminal state on every exit path, including connect, quit, errors, signals, and panic. Never exit while the terminal is modified.
- On Windows, process only key-press events; ignore release and repeat events.
- Preserve PTY frame delimiters and terminal restoration behavior used by `tests/verify.py`.

## Tests and releases

- Tests use isolated homes and SSH stubs; never contact real hosts.
- Use fictional hostnames and documentation IPs only; never commit personal configuration, credentials, or real inventories.
- Interface changes must cover keyboard, mouse, narrow terminals, resize, and restoration.
- `Cargo.toml` is the only version source; use `env!("CARGO_PKG_VERSION")` in Rust and read the manifest in Python. Cargo updates the lockfile.
- Source releases use an explicit allowlist; keep `tools/package.py`, its packaging test, and `PACKAGING.md` aligned.
- Update README and CHANGELOG when behavior changes. Release instructions are in `PACKAGING.md`.
