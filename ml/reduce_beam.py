"""Experiment: beam search for a 4x4x4 reduction with all moves, guided by the network of
reduce_net.py. Every level keeps the `width` cubes the network puts closest to a reduced cube
and stops at the first level that contains a reduced cube.

Usage:
  ./target/release/cubenxnxn free-export bench/scrambles-1000.txt --limit 20 --solve > DIR/cubes.json
  python ml/reduce_beam.py --work DIR --widths 1024,4096,16384 --out DIR/found.tsv
  ./target/release/cubenxnxn free-verify DIR/found.tsv
"""
import argparse, json, os, time
import numpy as np
import torch
from reduce_net import Net, features, device


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--work", required=True)
    ap.add_argument("--cubes", default=None)
    ap.add_argument("--widths", default="1024,4096,16384")
    ap.add_argument("--limit", type=int, default=20)
    ap.add_argument("--max-depth", type=int, default=30)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    dev = device()
    data = json.load(open(args.cubes or os.path.join(args.work, "cubes.json")))
    ck = torch.load(os.path.join(args.work, "net.pt"), map_location=dev)
    net = Net(ck["width"], ck["blocks"]).to(dev)
    net.load_state_dict(ck["state"])
    net.eval()

    ct_src = torch.tensor(data["ct_src"], device=dev)          # (27, 24)
    ep_src = torch.tensor(data["ep_src"], device=dev)
    rots = torch.tensor(data["rots"], device=dev)              # (24, 24)
    rot_parity = torch.tensor(data["rot_parity"], device=dev)
    names = data["names"]
    # skip[last + 1][m]: move m must not follow move `last` (same face, or same axis in decreasing order).
    skip = torch.zeros(28, 27, dtype=torch.bool)
    for last in range(27):
        for m in range(27):
            a, b = last // 3, m // 3
            skip[last + 1, m] = a == b or (a % 3 == b % 3 and last > m)
    skip = skip.to(dev)
    quarter = torch.tensor([m % 3 != 1 for m in range(27)], device=dev)
    wide_quarter = torch.tensor([m >= 18 and m % 3 != 1 for m in range(27)], device=dev)
    lo, hi = torch.arange(12, device=dev), torch.arange(12, 24, device=dev)
    inv_i, inv_j = np.triu_indices(12, 1)
    inv_i, inv_j = torch.tensor(inv_i, device=dev), torch.tensor(inv_j, device=dev)

    def score(ct, ep, wp):
        pos = torch.zeros_like(ep).scatter_(1, ep, torch.arange(24, device=dev).expand_as(ep))
        partner = pos.gather(1, (ep + 12) % 24)
        out = []
        with torch.no_grad():
            for s in range(0, ct.shape[0], 1 << 16):
                out.append(net(features(ct[s:s + (1 << 16)], partner[s:s + (1 << 16)], wp[s:s + (1 << 16)])))
        return torch.cat(out)

    def reduced(ct, ep, wp, cp):
        paired = (ep[:, hi] == (ep[:, lo] + 12) % 24).all(1) & (wp == 0)
        idx = paired.nonzero().squeeze(1)
        if idx.numel() == 0:
            return idx
        c, e = ct[idx], ep[idx]
        match = (c[:, None, :] == rots[None]).all(2)             # (n, 24)
        has = match.any(1)
        r = match.float().argmax(1)
        d = e[:, :12] % 12
        par = (d[:, inv_i] > d[:, inv_j]).sum(1) % 2
        ok = has & ((par ^ cp[idx]) == rot_parity[r])
        return idx[ok]

    def beam(cube, width):
        ct = torch.tensor([cube["ct"]], device=dev)
        ep = torch.tensor([cube["ep"]], device=dev)
        wp = torch.tensor([perm_parity(cube["ep"])], device=dev)
        cp = torch.tensor([cube["corner_parity"]], device=dev)
        last = torch.full((1,), -1, device=dev)
        links = []
        looked = 0
        for depth in range(1, args.max_depth + 1):
            n = ct.shape[0]
            valid = ~skip[last + 1]                               # (n, 27)
            parent, move = valid.nonzero(as_tuple=True)
            c2 = ct[parent[:, None], ct_src[move]]
            e2 = ep[parent[:, None], ep_src[move]]
            w2 = wp[parent] ^ wide_quarter[move].long()
            p2 = cp[parent] ^ quarter[move].long()
            looked += parent.numel()
            done = reduced(c2, e2, w2, p2)
            if done.numel() > 0:
                k = done[0]
                path = [int(move[k])]
                p = int(parent[k])
                for par, mv in reversed(links):
                    path.append(int(mv[p]))
                    p = int(par[p])
                return list(reversed(path)), looked
            s = score(c2, e2, w2)
            if s.numel() > width:
                keep = torch.topk(s, width, largest=False).indices
                parent, move, c2, e2, w2, p2 = parent[keep], move[keep], c2[keep], e2[keep], w2[keep], p2[keep]
            links.append((parent.cpu(), move.cpu()))
            ct, ep, wp, cp, last = c2, e2, w2, p2, move
        return None, looked

    out = open(args.out, "w") if args.out else None
    cubes = data["cubes"][:args.limit]
    for width in [int(w) for w in args.widths.split(",")]:
        lens, solver, secs, failed = [], [], 0.0, 0
        for cube in cubes:
            t = time.time()
            path, looked = beam(cube, width)
            secs += time.time() - t
            if path is None:
                failed += 1
                continue
            lens.append(len(path))
            solver.append(cube["solver"])
            if out:
                out.write(cube["scramble"] + "\t" + " ".join(names[m] for m in path) + "\n")
        mean = sum(lens) / max(1, len(lens))
        print(f"width {width}: reduction found on {len(lens)} of {len(cubes)} cubes, mean length {mean:.2f}, "
              f"solver on the same cubes {sum(solver) / max(1, len(solver)):.2f}, {secs / len(cubes):.1f} s per cube, "
              f"lengths {sorted(lens)}", flush=True)
    if out:
        out.close()


def perm_parity(p):
    seen, parity = [False] * len(p), 0
    for i in range(len(p)):
        if not seen[i]:
            j, n = i, 0
            while not seen[j]:
                seen[j] = True
                j = p[j]
                n += 1
            parity ^= (n - 1) & 1
    return parity


if __name__ == "__main__":
    main()
