import json, subprocess, sys, threading, time, os
kb=sys.argv[1]
p=subprocess.Popen(["codex","app-server"],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=open(kb+"/../err.txt","w"),text=True,bufsize=1,cwd=kb)
nid=0
def send(method,params=None,id_=None):
    global nid
    msg={"jsonrpc":"2.0","method":method}
    if params is not None: msg["params"]=params
    if id_ is not None: msg["id"]=id_
    p.stdin.write(json.dumps(msg)+"\n"); p.stdin.flush()
def req(method,params):
    global nid; nid+=1; send(method,params,nid); return nid
def respond(id_,result):
    p.stdin.write(json.dumps({"jsonrpc":"2.0","id":id_,"result":result})+"\n"); p.stdin.flush()
log=open(kb+"/../events.jsonl","w")
req("initialize",{"clientInfo":{"name":"kmdn-probe","title":"kmdn","version":"0.0.1"},"capabilities":{"experimentalApi":True}})
thread=None; done=False; t0=time.time()
while time.time()-t0<170 and not done:
    line=p.stdout.readline()
    if not line: break
    log.write(line); log.flush()
    try: m=json.loads(line)
    except: continue
    if m.get("id")==1 and "result" in m:
        send("initialized",{})
        req("thread/start",{"cwd":kb,"approvalPolicy":"untrusted","sandbox":"workspace-write","ephemeral":True})
    elif m.get("id")==2 and "result" in m:
        thread=m["result"]["thread"]["id"] if "thread" in m["result"] else m["result"].get("threadId")
        req("turn/start",{"threadId":thread,"input":[{"type":"text","text":"Create two files in the current directory: notes.md containing '# Notes' and scratch.txt containing 'temp'. Use apply_patch. Do not ask questions."}]})
    elif "method" in m and "id" in m:  # server request
        meth=m["method"]; params=m.get("params",{})
        print("SERVER_REQUEST",meth,json.dumps(params)[:400])
        if meth in ("item/fileChange/requestApproval","applyPatchApproval"):
            # deny if any changed path is not .md
            paths=[]
            for c in (params.get("changes") or []):
                paths.append(c.get("path",""))
            if not paths:
                # v2 lacks paths in params; look up from last fileChange item started
                paths=last_paths
            bad=[x for x in paths if not x.endswith(".md")]
            decision="decline" if bad else "accept"
            if meth=="applyPatchApproval": respond(m["id"],{"decision":"denied" if bad else "approved"})
            else: respond(m["id"],{"decision":decision})
            print("DECISION",decision,paths)
        elif meth in ("item/commandExecution/requestApproval","execCommandApproval"):
            respond(m["id"],{"decision":"decline"} if meth.startswith("item") else {"decision":"denied"}); print("DECISION decline command")
        else:
            respond(m["id"],{})
    elif "method" in m:
        meth=m["method"]; params=m.get("params",{})
        if meth=="item/started":
            item=params.get("item",{})
            if item.get("type")=="fileChange":
                last_paths=[c.get("path") for c in item.get("changes",[])]
                print("ITEM fileChange",last_paths)
            else: print("ITEM",item.get("type"))
        elif meth=="item/agentMessage/delta": pass
        elif meth=="turn/completed": print("TURN_COMPLETED",json.dumps(params.get("turn",{}))[:200]); done=True
        elif meth.startswith("error") or meth=="turn/failed": print(meth,json.dumps(params)[:300]); done=True
        elif meth in ("thread/started","turn/started","item/completed"): print(meth, json.dumps(params)[:150])
    elif "error" in m: print("ERROR",json.dumps(m)[:300]); 
last_paths=[]
p.terminate()
