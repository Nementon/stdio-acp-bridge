import subprocess
import json
import time
import os


def openai_base_api() -> str:
    val = os.environ.get("OPENAI_BASE_API", None)
    if not val:
        raise RuntimeError("OPENAI_BASE_API environ must be set")
    return val

def openai_api_key() -> str:
    val = os.environ.get("OPENAI_API_KEY", None)
    if not val:
        raise RuntimeError("OPENAI_API_KEY environ must be set")
    return val

def send_msg(proc, msg):
    print("SENDING:", json.dumps(msg))
    proc.stdin.write(json.dumps(msg) + "\n")
    proc.stdin.flush()

proc = subprocess.Popen(
    ["cargo", "run", "--", "--api-base", openai_base_api(), "--api-key", openai_api_key()],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True
)

init_msg = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {}
}
send_msg(proc, init_msg)
line = proc.stdout.readline()
print("RECEIVED:", line.strip())

new_session = {
    "jsonrpc": "2.0",
    "id": 2,
    "method": "session/new",
    "params": {}
}
send_msg(proc, new_session)
line = proc.stdout.readline()
print("RECEIVED:", line.strip())
resp = json.loads(line)
sid = resp["result"]["sessionId"]

prompt_msg = {
    "jsonrpc": "2.0",
    "id": 3,
    "method": "session/prompt",
    "params": {
        "sessionId": sid,
        "content": [{"type": "text", "text": "hello"}]
    }
}
send_msg(proc, prompt_msg)

while True:
    line = proc.stdout.readline()
    if not line:
        break
    print("RECEIVED:", line.strip())
    if "result" in line or "error" in line:
        break

proc.terminate()
