"""Drive a preview build through real Tauri IPC using isolated data and a fake CLI.

Run under dbus-run-session and xvfb-run on Linux with tauri-driver installed.
"""
import base64
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / ".cache/native-checks"
ELEMENT = "element-6066-11e4-a52e-4f735466cecf"
BASE = "http://127.0.0.1:4444"


def request(method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=45) as response:
            value = json.load(response).get("value")
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error
    if isinstance(value, dict) and value.get("error"):
        raise RuntimeError(value)
    return value


def wait(action, timeout=30):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            result = action()
            if result:
                return result
        except (RuntimeError, OSError) as error:
            last = error
        time.sleep(0.2)
    raise AssertionError(f"Timed out: {last}")


class Window:
    def __init__(self, binary):
        self.id = request("POST", "/session", {"capabilities": {"alwaysMatch": {"tauri:options": {"application": str(binary)}}}})["sessionId"]

    def command(self, method, path, body=None):
        return request(method, f"/session/{self.id}" + path, body)

    def find(self, selector, using="css selector"):
        return self.command("POST", "/element", {"using": using, "value": selector})[ELEMENT]

    def click(self, selector, using="css selector"):
        element = wait(lambda: self.find(selector, using))
        self.command("POST", f"/element/{element}/click", {})

    def button(self, text):
        self.click(f"//button[normalize-space(.)='{text}']", "xpath")

    def type(self, selector, text):
        element = wait(lambda: self.find(selector))
        self.command("POST", f"/element/{element}/value", {"text": text})

    def text(self, selector):
        return self.command("GET", f"/element/{self.find(selector)}/text")

    def screenshot(self, name):
        (OUTPUT / name).write_bytes(base64.b64decode(self.command("GET", "/screenshot")))

    def close(self):
        self.command("DELETE", "")


def run(binary):
    OUTPUT.mkdir(parents=True, exist_ok=True)
    driver_binary = shutil.which("tauri-driver")
    if not driver_binary:
        raise SystemExit("Install tauri-driver 2.1.0 first.")
    with tempfile.TemporaryDirectory(prefix="code-ui-native-") as temporary:
        home = Path(temporary)
        env = os.environ.copy()
        for key, value in {"HOME": home, "XDG_CONFIG_HOME": home / "config", "XDG_DATA_HOME": home / "data", "XDG_CACHE_HOME": home / "cache", "XDG_RUNTIME_DIR": home / "runtime", "CLAUDE_CONFIG_DIR": home / "claude", "CODEX_HOME": home / "codex", "CODE_UI_FAKE_LOG": home / "protocol.jsonl", "SHELL": "/bin/sh", "CODE_UI_PROFILE": "preview", "GDK_BACKEND": "x11", "WEBKIT_DISABLE_DMABUF_RENDERER": "1"}.items():
            env[key] = str(value)
        (home / "runtime").mkdir(mode=0o700)
        config = home / "config/code-ui-preview"
        config.mkdir(parents=True)
        fake = home / "fake-codex"
        shutil.copyfile(ROOT / "tools/testing/fake_codex.py", fake)
        fake.chmod(0o700)
        settings = {"schemaVersion": 1, "projectsRoot": str(home / "projects with spaces"), "binaries": {agent: str(fake if agent == "codex" else home / "missing-cli") for agent in ("claude", "codex", "agy", "muse")}, "enabledAgents": ["codex"], "defaultAgent": "codex", "defaultPermission": "ask", "historyAgents": [], "trustedProjects": [], "setupComplete": False, "notifications": False}
        (config / "settings.json").write_text(json.dumps(settings))
        window = None
        with (OUTPUT / "driver.log").open("w") as log:
            driver = subprocess.Popen([driver_binary], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                wait(lambda: request("GET", "/status"))
                window = Window(binary)
                wait(lambda: "Set up CODE_UI" in window.text("dialog"))
                window.click(".settings-check input[type=checkbox]")
                window.button("Next")
                assert "fake-codex 0.0.0" in window.text("dialog")
                window.button("Next")
                window.button("Open workspace")
                window.click('button[aria-label="New project"]')
                window.type("dialog input", "Native project")
                window.button("Create project")
                window.type(".composer textarea", "Synthetic native request")
                window.click('button[aria-label="Send message"]')
                wait(lambda: "Trust this project?" in window.text("dialog"))
                assert not (home / "protocol.jsonl").exists(), "CLI ran before project trust"
                window.screenshot("trust.png")
                window.button("Trust folder and continue")
                wait(lambda: "Synthetic native reply" in window.text(".message.assistant"))
                window.screenshot("conversation.png")
                rows = [json.loads(line) for line in (home / "protocol.jsonl").read_text().splitlines()]
                turn = next(row for row in rows if row.get("method") == "turn/start")
                assert turn["params"]["approvalPolicy"] == "on-request"
                assert turn["params"]["sandboxPolicy"]["type"] == "workspaceWrite"
                saved = json.loads((config / "settings.json").read_text())
                assert saved["setupComplete"] and saved["trustedProjects"]
                window.close()
                window = None
                time.sleep(1)
                window = Window(binary)
                wait(lambda: "Synthetic native reply" in window.text(".message.assistant"))
                window.type(".composer textarea", "Resume synthetic native thread")
                window.click('button[aria-label="Send message"]')
                wait(lambda: len(window.command("POST", "/elements", {"using": "css selector", "value": ".message.assistant"})) == 2)
                rows = [json.loads(line) for line in (home / "protocol.jsonl").read_text().splitlines()]
                assert any(row.get("method") == "thread/resume" for row in rows)
                assert not (home / "config/code-ui").exists(), "Preview touched stable configuration"
                assert not (home / "data/code-ui").exists(), "Preview touched stable data"
                print("Native smoke passed: setup, folder with spaces, trust gate, real IPC, fake Codex turn, Ask policy, saved conversation, resume and isolated preview data.")
            except Exception:
                if window:
                    try:
                        window.screenshot("failure.png")
                    except Exception:
                        pass
                raise
            finally:
                if window:
                    try:
                        window.close()
                    except Exception:
                        pass
                os.killpg(driver.pid, signal.SIGTERM)
                driver.wait(timeout=10)


if __name__ == "__main__":
    run(Path(sys.argv[1]).resolve())
