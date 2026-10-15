"""Inspect desktop packages and assemble checksummed release assets."""
import hashlib
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / ".cache/release"


def assemble():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    if any(OUTPUT.iterdir()):
        raise SystemExit("Release output must be empty to avoid stale assets.")
    bundles = ROOT / "src-tauri/target/release/bundle"
    debs = list(bundles.glob("deb/*.deb"))
    images = list(bundles.glob("appimage/*.AppImage"))
    if len(debs) != 1 or len(images) != 1:
        raise SystemExit("Expected one Debian package and one AppImage.")
    listing = subprocess.check_output(["dpkg-deb", "--contents", str(debs[0])], text=True)
    for required in ("/usr/bin/code-ui", "/LICENSE", "/THIRD_PARTY_LICENSES.txt", ".desktop"):
        if required not in listing:
            raise SystemExit(f"Debian package is missing {required}")
    subprocess.run(["dpkg-deb", "--info", str(debs[0])], check=True)
    for item in [*debs, *images, ROOT / ".cache/THIRD_PARTY_LICENSES.txt", ROOT / "LICENSE"]:
        shutil.copy2(item, OUTPUT / item.name)
    checksums = [f"{hashlib.sha256(item.read_bytes()).hexdigest()}  {item.name}" for item in sorted(OUTPUT.iterdir())]
    (OUTPUT / "SHA256SUMS").write_text("\n".join(checksums) + "\n")
    print("Verified Debian contents; assembled AppImage, Debian package, license notices and SHA256SUMS.")


if __name__ == "__main__":
    assemble()
