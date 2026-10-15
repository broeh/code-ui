#!/usr/bin/env python3
"""Synthetic Codex app-server for native CI. Never contacts a provider."""
import json
import os
import sys
from pathlib import Path

if "--version" in sys.argv:
    print("fake-codex 0.0.0")
    raise SystemExit(0)


def emit(value):
    print(json.dumps(value), flush=True)


for line in sys.stdin:
    message = json.loads(line)
    with Path(os.environ["CODE_UI_FAKE_LOG"]).open("a") as log:
        log.write(json.dumps(message) + "\n")
    if "id" not in message:
        continue
    method = message.get("method")
    result = {}
    if method in ("thread/start", "thread/resume"):
        result = {"thread": {"id": "synthetic-native-thread"}, "model": "synthetic-model"}
    elif method == "model/list":
        result = {"data": [{"model": "synthetic-model", "displayName": "Synthetic model", "isDefault": True, "supportedReasoningEfforts": []}], "nextCursor": None}
    elif method == "turn/start":
        result = {"turn": {"id": "synthetic-turn"}}
    emit({"id": message["id"], "result": result})
    if method == "turn/start":
        item = {"id": "synthetic-message", "type": "agentMessage", "phase": "final_answer", "text": "Synthetic native reply"}
        emit({"method": "item/completed", "params": {"item": item}})
        emit({"method": "turn/completed", "params": {"turn": {"id": "synthetic-turn", "status": "completed", "items": [item], "durationMs": 1}}})
