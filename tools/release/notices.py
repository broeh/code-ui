"""Generate dependency license notices for desktop bundles from locked dependencies."""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / ".cache/THIRD_PARTY_LICENSES.txt"


def license_files(folder, explicit=None):
    files = set()
    if explicit and (folder / explicit).is_file():
        files.add((folder / explicit).resolve())
    for item in folder.iterdir():
        if item.is_file() and item.name.lower().startswith(("license", "licence", "copying", "notice", "copyright")):
            files.add(item)
    return sorted(files)


def generate():
    sections = ["CODE_UI third-party licenses\n\nOriginal application source: MIT. Provider CLIs are installed separately.\n"]
    missing = []
    count = 0
    supplemental = json.loads((ROOT / "tools/release/licenses/index.json").read_text())

    def add(name, version, license_name, source, files):
        nonlocal count
        count += 1
        sections.append(f"\n{'=' * 72}\n{name} {version}\nDeclared license: {license_name or 'See upstream source'}\nSource: {source}\n")
        if not files:
            missing.append(name)
            sections.append("No license file was included in the installed package. Consult the declared license and upstream source distribution.\n")
        for item in files:
            sections.append(f"\n--- {item.name} ---\n{item.read_text(errors='replace')}\n")

    lock = json.loads((ROOT / "package-lock.json").read_text())
    for location, package in sorted(lock["packages"].items()):
        if not location or package.get("dev"):
            continue
        folder = ROOT / location
        if not folder.is_dir():
            continue
        metadata = json.loads((folder / "package.json").read_text())
        name, version = metadata["name"], metadata["version"]
        files = license_files(folder)
        if not files:
            files = [ROOT / "tools/release/licenses" / name for name in supplemental.get(f"{name}@{version}", [])]
        add(name, version, metadata.get("license"), f"https://www.npmjs.com/package/{name}/v/{version}", files)

    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", "x86_64-unknown-linux-gnu", "--manifest-path", str(ROOT / "src-tauri/Cargo.toml")], cwd=ROOT))
    included = {node["id"] for node in metadata["resolve"]["nodes"]}
    for package in sorted(metadata["packages"], key=lambda package: (package["name"], package["version"])):
        if package["id"] in metadata["workspace_members"] or package["id"] not in included:
            continue
        folder = Path(package["manifest_path"]).parent
        files = license_files(folder, package.get("license_file"))
        if not files:
            files = [ROOT / "tools/release/licenses" / name for name in supplemental.get(f"{package['name']}@{package['version']}", [])]
        add(package["name"], package["version"], package.get("license"), f"https://crates.io/crates/{package['name']}/{package['version']}", files)

    add("LobeHub agent icons", "", "MIT", "https://github.com/lobehub/lobe-icons", [ROOT / "src/assets/agents/LICENSE"])
    OUTPUT.parent.mkdir(exist_ok=True)
    OUTPUT.write_text("\n".join(sections))
    print(f"Generated bundled notices for {count} dependencies.")
    if missing:
        raise SystemExit("Missing license texts: " + ", ".join(missing))


if __name__ == "__main__":
    generate()
