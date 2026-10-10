"""SDK-free exited-child control. No browser, SDK, forced GC or memory reclaim."""
import json
import sys

print(json.dumps({"type": "phase", "row": {"phase": "idle"}}), flush=True)
assert sys.stdin.readline().strip() == "ack"
data = bytearray(64 * 1024 * 1024)
for n in range(0, len(data), 4096):
    data[n] = 17
assert sum(data[::4096]) == 278528
print(json.dumps({"type": "phase", "row": {"phase": "held-64MiB", "payloadBytes": len(data)}}), flush=True)
assert sys.stdin.readline().strip() == "ack"
data = None
print(json.dumps({"type": "result", "status": "passed", "payloadBytes": 0}), flush=True)
