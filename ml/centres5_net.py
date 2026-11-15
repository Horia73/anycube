"""Experiment: a network that estimates how many moves stage the centres of a 5x5x5.

Data comes from `anycube centres5-dump`: 58-byte records of a cube K random moves away
from staged centres (48 axes of the centres, x orbit then t orbit; the parity of the wings;
8 exact distances from the tables; K). The network sees the axis at each position, the
parity and the distances, and is trained to say K. `src/learned5.rs` reads what `export`
writes and searches with it.

K is a poor label: 24 of the 36 moves leave staged centres staged, and about half the cubes
K moves away are nearer than K. `tune` trains on exact distances instead: records from
`centres5-dump --exact 11` (files DIR/x-*.bin), each turned by a random one of the 48
symmetries of the cube (DIR/syms.bin, from `centres5-dump --syms`).

Usage:
  python ml/centres5_net.py train --bin ./target/release/anycube --work DIR [--width 512 --blocks 2]
  python ml/centres5_net.py eval --bin ./target/release/anycube --work DIR
  python ml/centres5_net.py tune --work DIR --name exact [--init net] [--width 512 --blocks 2]
"""
import argparse, glob, os, struct, subprocess, time
import numpy as np
import torch
import torch.nn as nn

REC = 58
N_IN = 153


def device():
    return torch.device("mps" if torch.backends.mps.is_available() else "cuda" if torch.cuda.is_available() else "cpu")


def features(rec):
    """rec: (B, 58) uint8 tensor -> (B, 153) float, the inputs as `learned5::Tools::inputs` builds them."""
    axes = rec[:, :48].long()
    onehot = torch.zeros(rec.shape[0], 48, 3, device=rec.device)
    onehot.scatter_(2, axes.unsqueeze(2), 1.0)
    return torch.cat([onehot.flatten(1), rec[:, 48:49].float(), rec[:, 49:57].float() / 8.0], dim=1)


class Block(nn.Module):
    def __init__(self, width):
        super().__init__()
        self.a, self.b = nn.Linear(width, width), nn.Linear(width, width)
        self.na, self.nb = nn.BatchNorm1d(width), nn.BatchNorm1d(width)

    def forward(self, x):
        y = torch.relu(self.na(self.a(x)))
        return torch.relu(x + self.nb(self.b(y)))


class Net(nn.Module):
    def __init__(self, width=512, blocks=2):
        super().__init__()
        self.inp, self.ninp = nn.Linear(N_IN, width), nn.BatchNorm1d(width)
        self.blocks = nn.ModuleList([Block(width) for _ in range(blocks)])
        self.out = nn.Linear(width, 1)

    def forward(self, x):
        h = torch.relu(self.ninp(self.inp(x)))
        for b in self.blocks:
            h = b(h)
        return self.out(h).squeeze(1)


def dump(args, count, seed, path, kmax=None):
    subprocess.run([args.bin, "centres5-dump", "--count", str(count), "--kmin", "1", "--kmax", str(kmax or args.kmax),
                    "--seed", str(seed), "--out", path], check=True, stderr=subprocess.DEVNULL)
    return np.fromfile(path, dtype=np.uint8).reshape(-1, REC)


def folded(linear, norm):
    """Weights (inputs by outputs) and bias of a layer with the normalisation after it folded in."""
    w, b = linear.weight.detach().cpu().double(), linear.bias.detach().cpu().double()
    if norm is not None:
        scale = norm.weight.detach().cpu().double() / torch.sqrt(norm.running_var.detach().cpu().double() + norm.eps)
        w = w * scale[:, None]
        b = (b - norm.running_mean.detach().cpu().double()) * scale + norm.bias.detach().cpu().double()
    return w.t().contiguous().float().numpy(), b.float().numpy()


def export(net, path, check):
    """Writes the network for `learned5::Net::load`, with a few cubes and what it says of them."""
    net.eval()
    width, blocks = net.inp.out_features, len(net.blocks)
    with open(path, "wb") as f:
        f.write(b"C5NET1\0\0" + struct.pack("<III", N_IN, width, blocks))
        layers = [(net.inp, net.ninp)] + [p for b in net.blocks for p in ((b.a, b.na), (b.b, b.nb))] + [(net.out, None)]
        for linear, norm in layers:
            w, b = folded(linear, norm)
            f.write(w.tobytes() + b.tobytes())
        dev = next(net.parameters()).device
        with torch.no_grad():
            said = net(features(torch.as_tensor(check, device=dev))).cpu().numpy().astype(np.float32)
        f.write(struct.pack("<I", len(check)))
        for r, v in zip(check, said):
            f.write(r.tobytes() + struct.pack("<f", v))


