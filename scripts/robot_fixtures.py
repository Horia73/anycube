"""Writes tests/fixtures/multicuber_planner.json: what MultiCuber's robot.py
Planner makes of many random move sequences, so that the Rust port
(src/robot/multicuber/planner.rs) can be checked against it.

    python3 scripts/robot_fixtures.py /path/to/MultiCuber/robot.py > tests/fixtures/multicuber_planner.json
"""
import importlib.util
import json
import random
import sys


def load(path):
    spec = importlib.util.spec_from_file_location("robot", path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["robot"] = mod
    spec.loader.exec_module(mod)
    return mod


def random_moves(n, count, rng, rotations):
    out = []
    for _ in range(count):
        if rotations and rng.random() < 0.08:
            out.append(rng.choice("xyz") + rng.choice(["", "'", "2"]))
            continue
        depth = 1
        if n > 3 and rng.random() < 0.4:
            depth = rng.randint(2, n - 1)
        face = rng.choice("URFDLB")
        head = face if depth == 1 else (f"{face}w" if depth == 2 else f"{depth}{face}w")
        out.append(head + rng.choice(["", "'", "2"]))
    return out


def op_text(op):
    text = op.line()
    if op.remap:
        text += " @" + op.remap
    if op.photo:
        text += " #%s:%d" % (op.photo[0][len("rubiks-side-"):][0], op.photo[1])
    return text


def main():
    robot = load(sys.argv[1])
    rng = random.Random(20261126)
    cases = []
    for n in (3, 4, 5, 6, 7):
        for k in range(60 if n <= 5 else 15):
            moves = random_moves(n, rng.randint(0, 30), rng, rotations=k % 3 == 0)
            b_rule = "legacy" if k % 4 else "flipper"
            flip_up = k % 2 == 0
            p = robot.Planner(n, b_rule)
            p.flip_up = flip_up
            if k % 5 == 4:
                p.lift = rng.choice([0, "SCAN", 1, n])
            lift0 = p.lift
            p.solution(robot.parse_moves(moves, n))
            cases.append({"kind": "solution", "n": n, "b_rule": b_rule, "flip_up": flip_up, "lift": str(lift0),
                          "moves": " ".join(moves), "ops": [op_text(o) for o in p.ops],
                          "end": {"lift": str(p.lift), "flip_up": p.flip_up}})
        for flip_up in (True, False):
            for normal in (False, True):
                p = robot.Planner(n)
                p.flip_up = flip_up
                p.scan(normal)
                cases.append({"kind": "scan", "n": n, "flip_up": flip_up, "scan_flip_normal": normal, "lift": "0",
                              "ops": [op_text(o) for o in p.ops], "end": {"lift": str(p.lift), "flip_up": p.flip_up}})
        # A whole session as robot.py's CubeSession drives one planner: scan, solve, park.
        p = robot.Planner(n)
        p.scan()
        scan_ops, p.ops = p.ops, []
        moves = random_moves(n, 20, rng, rotations=False)
        p.solution(robot.parse_moves(moves, n))
        solve_ops, p.ops = p.ops, []
        p.park()
        cases.append({"kind": "session", "n": n, "moves": " ".join(moves), "scan": [op_text(o) for o in scan_ops],
                      "solve": [op_text(o) for o in solve_ops], "park": [op_text(o) for o in p.ops]})
        # robot.plan_solution, which assumes the flipper DOWN after a scan.
        moves = random_moves(n, 25, rng, rotations=False)
        cases.append({"kind": "plan_solution", "n": n, "moves": " ".join(moves),
                      "ops": [op_text(o) for o in robot.plan_solution(n, moves)]})
    json.dump({"source": "MultiCuber robot.py (firmware-drv8825)", "cases": cases}, sys.stdout, separators=(",", ":"))
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
