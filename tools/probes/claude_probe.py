"""Drive claude -p stream-json as an SDK-style host and print protocol events."""
import json
import subprocess
import sys
import threading

cwd, mode, prompt_tool = sys.argv[1], sys.argv[2], sys.argv[3] == "1"
cmd = [
    "claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json",
    "--verbose", "--model", "haiku", "--permission-mode", mode, "--no-session-persistence",
]
if prompt_tool:
    cmd += ["--permission-prompt-tool", "stdio"]
p = subprocess.Popen(cmd, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                     stderr=subprocess.PIPE, text=True, bufsize=1)


def send(obj):
    print(">>", json.dumps(obj)[:300], flush=True)
    p.stdin.write(json.dumps(obj) + "\n")
    p.stdin.flush()


threading.Timer(80, p.kill).start()
send({"type": "control_request", "request_id": "init-1", "request": {"subtype": "initialize"}})
send({"type": "user", "message": {"role": "user", "content":
      "Use the AskUserQuestion tool to ask me whether I prefer red or blue (two options). "
      "After I answer, run the shell command `echo picked` with Bash, then reply with one short sentence."}})

for line in p.stdout:
    try:
        o = json.loads(line)
    except ValueError:
        print("RAW", line[:200])
        continue
    t, st = o.get("type"), o.get("subtype", "")
    if t == "system" and st in ("init", "commands_changed", "hook_started", "hook_response", "thinking_tokens"):
        print("<<", t, st)
        continue
    print("<<", json.dumps(o)[:900], flush=True)
    if t == "control_request":
        req = o["request"]
        if req.get("subtype") == "can_use_tool":
            inp = req["input"]
            if req.get("tool_name") == "AskUserQuestion":
                qs = inp["questions"]
                if isinstance(qs, str):
                    qs = json.loads(qs)
                inp = dict(inp, answers={qs[0]["question"]: "Blue"})
            send({"type": "control_response", "response": {
                "subtype": "success", "request_id": o["request_id"],
                "response": {"behavior": "allow", "updatedInput": inp}}})
    if t == "result":
        p.stdin.close()
p.wait()
print("exit", p.returncode, "stderr:", p.stderr.read()[:500])
