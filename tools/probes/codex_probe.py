"""Drive codex app-server over stdio JSON-RPC and print protocol traffic."""
import json
import subprocess
import sys
import threading

cwd = sys.argv[1]
p = subprocess.Popen(["codex", "app-server"], cwd=cwd, stdin=subprocess.PIPE,
                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
threading.Timer(90, p.kill).start()
next_id = [0]


def send(method, params=None, id_=None, result=None):
    if result is not None:
        msg = {"id": id_, "result": result}
    elif id_ is None and method in ("initialized",):
        msg = {"method": method}
    else:
        next_id[0] += 1
        msg = {"id": next_id[0], "method": method, "params": params or {}}
    print(">>", json.dumps(msg)[:300], flush=True)
    p.stdin.write(json.dumps(msg) + "\n")
    p.stdin.flush()


send("initialize", {"clientInfo": {"name": "code_ui_spike", "version": "0.0.1"}})
thread_id = None
for line in p.stdout:
    o = json.loads(line)
    m = o.get("method")
    if m in ("item/agentMessage/delta", "item/reasoning/summaryTextDelta", "item/commandExecution/outputDelta",
             "account/rateLimits/updated", "thread/tokenUsage/updated", "mcpServer/startupStatus/updated"):
        print("<<", m, "(delta/noise)")
        continue
    print("<<", json.dumps(o)[:700], flush=True)
    if o.get("id") == 1:
        send("initialized")
        send("thread/start", {"cwd": cwd, "approvalPolicy": "on-request", "sandbox": "read-only",
                              "ephemeral": True})
    elif o.get("id") == 2 and "result" in o:
        thread_id = o["result"]["thread"]["id"]
        send("turn/start", {"threadId": thread_id, "input": [{"type": "text", "text":
             "Run `touch codex_probe.txt` in the shell, then reply with one short sentence."}]})
    elif o.get("id") == 2 and "error" in o:
        send("thread/start", {"cwd": cwd, "approvalPolicy": "on-request", "sandbox": "read-only",
                              "ephemeral": True})
    elif m and "requestApproval" in m:
        send(None, id_=o["id"], result={"decision": "decline"})
    elif m == "turn/completed":
        break
p.kill()
