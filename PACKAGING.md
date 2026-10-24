# Packaging and Homebrew

Repository: https://github.com/thisishugow/sshto

Source archives support Homebrew and local Rust builds. Binary archives let macOS, Linux, and Windows users install without Rust. Only an OpenSSH client is required at runtime.

## Source archive contents

`tools/package.py` requires Python 3.11 or newer, reads the version from `Cargo.toml`, and packages only these files under `sshto-<version>/`:

- `Cargo.toml`, `Cargo.lock`
- `src/cli.rs`, `src/config.rs`, `src/hosts.rs`, `src/main.rs`, `src/menu.rs`, `src/ssh.rs`, `src/ui.rs`
- `README.md`, `PACKAGING.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `LICENSE`
- `assets/sshto.svg`, `examples/config`

All entries are regular files with mode 0644. Archive entries omit local ownership and original file timestamps. The builder does not recursively collect the working directory or read personal SSH/group configuration. Generated archives and Formulae belong in ignored output directories, not source control.

Extract the archive and run `cargo build --release --locked` from its root to build with the Rust toolchain. Cargo downloads the locked dependencies; the archive does not vendor them.

## Build a source release

Run the checks, then build into a new output directory:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 tests/verify.py
python3 tools/package.py dist/0.2.0 \
  --homepage https://github.com/thisishugow/sshto \
  --url https://github.com/thisishugow/sshto/releases/download/v0.2.0/sshto-0.2.0.tar.gz
```

The builder writes `sshto-<version>.tar.gz`, its `SHA256SUMS`, and `Formula/sshto.rb`. The URL is the intended publication location; this command does not upload or verify it. Inspect the archive and generated Formula before publication.

For a preview without publication URLs:

```sh
python3 tools/package.py dist/preview
```

This creates `Formula/sshto.rb.in` with unresolved URL placeholders. It is a template, not an installable Formula. Output directories must not already exist.

## Binary release workflow

`.github/workflows/release.yml` builds these targets:

| Target | Runner | Archive |
| --- | --- | --- |
| `x86_64-unknown-linux-musl` | `ubuntu-latest` with `musl-tools` | `.tar.gz` |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` with `musl-tools` | `.tar.gz` |
| `x86_64-apple-darwin` | `macos-latest` | `.tar.gz` |
| `aarch64-apple-darwin` | `macos-latest` | `.tar.gz` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `.zip` |
| `aarch64-pc-windows-msvc` | `windows-latest` | `.zip` |

Each `sshto-<version>-<target>` archive contains `sshto` (Unix) or `sshto.exe` (Windows), `README.md`, `LICENSE`, and `examples/config`. The workflow also builds the source archive and creates one `SHA256SUMS` covering all seven archives.

Pull requests changing the release workflow build and collect artifacts without publishing. Pushing a `v*` tag builds the archives, verifies that the tag equals `v` plus the Cargo version, and creates a GitHub release with all archives and checksums using `gh` and `GITHUB_TOKEN`. Only the publishing job has `contents: write`; build and assembly jobs have read access. Checkouts do not persist credentials.

## Cut a release

1. Update the version in `Cargo.toml`, refresh `Cargo.lock` with `cargo check`, and record user-facing changes in `CHANGELOG.md`. Binary and test version strings read the Cargo version.
2. Run the local checks above and verify that an extracted source archive builds with `cargo build --release --locked`.
3. Commit the release changes through the project's normal review process. Tag the reviewed release commit and push the tag using the existing remote:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

4. Confirm the release workflow succeeds and inspect the published archives and `SHA256SUMS`.
5. Update the Homebrew tap using the checksum of the exact published source archive. Do not substitute the checksum from a separately generated archive: gzip metadata can change it.

## Publish a tap

The public tap is [thisishugow/homebrew-tap](https://github.com/thisishugow/homebrew-tap). Put the generated Formula at `Formula/sshto.rb` in that repository and set its SHA256 to the published source archive's entry in `SHA256SUMS`.

The Formula supports macOS and Linuxbrew. It declares `depends_on "rust" => :build` and installs with `cargo install` using Homebrew's `std_cargo_args`. It also installs documentation, the MIT license, and a generic configuration example. It never writes to users' SSH or group configuration. The example is installed at `$(brew --prefix sshto)/share/sshto/config`.

After updating the Formula, install and test it:

```sh
brew install thisishugow/tap/sshto
brew test thisishugow/tap/sshto
```

Upgrade with `brew upgrade thisishugow/tap/sshto`. Private repositories require appropriate Git and download authentication.

## Local Formula verification

For local testing, the builder accepts a `file:///` archive URL. Install the generated Formula through a temporary local tap and run `brew test`. Never publish a Formula that refers to a local file URL.

Official references: [Creating and maintaining a tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap) and [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook).
