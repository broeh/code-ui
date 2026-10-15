"""Record small Muse MSP cases in a scratch directory using the existing CLI login."""
import argparse
import json
import os
import selectors
import signal
import subprocess
import time
import uuid
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('cwd', type=Path)
parser.add_argument('out', type=Path)
parser.add_argument('--case', choices=['plain','read','edit','question','approval'], default='plain')
args = parser.parse_args()
args.cwd.mkdir(parents=True, exist_ok=True)
(args.cwd/'README.md').write_text('# Scratch\nA Muse protocol test.\n')
command = ['muse','serve']
if args.case != 'approval': command += ['--disable-sandbox','--trust-workspace']
p = subprocess.Popen(command,cwd=args.cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,bufsize=0,start_new_session=True)
selector=selectors.DefaultSelector();selector.register(p.stdout,selectors.EVENT_READ)
buffer=b'';records=[];next_id=0

def send(message):
    message={'jsonrpc':'2.0',**message}
    records.append({'dir':'in','msg':message})
    p.stdin.write((json.dumps(message)+'\n').encode())

def request(method,params):
    global next_id
    next_id+=1;send({'id':next_id,'method':method,'params':params});return next_id

def read():
    global buffer
    while b'\n' not in buffer:
        if not selector.select(50):raise TimeoutError('Muse did not produce an event')
        data=os.read(p.stdout.fileno(),65536)
        if not data:raise RuntimeError('Muse exited')
        buffer+=data
    line,buffer=buffer.split(b'\n',1)
    message=json.loads(line);records.append({'dir':'out','msg':message});return message

def present(message):
    method=message.get('method')
    if method not in ['approval/request','userInput/request'] or 'id' not in message:return
    params=message['params'];send({'id':message['id'],'result':{}})
    if method=='approval/request':
        choices=params['availableChoices'];choice=next(c for c in choices if c['scope']=='once' and c['decision'] in ['denied','abort'])
        request('approval/decide',{'commandId':str(uuid.uuid7()),'sessionId':params['sessionId'],'approvalId':params['approvalId'],'choiceId':choice['choiceId'],'requirementId':params['currentRequirementId']})
    else:
        answers=[]
        for q in params['questions']:
            option=next((o for o in q['options'] if 'blue' in o['label'].lower()),None)
            answer={'questionId':q['id']}
            if option:
                answer['selectedLabels' if q['selection']['mode']=='multiple' else 'selectedLabel']=[option['label']] if q['selection']['mode']=='multiple' else option['label']
            else:answer['freeText']='Blue'
            answers.append(answer)
        request('userInput/answer',{'commandId':str(uuid.uuid7()),'sessionId':params['sessionId'],'userInputId':params['userInputId'],'answers':answers})

def call(method,params):
    ident=request(method,params)
    while True:
        message=read();present(message)
        if message.get('id')==ident and 'method' not in message:
            if 'error' in message:raise RuntimeError(message['error']['message'])
            return message['result']

try:
    call('initialize',{'clientInfo':{'name':'code_ui_probe','version':'0.1.0'},'capabilities':{'userInputDialogs':True}})
    send({'method':'initialized'})
    models=call('model/list',{})
    print('Catalog:',[m['modelId'] for m in models['models']],flush=True)
    session=call('session/start',{'commandId':str(uuid.uuid7()),'workspaceRoot':str(args.cwd),'approvalMode':'onRequest' if args.case=='approval' else 'allowAll','modelId':'muse-spark-1.2'})['session']['sessionId']
    prompts={
        'plain':'Reply exactly muse-ready. Do not use tools.',
        'read':'List this folder and read README.md using tools, then reply in one sentence.',
        'edit':'Create hello.txt containing hello, then reply done.',
        'question':'Ask whether I prefer red or blue using the built-in user-input dialog. Wait for my answer, then reply with that choice.',
        'approval':'Use a shell tool to create denied.txt. Ask for approval before doing so. Do not use another method. Reply in one sentence.'
    }
    call('turn/start',{'commandId':str(uuid.uuid7()),'sessionId':session,'input':[{'type':'text','text':prompts[args.case]}],'reasoningEffort':'low','ifBusy':'queue'})
    while True:
        message=read();present(message)
        if message.get('method')=='turn/completed':
            print('Terminal:',message['params']['terminal'],flush=True);break
finally:
    os.killpg(p.pid,signal.SIGTERM);p.wait(timeout=5)
    args.out.write_text('\n'.join(json.dumps(r) for r in records)+'\n')
    print(args.case,len(records),'records',flush=True)
