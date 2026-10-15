"""Check a public source snapshot. Reports locations, never matching secret values."""
import argparse
import json
import re
from pathlib import Path

RULES = {
    "credential pattern": re.compile(r"(?:sk-(?:ant-|proj-)?[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[A-Z0-9]{16}|-----BEGIN (?:[A-Z]+ )?PRIVATE KEY-----)"),
    "personal home directory": re.compile(r"/(?:home|Users)/(?!user/|test/|someone/|example/)[A-Za-z][A-Za-z0-9._-]*/"),
}
SKIP = {".git", "node_modules", "target", "gen", ".cache", "__pycache__"}
FORBIDDEN = {"inspiration", "temp", "PLAN.md", "GRAPHITE_PLAN.md", ".env"}
TEXT = {".rs", ".ts", ".tsx", ".js", ".cjs", ".py", ".json", ".jsonl", ".md", ".yml", ".yaml", ".sh", ".txt", ".html", ".css", ".toml", ".lock"}


def audit(root):
    failures = []
    for item in sorted(root.rglob("*")):
        relative = item.relative_to(root)
        if any(part in SKIP for part in relative.parts):
            continue
        if any(part in FORBIDDEN for part in relative.parts):
            failures.append(f"{relative}: private/local material")
            continue
        if item.is_symlink():
            failures.append(f"{relative}: symlink requires manual review")
        if not item.is_file() or item.suffix not in TEXT:
            continue
        for number, line in enumerate(item.read_text(errors="replace").splitlines(), 1):
            for description, pattern in RULES.items():
                if pattern.search(line):
                    failures.append(f"{relative}:{number}: {description}")
    package = json.loads((root / "package.json").read_text())
    tauri = json.loads((root / "src-tauri/tauri.conf.json").read_text())
    cargo = re.search(r'^version\s*=\s*"([^"]+)"', (root / "src-tauri/Cargo.toml").read_text(), re.M).group(1)
    if len({package["version"], tauri["version"], cargo}) != 1:
        failures.append("Package, Cargo and Tauri versions differ")
    if package.get("private") is not True:
        failures.append("package.json must prevent accidental npm publication")
    if failures:
        raise SystemExit("Snapshot audit failed:\n" + "\n".join(failures))
    print("Public snapshot audit passed: local-material, credential-pattern, home-path and release-metadata checks.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, nargs="?", default=Path(__file__).resolve().parents[2])
    audit(parser.parse_args().root.resolve())