def train(args):
    dev = device()
    net = Net(args.width, args.blocks).to(dev)
    print("parameters", sum(p.numel() for p in net.parameters()), "device", dev, flush=True)
    opt = torch.optim.AdamW(net.parameters(), lr=args.lr, weight_decay=1e-5)
    steps = args.samples // args.batch
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=args.lr, total_steps=args.epochs * steps)
    path = os.path.join(args.work, f"epoch-{args.name}.bin")
    t0 = time.time()
    for epoch in range(args.epochs):
        # Fresh cubes every epoch: the generator makes two million a second.
        raw = torch.as_tensor(dump(args, args.samples, 1000 + epoch, path), device=dev)
        net.train()
        total, n = 0.0, 0
        for s in range(steps):
            rec = raw[s * args.batch:(s + 1) * args.batch]
            loss = nn.functional.mse_loss(net(features(rec)), rec[:, 57].float())
            opt.zero_grad(set_to_none=True)
            loss.backward()
            opt.step()
            sched.step()
            if s % 50 == 0:
                total, n = total + loss.item(), n + 1
        print(f"epoch {epoch} loss {total / n:.4f} time {time.time() - t0:.0f} s", flush=True)
        torch.save({"state": net.state_dict(), "width": args.width, "blocks": args.blocks, "kmax": args.kmax},
                   os.path.join(args.work, f"{args.name}.pt"))
    os.remove(path)
    evaluate(args)


class Turns:
    """The 48 symmetries, as index tables that turn records on the device."""

    def __init__(self, path, dev):
        raw = np.fromfile(path, dtype=np.uint8).reshape(-1, 51).astype(np.int64)
        src, axes, dist = [], [], []
        for g in raw:
            px, pt, ax = g[:24], g[24:48], g[48:]
            # The position each position of the image takes its centre from.
            s = np.zeros(48, dtype=np.int64)
            s[px] = np.arange(24)
            s[24 + pt] = 24 + np.arange(24)
            inv = np.argsort(ax)
            src.append(s)
            axes.append(ax)
            dist.append(np.concatenate([[0, 1], 2 + inv, 5 + inv]))
        self.src = torch.as_tensor(np.array(src), device=dev)
        self.axes = torch.as_tensor(np.array(axes), device=dev)
        self.dist = torch.as_tensor(np.array(dist), device=dev)
        check = np.fromfile(path + ".check", dtype=np.uint8).reshape(-1, 2, REC)
        cube = torch.as_tensor(check[:, 0], device=dev)
        image = self.turn(cube, torch.arange(len(raw), device=dev))
        assert torch.equal(image[:, :57], torch.as_tensor(check[:, 1, :57], device=dev)), "cubes are turned as learned5 turns them"

    def turn(self, rec, g):
        """rec: (B, 58) uint8; g: (B,) symmetry of each -> the records of the images."""
        axes = self.axes[g].gather(1, rec[:, :48].long().gather(1, self.src[g]))
        dist = rec[:, 49:57].long().gather(1, self.dist[g])
        return torch.cat([axes.to(torch.uint8), rec[:, 48:49], dist.to(torch.uint8), rec[:, 57:]], dim=1)


def target(rec):
    """The distance a record stands for: exact up to 11; a cube further than 11 after a walk
    of 12 is at 12, and after a longer walk it is taken for 13."""
    lab = rec[:, 57].float()
    return torch.where(lab >= 100, torch.clamp(lab - 100, max=13.0), lab)


