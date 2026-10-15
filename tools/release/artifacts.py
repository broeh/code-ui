"""Inspect desktop packages and assemble checksummed release assets."""
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from urllib.parse import quote

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
    subprocess.run(["dpkg-deb", "--info", str(debs[0])], check=True)
    expected = (ROOT / ".cache/THIRD_PARTY_LICENSES.txt").read_bytes()
    with tempfile.TemporaryDirectory(prefix="code-ui-packages-") as temporary:
        extracted = Path(temporary)
        subprocess.run(["dpkg-deb", "--extract", str(debs[0]), str(extracted / "deb")], check=True)
        deb = extracted / "deb"
        print("Debian executables:", ", ".join(item.name for item in (deb / "usr/bin").iterdir()), flush=True)
        if not (deb / "usr/bin/code-ui").is_file() or not list(deb.rglob("*.desktop")) or not list(deb.rglob("LICENSE")):
            raise SystemExit("Debian package is missing the executable, desktop entry or application license.")
        subprocess.run([str(images[0]), "--appimage-extract"], cwd=extracted, stdout=subprocess.DEVNULL, check=True)
        for package in (extracted / "deb", extracted / "squashfs-root"):
            notices = list(package.rglob("THIRD_PARTY_LICENSES.txt"))
            if len(notices) != 1 or notices[0].read_bytes() != expected:
                raise SystemExit(f"Missing or stale bundled license notices in {package.name}")
        appimage = extracted / "squashfs-root"
        copyrights = sorted((appimage / "usr/share/doc").rglob("copyright"))
        if not copyrights:
            raise SystemExit("AppImage is missing system-library copyright notices.")
        provided = {item.stem for item in (ROOT / "tools/release/licenses/common").glob("*.txt")}
        aliases = {"GPL": "GPL-3", "LGPL": "LGPL-3", "GPL-2.0": "GPL-2", "GPL-3.0": "GPL-3"}
        for item in copyrights:
            for name in re.findall(r"/usr/share/common-licenses/([A-Za-z0-9.-]+)", item.read_text(errors="replace")):
                name = name.rstrip(".")
                if aliases.get(name, name) not in provided:
                    raise SystemExit(f"Missing standard system license text: {name}")
        names = sorted({item.parent.name for item in copyrights})
        fields = "${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n"
        rows = subprocess.check_output(["dpkg-query", "--show", f"--showformat={fields}", *names], text=True)
        sources = []
        for row in rows.splitlines():
            binary, version, source, source_version = row.split("\t")
            sources.append({"binaryPackage": binary, "binaryVersion": version, "sourcePackage": source, "sourceVersion": source_version, "sourceUrl": f"https://launchpad.net/ubuntu/+source/{quote(source, safe='')}/{quote(source_version, safe='')}"})
        (OUTPUT / "SYSTEM_LIBRARY_SOURCES.json").write_text(json.dumps({"distribution": "Ubuntu 22.04", "packages": sources}, indent=2) + "\n")
        print(f"Verified standard license texts for {len(copyrights)} system notices; recorded {len(sources)} exact source package versions.")
    for item in [*debs, *images, ROOT / ".cache/THIRD_PARTY_LICENSES.txt", ROOT / "LICENSE"]:
        shutil.copy2(item, OUTPUT / item.name)
    checksums = [f"{hashlib.sha256(item.read_bytes()).hexdigest()}  {item.name}" for item in sorted(OUTPUT.iterdir())]
    (OUTPUT / "SHA256SUMS").write_text("\n".join(checksums) + "\n")
    print("Verified Debian contents and extracted license resources in both packages; assembled checksummed assets.")


if __name__ == "__main__":
    assemble()
