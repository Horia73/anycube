#!/usr/bin/env python3
"""Drives `anycube app --robot-sim` through its robot API (robots-spec/API.md)
as the page does, with no page, no camera and no cube: connect to the
simulated robot, scan (each photo acknowledged), read the cube off the
photos' `sim` stickers, solve, wait for `done`. Each run after the first
reconnects, which gives the simulator a new scramble.

    anycube app --no-window --port 8765 --robot-sim=0 &
    python3 scripts/robot_smoke.py http://127.0.0.1:8765 8

A run is good when it ends with `done` and the simulator's log says "cube
solved". Prints each run's compute time and candidates, the robot time of the
chosen solution beside that of the plain solver's answer, and a summary; the
exit status is 1 if any run was not good or any plan was slower than the plain
solver's. The first solve of a fresh server is the one to look at for a cold
start.
"""
import json
import sys
import time
import urllib.request

BASE = sys.argv[1].rstrip("/")
RUNS = int(sys.argv[2]) if len(sys.argv) > 2 else 1
SIZE = 3  # the colours are named after the middles: an odd size


def req(path, body=None):
    data = None if body is None else json.dumps(body).encode()
    r = urllib.request.Request(BASE + path, data=data, method="GET" if body is None else "POST")
    if body is not None:
        r.add_header("Content-Type", "application/json")
        r.add_header("Origin", BASE)
    with urllib.request.urlopen(r, timeout=30) as f:
        return json.loads(f.read())


def turned(stickers, degrees, n):
    """A face's stickers, row by row, turned clockwise as the page turns the picture."""
    grid = [list(stickers[i * n:(i + 1) * n]) for i in range(n)]
    for _ in range((degrees // 90) % 4):
        grid = [list(row) for row in zip(*grid[::-1])]
    return "".join("".join(row) for row in grid)


def connect():
    answer = req("/api/robot/connect", {"driver": "multicuber", "port": "sim", "size": SIZE})
    if not answer.get("ok"):
        sys.exit(f"connect refused: {answer}")
    return answer


print("robots:", json.dumps(req("/api/robots"))[:300])
print("connect:", json.dumps(connect())[:200])
seq = req("/api/robot/events?since=0&wait=0")["seq"]
good, computes, candidates, slower = 0, [], [], 0
for k in range(RUNS):
    if k:
        req("/api/robot/disconnect", {})
        connect()
    started = req("/api/robot/scan", {"size": SIZE})
    if not started.get("ok"):
        print(f"run {k + 1}: scan refused {started}")
        continue
    run, faces, order, result, solved, t0 = started["run"], {}, [], None, None, time.time()
    # The simulator's verdict comes in a log line after `done`: read on for it a moment.
    while result is None or (result["type"] == "done" and solved is None and time.time() - done_at < 3):
        ev = req(f"/api/robot/events?since={seq}&wait={10 if result is None else 1}")
        seq = ev["seq"]
        for e in ev["events"]:
            kind = e["type"]
            if kind == "log":
                print("   log", e.get("text"), flush=True)
                if e.get("run") == run and "simulated robot: cube" in e.get("text", "") and result is not None:
                    solved = "cube solved" in e["text"]
            if e.get("run") not in (None, run):
                continue
            if kind == "photo":
                faces[e["face"]] = turned(e["sim"], e["rotate"], SIZE)
                order.append(f"{e['face']}{e['rotate']}")
                req("/api/robot/photo-taken", {"run": run, "index": e["index"]})
            elif kind == "scan-done":
                raw = "".join(faces[f] for f in "URFDLB")
                middle = SIZE * SIZE // 2
                name = {faces[f][middle]: f for f in "URFDLB"}
                facelets = "".join(name[x] for x in raw)
                print(f"  scan-done {e['scan_ms']} ms order={order}")
                r = req("/api/robot/solve", {"run": run, "size": SIZE, "facelets": facelets, "analysis_ms": 0})
                if not r.get("ok"):
                    result = {"type": "refused", **r}
            elif kind == "plan":
                computes.append(e["compute_ms"])
                candidates.append(e.get("candidates"))
                plain = e.get("plain") or {}
                print(f"  plan: {len(e['moves'])} moves, {e.get('est_ms')} ms of robot time, compute {e['compute_ms']} ms, "
                      f"candidates {e.get('candidates')}; the plain solver's {plain.get('moves')} moves: {plain.get('est_ms')} ms"
                      f"{'' if plain.get('complete') else ' (its search was cut short)'}")
                if plain and e.get("est_ms", 0) > plain.get("est_ms", 0):
                    slower += 1
            elif kind in ("done", "error", "disconnected") and result is None:
                result, done_at = e, time.time()
    ok = result["type"] == "done" and solved is True
    good += ok
    print(f"run {k + 1}: {'good' if ok else 'BAD'} {result['type']}", json.dumps(result.get("times") or result)[:300],
          f"solved={solved} wall {time.time() - t0:.1f}s", flush=True)
if computes:
    rest = computes[1:] or computes
    print(f"summary: {good}/{RUNS} good; compute ms first {computes[0]}, then mean {sum(rest) / len(rest):.1f} max {max(rest)}; "
          f"candidates first {candidates[0]}, then mean {sum(candidates[1:] or candidates) / len(candidates[1:] or candidates):.0f} "
          f"min {min(candidates[1:] or candidates)}")
if slower:
    print(f"{slower} plans slower than the plain solver's answer: that must never be")
sys.exit(0 if good == RUNS and not slower else 1)
