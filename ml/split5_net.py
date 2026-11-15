#!/usr/bin/env python3
"""Train the network that `anycube split5-beam --net` uses as its estimate.

An experiment, not part of any mode. The records come from `anycube
split5-data` (cubes a walk of K moves from the end of phase C, with K) and
from `anycube split5-beam --dump` (the cubes of the ways a search found, with
the moves left, and cubes the search kept, with the moves they have left at
least). A record that is a lower bound costs only where the network says less.

    ml/split5_net.py --data walks.bin [found.bin ...] --out net.bin
    ml/split5_net.py --data walks.bin --fresh target/release/anycube --out net.bin

With `--fresh` the first file is made again before every epoch, with other
walks, so that no cube is seen twice.

Needs numpy and PyTorch; uses the GPU of a Mac if there is one.
"""
import argparse
import struct
import subprocess
import time

import numpy as np
import torch

N_IN, N_ON = 96 + 96 + 576 + 288 + 2, 24 + 24 + 24 + 12 + 1
RECORD = np.dtype([("on", "<u2", (N_ON,)), ("moves", "<f4"), ("bound", "u1"), ("pad", "u1")])


def model(sizes):
    a, b, c = sizes
    return torch.nn.Sequential(
        torch.nn.Linear(N_IN, a), torch.nn.ReLU(),
        torch.nn.Linear(a, b), torch.nn.ReLU(),
        torch.nn.Linear(b, c), torch.nn.ReLU(),
        torch.nn.Linear(c, 1),
    )


def save(net, sizes, path):
    lin = [m for m in net if isinstance(m, torch.nn.Linear)]
    with open(path, "wb") as f:
        f.write(b"S5N1" + struct.pack("<3I", *sizes))
        for i, m in enumerate(lin):
            w = m.weight.detach().cpu().numpy().astype("<f4")
            # By input, as the solver reads them; the last layer has one output.
            f.write((w.T if i < 3 else w[0]).tobytes())
            f.write(m.bias.detach().cpu().numpy().astype("<f4").tobytes())


def load(path):
    raw = open(path, "rb").read()
    assert raw[:4] == b"S5N1", path
    sizes = struct.unpack("<3I", raw[4:16])
    net, at = model(sizes), 16
    shapes = [(N_IN, sizes[0]), (sizes[0], sizes[1]), (sizes[1], sizes[2]), (sizes[2], 1)]
    for m, (n, k) in zip([m for m in net if isinstance(m, torch.nn.Linear)], shapes):
        w = np.frombuffer(raw, "<f4", n * k, at).reshape(n, k)
        b = np.frombuffer(raw, "<f4", k, at + 4 * n * k)
        at += 4 * (n * k + k)
        m.weight.data = torch.tensor(w.T.copy())
        m.bias.data = torch.tensor(b.copy())
    return net, sizes


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", nargs="+", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--init", help="a network to go on from")
    ap.add_argument("--sizes", default="512,256,64")
    ap.add_argument("--epochs", type=int, default=12)
    ap.add_argument("--batch", type=int, default=8192)
    ap.add_argument("--rate", type=float, default=1e-3)
    ap.add_argument("--repeat", type=int, default=1, help="times the files after the first are read in an epoch")
    ap.add_argument("--fresh", help="the anycube binary: new walks in the first file before every epoch")
    ap.add_argument("--count", type=int, default=12_000_000, help="walks an epoch with --fresh")
    args = ap.parse_args()

    dev = torch.device("mps" if torch.backends.mps.is_available() else "cuda" if torch.cuda.is_available() else "cpu")
    def walks(seed, count):
        subprocess.run([args.fresh, "split5-data", "--count", str(count), "--seed", str(seed), "--out", args.data[0]],
                       check=True)

    def read():
        parts = [np.fromfile(p, RECORD) for p in args.data]
        data = np.concatenate([parts[0]] + parts[1:] * args.repeat) if len(parts) > 1 else parts[0]
        return (torch.from_numpy(data["on"].astype(np.int16)), torch.from_numpy(data["moves"].copy()),
                torch.from_numpy(data["bound"].astype(bool)))

    if args.fresh:
        # Walks that no epoch trains on, to measure by.
        walks(999_983, 100_000)
        held = read()
        walks(1000, args.count)
    on, moves, bound = read()
    if not args.fresh:
        pick = torch.from_numpy(np.random.default_rng(1).permutation(len(on))[:100_000])
        held = (on[pick], moves[pick], bound[pick])
    print(f"{len(on):,} records, {int(bound.sum()):,} of them lower bounds, on {dev}", flush=True)

    if args.init:
        net, sizes = load(args.init)
    else:
        sizes = tuple(int(v) for v in args.sizes.split(","))
        net = model(sizes)
    net.to(dev)
    steps = args.epochs * (len(on) // args.batch)
    opt = torch.optim.AdamW(net.parameters(), lr=args.rate, weight_decay=1e-5)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=args.rate, total_steps=steps, pct_start=0.05)

    def run(on):
        x = torch.zeros(len(on), N_IN, device=dev)
        x.scatter_(1, on.to(dev).long(), 1.0)
        return net(x).squeeze(1)

    t = time.time()
    for epoch in range(args.epochs):
        if args.fresh and epoch > 0:
            walks(1000 + epoch, args.count)
            on, moves, bound = read()
        order = torch.randperm(len(on))
        total = 0.0
        net.train()
        for i in range(0, len(order) - args.batch + 1, args.batch):
            idx = order[i:i + args.batch]
            out, want, low = run(on[idx]), moves[idx].to(dev), bound[idx].to(dev)
            exact = torch.nn.functional.smooth_l1_loss(out, want, reduction="none")
            under = torch.relu(want - out) ** 2
            loss = torch.where(low, under, exact).mean()
            opt.zero_grad(set_to_none=True)
            loss.backward()
            opt.step()
            sched.step()
            total += float(loss)
        net.eval()
        with torch.no_grad():
            err = (run(held[0]) - held[1].to(dev)).abs().mean()
        print(f"epoch {epoch + 1:2}: loss {total / (len(order) // args.batch):.4f}, "
              f"walks off by {float(err):.3f}, {time.time() - t:.0f} s", flush=True)
        save(net, sizes, args.out)


if __name__ == "__main__":
    main()
