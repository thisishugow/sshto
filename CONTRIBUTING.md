# Contributing

Keep sshto small, portable across macOS, Linux, and Windows, and safe to use with existing SSH configuration.

## Local checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 tests/verify.py
```

Use the stable Rust toolchain. Python 3.11 or newer is needed only for the Unix PTY suite and release builder; run `tests/verify.py` on macOS or Linux. Tests must use fictional hostnames, documentation IP ranges, isolated home directories, and an SSH stub; never connect to a real host.

For interface changes, verify keyboard and mouse behavior, narrow terminals, resize handling, and terminal restoration. Preserve direct SSH argument forwarding and process only key-press events on Windows. Restore the terminal on every exit path, including errors and panics. Keep user configuration as data; do not introduce `source` or `eval` for group rules.

## Privacy

Do not commit personal SSH configuration, group rules, keys, passwords, tokens, real inventories, or screenshots containing them. Use `examples/config` for generic examples. Keep local files outside the repository; `.gitignore` is an extra safeguard, not a secret scanner.

## Changes

Describe the behavior being changed, how it was tested, and any relevant limitations. Update the README and changelog when behavior changes. See [PACKAGING.md](PACKAGING.md) for release packaging.

Contributions are provided under the project's [MIT license](LICENSE).
