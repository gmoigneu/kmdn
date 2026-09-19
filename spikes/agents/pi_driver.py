import json, subprocess, sys, time
kb, ext = sys.argv[1], sys.argv[2]
p = subprocess.Popen(["pi","--mode","rpc","--no-session","-e",ext], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=open(kb+"/../err.txt","w"), text=True, bufsize=1, cwd=kb)
def send(o): p.stdin.write(json.dumps(o)+"\n"); p.stdin.flush()
log=open(kb+"/../events.jsonl","w")
send({"id":"p1","type":"prompt","message":"Create two files in the current directory: notes.md containing '# Notes' and scratch.txt containing 'temp'. Use the write tool for each. Do not ask questions."})
t0=time.time()
while time.time()-t0<170:
    line=p.stdout.readline()
    if not line: break
    log.write(line); log.flush()
    try: m=json.loads(line)
    except: continue
    t=m.get("type")
    if t=="extension_ui_request":
        print("UI_REQUEST",m.get("method"),json.dumps(m)[:200])
        if m.get("method")=="confirm":
            send({"type":"extension_ui_response","id":m["id"],"confirmed":True,"value":True})
    elif t=="tool_execution_start": print("TOOL_START",m.get("toolName"),json.dumps(m.get("args",{}))[:120])
    elif t=="tool_execution_end": print("TOOL_END",m.get("toolName"),"error" if m.get("isError") else "ok",str(m.get("result",""))[:160].replace("\n"," "))
    elif t=="agent_end": print("AGENT_END"); break
    elif t in ("response","extension_error","error"): print(t,json.dumps(m)[:300])
    elif t=="message_update":
        pass
p.terminate()
