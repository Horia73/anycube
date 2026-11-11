"""Experiment: a network that estimates how many moves a 4x4x4 is away from a reduced cube.

Data comes from `anycube free-dump`: 52-byte records (24 centres, 24 partner positions,
parity bits, K = length of the walk away from a reduced cube, exact distance if at most 6,
hand score). The network sees only what decides the reductions of a cube:

- for every pair of centre positions, whether the two colours are the same and whether
  they are opposite (so the input does not depend on how the cube is held);
- for every wing position, where the other half of its dedge is;
- the parity of the wing permutation.

Usage:
  python ml/reduce_net.py train --bin ./target/release/anycube --work DIR [--epochs 30]
  python ml/reduce_net.py eval --work DIR
"""
import argparse, os, subprocess, time
import numpy as np
import torch
import torch.nn as nn

REC = 52
PAIRS = np.array([(i, j) for i in range(24) for j in range(i + 1, 24)])
N_IN = 2 * len(PAIRS) + 24 * 24 + 1


def device():
    return torch.device("mps" if torch.backends.mps.is_available() else "cuda" if torch.cuda.is_available() else "cpu")


def load(path):
    raw = np.fromfile(path, dtype=np.uint8).reshape(-1, REC)
    return raw


def features(ct, partner, wing_parity):
    """ct, partner: (B, 24) long tensors; wing_parity: (B,) -> (B, N_IN) float."""
    i = torch.as_tensor(PAIRS[:, 0], device=ct.device)
    j = torch.as_tensor(PAIRS[:, 1], device=ct.device)
    a, b = ct[:, i], ct[:, j]
    same = (a == b).float()
    opposite = ((a + 3) % 6 == b).float()
    onehot = torch.zeros(ct.shape[0], 24, 24, device=ct.device)
    onehot.scatter_(2, partner.unsqueeze(2), 1.0)
    return torch.cat([same, opposite, onehot.flatten(1), wing_parity.float().unsqueeze(1)], dim=1)


class Block(nn.Module):
    def __init__(self, width):
        super().__init__()
        self.a, self.b = nn.Linear(width, width), nn.Linear(width, width)
        self.na, self.nb = nn.BatchNorm1d(width), nn.BatchNorm1d(width)

    def forward(self, x):
        y = torch.relu(self.na(self.a(x)))
        return torch.relu(x + self.nb(self.b(y)))


class Net(nn.Module):
    def __init__(self, width=768, blocks=3):
        super().__init__()
        self.inp = nn.Sequential(nn.Linear(N_IN, width), nn.BatchNorm1d(width), nn.ReLU())
        self.blocks = nn.Sequential(*[Block(width) for _ in range(blocks)])
        self.out = nn.Linear(width, 1)

    def forward(self, x):
        return self.out(self.blocks(self.inp(x))).squeeze(1)


def batch_tensors(raw, dev):
    t = torch.as_tensor(raw, device=dev).long()
    ct, partner, bits = t[:, :24], t[:, 24:48], t[:, 48]
    k, exact = t[:, 49], t[:, 50]
    label = torch.where(exact != 255, exact, k).float()
    return features(ct, partner, bits & 1), label


def train(args):
    dev = device()
    net = Net(args.width, args.blocks).to(dev)
    print("parameters", sum(p.numel() for p in net.parameters()), "device", dev, flush=True)
    opt = torch.optim.AdamW(net.parameters(), lr=args.lr, weight_decay=1e-5)
    steps_per_epoch = args.samples // args.batch
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=args.lr, total_steps=args.epochs * steps_per_epoch)
    path = os.path.join(args.work, "epoch.bin")
    t0 = time.time()
    for epoch in range(args.epochs):
        # Fresh cubes every epoch: the generator makes millions per second.
        subprocess.run([args.bin, "free-dump", "--count", str(args.samples), "--kmin", "1", "--kmax", str(args.kmax),
                        "--seed", str(1000 + epoch), "--out", path], check=True, stderr=subprocess.DEVNULL)
        raw = load(path)
        net.train()
        total = 0.0
        for s in range(steps_per_epoch):
            x, y = batch_tensors(raw[s * args.batch:(s + 1) * args.batch], dev)
            loss = nn.functional.mse_loss(net(x), y)
            opt.zero_grad(set_to_none=True)
            loss.backward()
            opt.step()
            sched.step()
            total += loss.item() if s % 50 == 0 else 0.0
        print(f"epoch {epoch} loss {total / (steps_per_epoch / 50):.3f} time {time.time() - t0:.0f} s", flush=True)
        torch.save({"state": net.state_dict(), "width": args.width, "blocks": args.blocks},
                   os.path.join(args.work, "net.pt"))
    evaluate(args)


def predict(net, raw, dev, batch=65536):
    out = []
    net.eval()
    with torch.no_grad():
        for s in range(0, len(raw), batch):
            x, _ = batch_tensors(raw[s:s + batch], dev)
            out.append(net(x).cpu())
    return torch.cat(out).numpy()


def evaluate(args):
    dev = device()
    ck = torch.load(os.path.join(args.work, "net.pt"), map_location=dev)
    net = Net(ck["width"], ck["blocks"]).to(dev)
    net.load_state_dict(ck["state"])
    raw = load(os.path.join(args.work, "eval.bin"))
    far = load(os.path.join(args.work, "far.bin"))
    pred, k, hand = predict(net, raw, dev), raw[:, 49], raw[:, 51].astype(float)
    far_pred, far_hand = np.sort(predict(net, far, dev)), np.sort(far[:, 51].astype(float))
    print(f"random cubes: prediction {far_pred.mean():.2f} +- {far_pred.std():.2f}")
    print("share of random cubes that look at least as close as the median (and the 90th percentile) cube K moves away")
    print("K    prediction     network: median  90th      hand score: median  90th")
    for kk in sorted(set(k)):
        if kk > 30:
            continue
        p, h = pred[k == kk], hand[k == kk]
        row = [np.searchsorted(far_pred, np.quantile(p, q), side="right") / len(far_pred) for q in (0.5, 0.9)]
        row += [np.searchsorted(far_hand, np.quantile(h, q), side="right") / len(far_hand) for q in (0.5, 0.9)]
        print(f"{kk:2d}   {p.mean():5.2f} +- {p.std():4.2f}   {row[0]:9.2e} {row[1]:9.2e}        {row[2]:9.2e} {row[3]:9.2e}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["train", "eval"])
    ap.add_argument("--bin", default="./target/release/anycube")
    ap.add_argument("--work", required=True)
    ap.add_argument("--epochs", type=int, default=30)
    ap.add_argument("--samples", type=int, default=8_000_000)
    ap.add_argument("--batch", type=int, default=8192)
    ap.add_argument("--kmax", type=int, default=26)
    ap.add_argument("--width", type=int, default=768)
    ap.add_argument("--blocks", type=int, default=3)
    ap.add_argument("--lr", type=float, default=2e-3)
    a = ap.parse_args()
    train(a) if a.mode == "train" else evaluate(a)
