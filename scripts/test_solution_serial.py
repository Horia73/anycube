#!/usr/bin/env python3
"""Run the app, vision and independent controller over a Unix pseudoterminal."""
import argparse
import json
import os
import pathlib
import pty
import subprocess
import tempfile
import time
import tty
import urllib.request
from contextlib import ExitStack


def process(stack, args, **options):
    child = subprocess.Popen(args, **options)
    def stop():
        child.terminate()
        child.wait(timeout=5)
    stack.callback(stop)
    return child


def test(binary, source):
    with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
        master, slave = pty.openpty()
        stack.callback(os.close, master)
        stack.callback(os.close, slave)
        tty.setraw(slave)
        env = dict(os.environ, ANYCUBE_SOLUTION_LINK_SERIAL=os.ttyname(slave),
                   ANYCUBE_SOLUTION_LINK_PROTOCOL="native", XDG_DATA_HOME=directory)
        log = stack.enter_context(open(pathlib.Path(directory) / "controller.log", "w+"))
        process(stack, ["python3", "scripts/solution_controller.py", "--camera", source,
                        "--delay", "0", "--frames-dir", directory], stdin=master, stdout=master, stderr=log)
        app = process(stack, [binary, "app", "--no-window"], env=env, stdout=subprocess.PIPE, text=True)
        address = app.stdout.readline().strip()
        vision = process(stack, ["node", "app/tests/solution-link-vision.mjs", directory],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)

        def request(action, body=None):
            data = json.dumps(body).encode() if body is not None else None
            req = urllib.request.Request(address + "api/robot/" + action, data=data,
                                         headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req, timeout=30) as response:
                result = json.load(response)
            assert result.get("ok", True), result
            return result

        def analyse(event):
            print(json.dumps(event), file=vision.stdin, flush=True)
            return vision.stdout.readline().strip()

        robot = request("connect", {"driver": "solution-link"})
        assert robot["info"]["camera"] == ("robot" if source == "png" else source)
        run = request("scan", {})["run"]
        state = None
        since = photos = moves = 0
        end = time.monotonic() + 60
        while time.monotonic() < end:
            for event in request(f"events?since={since}&wait=1")["events"]:
                since = event["seq"]
                if event.get("run") != run:
                    continue
                kind = event["type"]
                assert kind != "error", event
                if kind == "state":
                    state = event["facelets"]
                elif kind == "photo":
                    assert analyse(event) == "ACK"
                    request("photo-taken", {"run": run, "index": event["index"]})
                    photos += 1
                elif kind == "scan-done":
                    assert photos == (0 if source == "state" else 6)
                    request("solve", {"run": run, "size": 3, "facelets": state or analyse({"type": "assemble"})})
                elif kind == "move":
                    assert event["completed"]
                    moves += 1
                elif kind == "done":
                    assert moves == 3
                    request("disconnect", {})
                    log.seek(0)
                    assert "Fixture solved: True" in log.read()
                    print(f"{source}: {photos} photos, {moves} completed moves, fixture solved")
                    return
        raise TimeoutError(source)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=pathlib.Path)
    args = parser.parse_args()
    for mode in ("png", "state", "app"):
        test(args.binary.resolve(), mode)
