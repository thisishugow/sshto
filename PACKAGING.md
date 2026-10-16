# Packaging and Homebrew

Repository: https://github.com/thisishugow/sshto

The project includes a release builder and a Homebrew Formula template. These do not publish a release or create a tap automatically.

## Archive contents

`tools/package.py` packages only these files:

- `sshto`
- `README.md`
- `PACKAGING.md`
- `CONTRIBUTING.md`
- `CHANGELOG.md`
- `LICENSE`
- `assets/sshto.svg`
- `examples/config`

It does not recursively collect the working directory or read personal SSH/group configuration. Archive entries omit local ownership and original file timestamps. Generated archives and Formulae belong in ignored output directories, not source control.

## Build a release

Run the tests, then build into a new output directory:

```sh
python3 tests/verify.py
python3 tools/package.py dist/0.1.0 \
  --homepage https://github.com/thisishugow/sshto \
  --url https://github.com/thisishugow/sshto/releases/download/v0.1.0/sshto-0.1.0.tar.gz
```

The builder writes the tarball, `SHA256SUMS`, and `Formula/sshto.rb`. The URL is the intended publication location; the builder does not upload or verify it.

Inspect the archive and generated Formula before publication. Upload the exact tarball to the specified GitHub release URL. Repacking it changes the SHA256 and invalidates the Formula.

For a preview without publication URLs:

```sh
python3 tools/package.py dist/preview
```

This creates `Formula/sshto.rb.in` with unresolved URL placeholders. It is a template, not an installable Formula. Output directories must not already exist.

## Publish a tap

The public tap is [thisishugow/homebrew-tap](https://github.com/thisishugow/homebrew-tap). Put the generated Formula at `Formula/sshto.rb` in that repository.

After uploading the release archive and pushing the updated Formula, users can install with:

```sh
brew install thisishugow/tap/sshto
```

Test the published Formula with `brew test thisishugow/tap/sshto`. Upgrade it using `brew upgrade thisishugow/tap/sshto` after updating the Formula's version, URL, and checksum for subsequent releases.

The Formula installs the executable, documentation, MIT license, and a generic configuration example. It never writes to users' SSH or group configuration. The example is installed at `$(brew --prefix sshto)/share/sshto/config`.

Private repositories require appropriate Git and download authentication; the public-tap instructions alone are insufficient for them.

## Local Formula verification

For local testing, the builder accepts a `file:///` archive URL. Install the generated Formula through a temporary local tap and run `brew test`. Never publish a Formula that refers to a local file URL.

## Version updates

Update the version returned by `sshto --version`, adjust its test expectation, and record user-facing changes in `CHANGELOG.md`. Run the suite, build a new archive, and update the tap only when the intended release asset is available.

Official references: [Creating and maintaining a tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap) and [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook).
