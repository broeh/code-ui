"""Record small real app-server cases in a scratch directory; no auth data is printed."""
import argparse,json,os,selectors,signal,subprocess,time
from pathlib import Path
ap=argparse.ArgumentParser();ap.add_argument('cwd',type=Path);ap.add_argument('out',type=Path);ap.add_argument('--case',default='read');args=ap.parse_args();args.cwd.mkdir(parents=True,exist_ok=True);(args.cwd/'README.md').write_text('# Scratch\nA protocol test.\n')
p=subprocess.Popen(['codex','app-server','--enable','default_mode_request_user_input'],cwd=args.cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,bufsize=0,start_new_session=True);sel=selectors.DefaultSelector();sel.register(p.stdout,selectors.EVENT_READ);records=[];next_id=0

def send(msg):records.append({'dir':'in','msg':msg});p.stdin.write((json.dumps(msg)+'\n').encode());p.stdin.flush()
buffer=b''
def read():
 global buffer
 while b'\n' not in buffer:
  if not sel.select(50):raise TimeoutError('Agent did not produce an event')
  data=os.read(p.stdout.fileno(),65536)
  if not data:raise RuntimeError('App-server exited')
  buffer+=data
 line,buffer=buffer.split(b'\n',1);msg=json.loads(line);records.append({'dir':'out','msg':msg});return msg
def call(method,params):
 global next_id
 next_id+=1;ident=next_id;send({'id':ident,'method':method,'params':params})
 while True:
  msg=read()
  if msg.get('id')==ident and 'method' not in msg:
   if 'error'in msg:raise RuntimeError(msg['error']['message'])
   return msg['result']
try:
 call('initialize',{'clientInfo':{'name':'code_ui','version':'0.1.0'}});send({'method':'initialized'})
 models=call('model/list',{'limit':100})['data'];model=next(m['model'] for m in models if m['model']=='gpt-6-luna')
 policy='on-request' if args.case=='approval' else 'never';sandbox='read-only' if args.case=='approval' else 'danger-full-access'
 thread=call('thread/start',{'cwd':str(args.cwd),'model':model,'approvalPolicy':policy,'approvalsReviewer':'user','sandbox':sandbox,'ephemeral':True})['thread']['id']
 prompts={'read':'Use a shell command to list the files here and read README.md. Reply in one sentence.','approval':'Run touch denied.txt with exec_command and sandbox_permissions=require_escalated, so the client receives an approval request before it runs. Do not use any other method. Reply in one sentence.','question':'Use the request_user_input tool to ask whether I prefer red or blue, then reply with my choice.','edit':'Use apply_patch to create patch.txt containing hello, then reply done. Do not run shell commands.','plain':'Reply exactly: protocol-ready.'}
 call('turn/start',{'threadId':thread,'effort':'low','input':[{'type':'text','text':prompts.get(args.case,prompts['plain'])}]})
 while True:
  msg=read();method=msg.get('method','')
  if 'requestApproval' in method:send({'id':msg['id'],'result':{'decision':'decline'}})
  elif method=='item/tool/requestUserInput':send({'id':msg['id'],'result':{'answers':{q['id']:{'answers':['Blue']} for q in msg['params']['questions']}}})
  if method=='turn/completed':break
finally:
 os.killpg(p.pid,signal.SIGTERM);p.wait(timeout=5)
 # Exact tool traffic is kept locally, then scrubbed before a fixture is committed.
 args.out.write_text('\n'.join(json.dumps(r) for r in records)+'\n');print(f'{args.case}: {len(records)} records')
