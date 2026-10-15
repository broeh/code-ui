"""Record Antigravity print-mode traffic in a scratch folder."""
import argparse
import json
import os
import selectors
import signal
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('cwd', type=Path)
parser.add_argument('out', type=Path)
parser.add_argument('--case', choices=['read', 'question', 'plan', 'plain'], default='read')
args = parser.parse_args()
args.cwd.mkdir(parents=True, exist_ok=True)
(args.cwd / 'README.md').write_text('# Scratch\nAn Antigravity protocol test.\n')
command = ['agy', '--input-format', 'stream-json', '--output-format', 'stream-json', '-p=', '--model', 'gemini-3.8-flash-low']
command += ['--mode', 'plan'] if args.case == 'plan' else ['--dangerously-skip-permissions']
p = subprocess.Popen(command, cwd=args.cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=0, start_new_session=True)
selector = selectors.DefaultSelector()
selector.register(p.stdout, selectors.EVENT_READ)
records = []
prompts = {
    'read': 'Use list_dir and view_file to list this folder and read README.md. Reply in one sentence.',
    'question': 'Use ask_question to ask whether I prefer red or blue. Do not guess. Ask before doing any work.',
    'plan': 'Plan how to create a greeting.txt containing hello. Do not create or edit any files.',
    'plain': 'Reply exactly protocol-ready.'
}
message = {'event': 'user', 'message': {'role': 'user', 'content': prompts[args.case]}}
records.append({'dir': 'in', 'msg': message})
p.stdin.write((json.dumps(message)+'\n').encode())
p.stdin.flush()
buffer = b''
deadline = time.monotonic()+60
try:
    while time.monotonic()<deadline:
        if not selector.select(1):
            if p.poll() is not None: break
            continue
        data = os.read(p.stdout.fileno(),65536)
        if not data: break
        buffer += data
        while b'\n' in buffer:
            line, buffer = buffer.split(b'\n',1)
            try: msg=json.loads(line)
            except json.JSONDecodeError: continue
            records.append({'dir':'out','msg':msg})
            if msg.get('event')=='result':
                print('Result status:',msg.get('result',{}).get('status'))
                raise StopIteration
except StopIteration:
    pass
finally:
    os.killpg(p.pid,signal.SIGTERM)
    p.wait(timeout=5)
    args.out.write_text('\n'.join(json.dumps(record) for record in records)+'\n')
    print(args.case, len(records), 'records')
