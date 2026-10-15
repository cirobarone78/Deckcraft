#!/usr/bin/env python3
"""Install the pinned, OFL-licensed STIX math face into an external craft-fonts checkout.

Run once before building; Cargo itself never downloads fonts. Existing unrelated manifest
entries and fonts are preserved. No font binaries are stored in the DeckCraft repository.
"""
import argparse
import hashlib
from pathlib import Path
from urllib.request import urlopen

REV = "744a22a4dd626cd14d75728aef34fc8ad7c85db0"
SOURCE = f"https://raw.githubusercontent.com/stipub/stixfonts/{REV}/"
FILES = {
    "STIXTwoMath-Regular.otf": ("fonts/static_otf/STIXTwoMath-Regular.otf", "3a5f3f26f40d5698b3c62dd085d48d6663696a3f80825aab8b553d5097518e8c"),
    "OFL.txt": ("OFL.txt", "0c8825913b60d858aacdb33c4ca6660a7d64b0d6464702efbb19313f5765861a"),
}


def install(root):
    if not (root / "fonts/manifest.txt").is_file():
        raise SystemExit(f"Not a craft-fonts checkout: {root}")
    target = root / "fonts/stix-two-math"
    verified = {}
    for name, (remote, sha) in FILES.items():
        path = target / name
        data = path.read_bytes() if path.exists() else b""
        if hashlib.sha256(data).hexdigest() != sha:
            with urlopen(SOURCE + remote, timeout=30) as response:
                data = response.read(2 * 1024 * 1024)
        if hashlib.sha256(data).hexdigest() != sha:
            raise SystemExit(f"Checksum mismatch for {name}; no files were installed")
        verified[name] = data
    target.mkdir(parents=True, exist_ok=True)
    for name, data in verified.items():
        (target / name).write_bytes(data)
    manifest = root / "fonts/latin-manifest.txt"
    previous = manifest.read_text() if manifest.exists() else ""
    lines = [line for line in previous.splitlines() if not line.startswith("STIX Two Math | ")]
    lines.append(" | ".join([
        "STIX Two Math", "Regular", "fonts/stix-two-math/STIXTwoMath-Regular.otf", "Latn,Grek,Math", "OFL-1.1",
        "fonts/stix-two-math/OFL.txt", FILES["STIXTwoMath-Regular.otf"][1],
        f"https://github.com/stipub/stixfonts/tree/{REV}",
    ]))
    manifest.write_text("\n".join(lines) + "\n")
    print(f"STIX Two Math v2.13b171 installed in {target}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, nargs="?", default=Path(__file__).resolve().parents[2] / "craft-fonts")
    install(parser.parse_args().directory.resolve())
