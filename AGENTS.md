# AGENTS.md

`sshto` is a single Bash script (`sshto`) that shows `~/.ssh/config` Host aliases in a grouped TUI menu and connects to the selected one.

## Commands

```sh
/bin/bash -n sshto                                          # Syntax check
python3 tests/verify.py                                     # Full suite (same as CI)
python3 tests/verify.py MenuTests.test_cli_and_passthrough  # Single test
python3 tools/package.py dist/preview                       # Preview build; output dir must not exist
```

The suite runs against every installed Bash among `/bin/bash`, `/opt/homebrew/bin/bash`, and `/usr/local/bin/bash`. Release steps are in `PACKAGING.md`.

## Constraints

- Stay compatible with macOS **Bash 3.2**: no associative arrays, `${var,,}`, `mapfile`, or other Bash 4+ features. State lives in parallel indexed arrays.
- Runtime dependencies are OpenSSH, Perl, and standard utilities only. Python is for tests and packaging.
- Parse group configuration as data. Never `source` or `eval` it.
- Any invocation other than exactly `--help` or `--version` must `exec ssh "$@"` unchanged.
- Leave the terminal clean: `cleanup` restores `stty` and disables mouse reporting, and `connect_selected` runs it before `exec ssh` so leftover mouse events never reach SSH.
- The main loop's `read -t 1` timeout lets Bash 5 handle `WINCH`. Bash 3.2 returns 1 for both timeout and EOF, so `$SECONDS` is used to distinguish them; don't simplify this away.

## Tests

- `tests/verify.py` drives the script through real PTYs with an isolated `HOME` and an `ssh` stub on `PATH`. Never connect to a real host.
- Use only fictional hostnames and documentation IPs (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24). Never commit personal SSH configuration or real inventories.
- For interface changes, cover keyboard, mouse, narrow terminals, resize, and terminal restoration.

## Releases

- `tools/package.py` ships an explicit allowlist. To add a release file, update the allowlist, `PACKAGING.md`, and the expected list in the packaging test.
- To bump the version, update the `sshto --version` output, the hardcoded versions in `tests/verify.py`, and `CHANGELOG.md`.
- Update the README and `CHANGELOG.md` when behavior changes.
