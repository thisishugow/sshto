# Contributing

Keep sshto small, compatible with macOS Bash 3.2, and safe to use with existing SSH configuration.

## Local checks

```sh
/bin/bash -n sshto
python3 tests/verify.py
```

Python 3 is a development dependency only. The test suite also uses a Homebrew Bash installation when available. Tests must use fictional hostnames, documentation IP ranges, isolated home directories, and an SSH stub; never connect to a real host.

For interface changes, verify keyboard and mouse behavior, narrow terminals, resize handling, and terminal restoration. Preserve direct SSH argument forwarding. Keep user configuration as data; do not introduce `source` or `eval` for group rules.

## Privacy

Do not commit personal SSH configuration, group rules, keys, passwords, tokens, real inventories, or screenshots containing them. Use `examples/config` for generic examples. Keep local files outside the repository; `.gitignore` is an extra safeguard, not a secret scanner.

## Changes

Describe the behavior being changed, how it was tested, and any relevant limitations. Update the README and changelog when behavior changes. See [PACKAGING.md](PACKAGING.md) for release packaging.

Contributions are provided under the project's [MIT license](LICENSE).