def tune(args):
    dev = device()
    files = sorted(glob.glob(os.path.join(args.work, "x-*.bin")))
    raw = np.concatenate([np.fromfile(f, dtype=np.uint8).reshape(-1, REC) for f in files])
    # The records of one cube are next to each other: hold out the end, not a random part.
    held = max(len(raw) // 50, 1)
    val, raw = raw[-held:], raw[:-held]
    raw = raw[:int(len(raw) * args.take)]
    lab = raw[:, 57]
    print("files", len(files), "records", len(raw), "held out", len(val), flush=True)
    turns = Turns(os.path.join(args.work, "syms.bin"), dev)
    if args.init:
        ck = torch.load(os.path.join(args.work, f"{args.init}.pt"), map_location=dev)
        net = Net(ck["width"], ck["blocks"]).to(dev)
        net.load_state_dict(ck["state"])
        args.width, args.blocks = ck["width"], ck["blocks"]
    else:
        net = Net(args.width, args.blocks).to(dev)
    print("parameters", sum(p.numel() for p in net.parameters()), "device", dev, flush=True)
    # Cubes a few moves from staged are many (every staging found passes through them) and easy.
    near = np.nonzero(lab <= args.near)[0]
    rest = np.nonzero(lab > args.near)[0]
    per_epoch = len(rest) + int(len(near) * args.near_share)
    steps = per_epoch // args.batch
    opt = torch.optim.AdamW(net.parameters(), lr=args.lr, weight_decay=1e-5)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=args.lr, total_steps=args.epochs * steps)
    data = torch.as_tensor(raw, device=dev)
    rng = np.random.default_rng(1)
    t0 = time.time()
    for epoch in range(args.epochs):
        pick = np.concatenate([rest, rng.choice(near, int(len(near) * args.near_share), replace=False)])
        order = torch.as_tensor(rng.permutation(pick), device=dev)
        net.train()
        total, n = 0.0, 0
        for s in range(steps):
            rec = data[order[s * args.batch:(s + 1) * args.batch]]
            rec = turns.turn(rec, torch.randint(0, 48, (rec.shape[0],), device=dev))
            loss = nn.functional.mse_loss(net(features(rec)), target(rec))
            opt.zero_grad(set_to_none=True)
            loss.backward()
            opt.step()
            sched.step()
            if s % 50 == 0:
                total, n = total + loss.item(), n + 1
        print(f"epoch {epoch} loss {total / n:.4f} held out {held_out(net, val, dev)[0]:.4f} time {time.time() - t0:.0f} s", flush=True)
        torch.save({"state": net.state_dict(), "width": args.width, "blocks": args.blocks, "kmax": args.kmax},
                   os.path.join(args.work, f"{args.name}.pt"))
    _, pred = held_out(net, val, dev)
    lab = val[:, 57].astype(int)
    print("distance   cubes   estimate       mean error")
    for v in sorted(set(lab)):
        p = pred[lab == v]
        want = min(v - 100, 13) if v >= 100 else v
        name = f"{v:3d}     " if v < 100 else f">11, K {v - 100:2d}"
        print(f"{name}  {len(p):6d}   {p.mean():5.2f} +- {p.std():4.2f}   {np.abs(p - want).mean():5.2f}")
    far = pred[lab > 112]
    for v in (9, 10, 11, 112):
        if (lab == v).any():
            p = pred[lab == v]
            # How often a cube at this distance looks nearer than one of the far cubes.
            rank = (np.searchsorted(np.sort(far), p) / len(far)).mean()
            print(f"a cube at {v if v < 100 else 12} is put before {100 - rank * 100:.1f}% of the cubes more than 12 away")
    out = os.path.join(args.work, f"{args.name}.bin")
    export(net, out, val[:64])
    print("written", out)


def held_out(net, val, dev):
    net.eval()
    with torch.no_grad():
        rec = torch.as_tensor(val, device=dev)
        pred = torch.cat([net(features(rec[s:s + 65536])) for s in range(0, len(val), 65536)])
        loss = nn.functional.mse_loss(pred, target(rec)).item()
    return loss, pred.cpu().numpy()


def evaluate(args):
    dev = device()
    ck = torch.load(os.path.join(args.work, f"{args.name}.pt"), map_location=dev)
    net = Net(ck["width"], ck["blocks"]).to(dev)
    net.load_state_dict(ck["state"])
    net.eval()
    raw = dump(args, 1_600_000, 7, os.path.join(args.work, f"eval-{args.name}.bin"), kmax=max(ck.get("kmax", 16), 16))
    os.remove(os.path.join(args.work, f"eval-{args.name}.bin"))
    with torch.no_grad():
        pred = torch.cat([net(features(torch.as_tensor(raw[s:s + 65536], device=dev))).cpu()
                          for s in range(0, len(raw), 65536)]).numpy()
    k = raw[:, 57].astype(int)
    bound = raw[:, 49:57].max(1)
    print("K    estimate       mean error   off by more than 1   largest exact distance")
    for kk in sorted(set(k)):
        p = pred[k == kk]
        print(f"{kk:2d}   {p.mean():5.2f} +- {p.std():4.2f}   {np.abs(p - kk).mean():5.2f}        "
              f"{(np.abs(p - kk) > 1).mean() * 100:5.1f}%              {bound[k == kk].mean():5.2f}")
    print(f"all  mean error {np.abs(pred - k).mean():.3f}, root of the mean square {np.sqrt(((pred - k) ** 2).mean()):.3f}")
    out = os.path.join(args.work, f"{args.name}.bin")
    export(net, out, raw[:64])
    print("written", out)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["train", "eval", "tune"])
    ap.add_argument("--init", default=None)
    ap.add_argument("--take", type=float, default=1.0)
    ap.add_argument("--near", type=int, default=7)
    ap.add_argument("--near-share", type=float, default=0.3)
    ap.add_argument("--bin", default="./target/release/anycube")
    ap.add_argument("--work", required=True)
    ap.add_argument("--name", default="net")
    ap.add_argument("--epochs", type=int, default=20)
    ap.add_argument("--samples", type=int, default=8_000_000)
    ap.add_argument("--batch", type=int, default=8192)
    ap.add_argument("--kmax", type=int, default=16)
    ap.add_argument("--width", type=int, default=512)
    ap.add_argument("--blocks", type=int, default=2)
    ap.add_argument("--lr", type=float, default=2e-3)
    a = ap.parse_args()
    {"train": train, "eval": evaluate, "tune": tune}[a.mode](a)
