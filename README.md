# cube444

A 4x4x4 Rubik's cube solver in Rust that aims for shorter solutions than the
existing reduction solvers, within a few seconds per cube.

Move counts are in OBTM: any face turn or wide turn counts as one move.

## Status

Work in progress. Measured on the fixed scrambles in
`bench/scrambles-1000.txt` (Apple M1 Pro); details in `bench/RESULTS.md`.
With the defaults no solution is longer than TPR's for the same cube, the
longest is 43 moves and the slowest solve took 9.1 s.

| Solver | Mean length | Mean time | Cubes |
|---|---|---|---|
| TPR (Java) | 44.57 | 146 ms | 1,000 |
| cube444, phases as in TPR, fast | 44.84 | 121 ms | 1,000 |
| cube444, phases as in TPR, many reductions compared | 42.92 | 0.95 s | 1,000 |
| cube444 `--mode fast` | 41.82 | 0.34 s | first 100 |
| cube444, default | 40.67 | 3.7 s | 1,000 |
| cube444 `--mode full` | 40.25 | 13.7 s | first 24 |

## How it works

The phase structure started from Chen Shuang's
[TPR solver](https://github.com/cs0x7f/TPR-4x4x4-Solver) (itself based on
Tsai's 8-step method) and now has three stages:

1. **Joint phase** (all moves): stage every centre on a face of its own axis,
   bring the R/L centres into a pattern the next phase can finish, make the
   wing permutation even and the dedges pairable. This replaces TPR's phases
   1 and 2. An exact table over all 9.47 billion centre states makes the centre
   part of the search perfect; the other conditions are tested at the end.
2. **Phase 3** (`U D F B R2 L2 Uw2 Rw2 Fw2`): pair the 12 dedges and solve the
   centres, as in TPR, with an exact table of the pairing permutation.
3. **3x3x3**: Kociemba's two-phase algorithm on the reduced cube.

What makes the solutions shorter than TPR's:

- Phases 1 and 2 are searched as one phase (about 1.2 moves).
- The solver collects thousands of joint-phase endpoints and many reductions,
  and the 3x3x3 stage picks the reduction with the shortest total (about 2
  moves). The most promising reductions get a much deeper 3x3x3 search.
- The cube has no fixed centres, so it may end in any of 24 orientations and
  the phases may run along any of 3 axes. All 18 distinct combinations are
  tried; one search serves the three axes.

## Usage

```bash
cargo build --release
```

```bash
./target/release/cube444 solve "Rw U2 F' Uw L2 D Bw' R"
```

```bash
./target/release/cube444 solve --state <96 facelets in URFDLB order>
```

```bash
./target/release/cube444 bench bench/scrambles-1000.txt --tpr bench/tpr-results.tsv
```

### Tables

Generated on first use into `tables/` (set `CUBE444_TABLES` to move them):

| File | Size | Time on 8 cores | Contents |
|---|---|---|---|
| `centres-exact.bin` | 2.37 GB | 105 s | moves to stage all centres, every centre state |
| `pairing.bin` | 240 MB | 20 s | moves to pair the dedges in phase 3 |
| `slice-exact.bin` | 1.72 GB | 55 s | 3x3x3 distance to "no twist, no flip, slice edges in place" |
| `phase1-exact.bin` | 554 MB | 20 s | 3x3x3 distance to the domino subgroup |

### Search flags

| Flag | Meaning | Default |
|---|---|---|
| `--mode` | `fast`, `normal` or `full`; sets the flags below | normal |
| `--time` | soft time limit per cube, in ms | 8000 |
| `--n2` | joint-phase endpoints collected | 10000 |
| `--joint-nodes` | node budget of the joint phase, in millions | 1000 |
| `--k3` | endpoints tried in phase 3 | 10000 |
| `--n3` | reductions compared by total length | 2000 |
| `--extra3` | moves past the shortest reduction to keep collecting | 1 |
| `--probes` | 3x3x3 two-phase effort per reduction | 3000 |
| `--refine` | effort multiplier for the best sixteenth of the reductions | 16 |
| `--split` | search phases 1 and 2 separately, as TPR does | off |
| `--n1`, `--k1` | with `--split`: phase-1 solutions enumerated and kept | 10000, 500 |
| `--exact` | node budget (millions) of the exact 3x3x3 search | 0 (off) |
| `--exact-cap` | longest 3x3x3 solution the exact search looks for | 15 |
| `--far-g` | also keep longer reductions this close to a domino subgroup | 0 (off) |

`--split --n2 100 --k3 100 --n3 1 --probes 500 --refine 1` mirrors TPR's search.

`--mode full` adds the exact 3x3x3 search (`src/opt3.rs`): it scans the
reductions for 3x3x3 solutions of up to 16 moves that the two-phase search
missed, at about 0.15 s per 15-move query. The "far" level of longer
reductions is off because it did not pay for itself in testing.

## Verification

Every solution is applied to the full cube model and checked before it is
returned. The model itself is cross-checked against TPR in the benchmark: same
facelets for every scramble, and TPR's solutions solve the cube in this model.
The 3x3x3 table reproduces Kociemba's published distance distribution, and the
shared joint search finds the same endpoints as an unshared one.

## Credits

Piece numbering, move definitions and the design of phase 3 come from TPR by
Chen Shuang (MIT licence).
