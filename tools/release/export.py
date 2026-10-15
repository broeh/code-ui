"""Export a reviewed source snapshot without private history or local material."""
import argparse
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FILES = [".gitignore", ".node-version", "rust-toolchain.toml", "README.md", "LICENSE", "CHANGELOG.md", "CONTRIBUTING.md", "SECURITY.md", "THIRD_PARTY_NOTICES.md", "package.json", "package-lock.json", "index.html", "tsconfig.json", "tsconfig.node.json", "vite.config.ts"]
DIRECTORIES = ["src", "public", "src-tauri", "tools", ".github"]
DOCS = ["setup.md", "agents.md", "architecture.md", "roadmap.md", "release-plan.md", "screenshots", "releases"]


def export(destination):
    destination = destination.resolve()
    if destination == ROOT or ROOT in destination.parents:
        raise SystemExit("Export to a separate, empty directory outside the source checkout.")
    if destination.exists() and any(destination.iterdir()):
        raise SystemExit("Destination is not empty; existing files were preserved.")
    destination.mkdir(parents=True, exist_ok=True)
    ignored = shutil.ignore_patterns(".git", "node_modules", "target", "gen", "__pycache__", ".cache", "*.pyc", "*.log", "*.trace.jsonl", "*.db", "*.db-*", "*.sqlite*")
    for name in FILES:
        shutil.copy2(ROOT / name, destination / name)
    for name in DIRECTORIES:
        if (ROOT / name).exists():
            shutil.copytree(ROOT / name, destination / name, ignore=ignored)
    (destination / "docs").mkdir(exist_ok=True)
    for name in DOCS:
        source, target = ROOT / "docs" / name, destination / "docs" / name
        if source.is_dir():
            shutil.copytree(source, target, ignore=ignored)
        else:
            shutil.copy2(source, target)
    (destination / "development_log.md").write_text("# Development log\n\nThe public repository starts with the initial alpha source snapshot.\n\n" + (ROOT / "CHANGELOG.md").read_text().removeprefix("# Changelog\n\n"))
    print(f"Exported public snapshot to {destination}; Git history and private project material were excluded.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    export(parser.parse_args().destination)
