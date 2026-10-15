"""Reduce CODE_UI scratch-workspace traces to shareable parser fixtures.

Review the resulting fixtures before committing. Do not use private project traces.
"""
import argparse
import json
import re
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("source", type=Path)
parser.add_argument("destination", type=Path)
parser.add_argument("--scratch", default="/tmp/code-ui-acceptance")
parser.add_argument("--agent", choices=["claude","codex","agy","muse"], default="claude")
args = parser.parse_args()


def scrub(value):
    if isinstance(value, str):
        return value.replace(args.scratch, "/scratch").replace(str(Path.home()), "/home/user")
    if isinstance(value, list):
        return [scrub(item) for item in value]
    if isinstance(value, dict):
        return {key: scrub(item) for key, item in value.items() if key != "signature"}
    return value


records = []
for line in args.source.read_text().splitlines():
    record = json.loads(line)
    msg = record["msg"]
    if args.agent == "agy":
        if msg.get("event") == "init":
            msg = dict(msg, init={key: msg["init"][key] for key in ["model", "cwd", "permission_mode"] if key in msg["init"]})
        records.append({"dir":record["dir"], "msg":scrub(msg)})
        continue
    if args.agent in ["codex", "muse"]:
        method=msg.get("method", "")
        if method.startswith(("account/", "thread/tokenUsage/", "mcpServer/startupStatus/", "remoteControl/")): continue
        if method == "initialize" and "result" in msg: continue
        if "result" in msg and isinstance(msg["result"],dict):
            result=dict(msg["result"])
            for key in ["codexHome","museHome","userAgent","auth","account","token"]:result.pop(key,None)
            msg=dict(msg,result=result)
        records.append({"dir":record["dir"],"msg":scrub(msg)})
        continue
    kind = msg.get("type")
    if kind == "system":
        if msg.get("subtype") != "init":
            continue
        msg = {key: msg[key] for key in ["type", "subtype", "session_id", "model", "permissionMode", "cwd"] if key in msg}
    elif kind == "control_response" and msg.get("response", {}).get("request_id") == "init-1":
        msg = {"type": "control_response", "response": {"subtype": msg["response"]["subtype"], "request_id": "init-1", "response": {"commands": []}}}
    elif kind in ["assistant", "user"]:
        msg = {key: msg[key] for key in ["type", "message", "parent_tool_use_id", "session_id"] if key in msg}
        msg["message"] = {key: msg["message"][key] for key in ["id", "role", "content", "model"] if key in msg["message"]}
    elif kind == "result":
        msg = {key: msg[key] for key in ["type", "subtype", "result", "is_error", "duration_ms", "session_id", "errors"] if key in msg}
    elif kind not in ["control_request", "control_response", "control_cancel_request"]:
        continue
    records.append({"dir": record["dir"], "msg": scrub(msg)})
args.destination.parent.mkdir(parents=True, exist_ok=True)
args.destination.write_text("\n".join(json.dumps(record, ensure_ascii=False) for record in records) + "\n")
print(f"Wrote {len(records)} records to {args.destination}")
