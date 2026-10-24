#!/usr/bin/env python3
"""Build a release from an explicit file allowlist and optional publication URLs."""
import argparse
import hashlib
from pathlib import Path
import re
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FILES = ("Cargo.toml", "Cargo.lock", "src/cli.rs", "src/config.rs", "src/hosts.rs",
         "src/main.rs", "src/menu.rs", "src/pattern.rs", "src/ssh.rs", "src/ui.rs", "README.md", "PACKAGING.md", "CHANGELOG.md", "CONTRIBUTING.md", "LICENSE", "assets/sshto.svg", "examples/config")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="Output directory; must not already exist")
    parser.add_argument("--url", help="Full HTTPS archive URL; file:/// is supported for local testing")
    parser.add_argument("--homepage", help="Full project HTTPS URL")
    args = parser.parse_args()
    if bool(args.url) != bool(args.homepage):
        parser.error("--url and --homepage must be provided together")
    for value in (args.url, args.homepage):
        # Reject interpolation, escapes and injected Ruby code in generated URL strings.
        if value and not re.fullmatch(r"(?:https://|file:///)[A-Za-z0-9._~:/%+?=&-]+", value):
            parser.error("Use HTTPS (or local file:///); quotes, spaces and interpolation characters are not allowed")
    with (ROOT / "Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["package"]["version"]
    args.output.mkdir(parents=True, exist_ok=False)
    archive = args.output / f"sshto-{version}.tar.gz"
    with tarfile.open(archive, "w:gz", format=tarfile.PAX_FORMAT) as tar:
        for name in FILES:
            path = ROOT / name
            if path.is_symlink() or not path.is_file():
                raise ValueError(f"Only regular files are allowed: {name}")
            info = tarfile.TarInfo(f"sshto-{version}/{name}")
            info.size = path.stat().st_size
            info.mode = 0o644
            # Do not include local ownership, working directory or file timestamps.
            with path.open("rb") as source:
                tar.addfile(info, source)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (args.output / "SHA256SUMS").write_text(f"{digest}  {archive.name}\n")
    formula = (ROOT / "packaging/sshto.rb.in").read_text()
    values = {"VERSION": version, "SHA256": digest}
    if args.url:
        values.update(URL=args.url, HOMEPAGE=args.homepage)
    for key, value in values.items():
        formula = formula.replace(f"@@{key}@@", value)
    (args.output / "Formula").mkdir()
    suffix = "rb" if args.url else "rb.in"
    (args.output / "Formula" / f"sshto.{suffix}").write_text(formula)
    print(archive)
    print(f"SHA256 {digest}")
    if not args.url:
        print("No publication URLs supplied: generated a Formula template, not an installable Formula.")


if __name__ == "__main__":
    main()
