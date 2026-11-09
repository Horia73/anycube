# cube444

A 4x4x4 Rubik's cube solver in Rust that aims for shorter solutions than the
existing reduction solvers, within a few seconds per cube.

Move counts are in OBTM: any face turn or wide turn counts as one move.

## Status

Work in progress. On the fixed set of 1,000 scrambles in
`bench/scrambles-1000.txt` (Apple M1 Pro):

| Solver | Mean length | Mean time |
|---|---|---|
| TPR (Java) | 44.57 | 146 ms |
| cube444, fast mode | 44.84 | 121 ms |
| cube444, default | 42.92 | 0.95 s |

Details are in `bench/RESULTS.md`.

## How it works

The phase structure follows Chen Shuang's
[TPR solver](https://github.com/cs0x7f/TPR-4x4x4-Solver) (itself based on
Tsai's 8-step method):

1. Bring the R/L-coloured centres onto the R and L faces (all moves).
2. Separate the U/D centres from the F/B centres, bring the R/L centres into a
   finishable pattern and make the wing permutation even.
3. Pair the 12 dedges and solve the centres.
4. Solve the reduced cube as a 3x3x3 (Kociemba's two-phase algorithm).

Where it differs from TPR:

- Phase 3 does not stop at its first solution. It collects many reductions and
  the 3x3x3 stage picks the one with the shortest total.
- The 24 target orientations and 3 axes are handled as explicit variants of
  the cube instead of symmetry-reduced coordinates.
- The dedge pairing table is stored unreduced (240 MB) with exact distances.

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

The first run generates `tables/pairing.bin` (240 MB) and
`tables/phase1-exact.bin` (554 MB), about 20 s each on 8 cores.
Set `CUBE444_TABLES` to keep the tables somewhere else.

Search effort is set with flags. `--n2 100 --k3 100 --n3 1 --probes 500` gives
the fast mode that mirrors TPR's search.

| Flag | Meaning | Default |
|---|---|---|
| `--n1` | phase-1 solutions enumerated | 10000 |
| `--k1` | phase-1 endpoints kept | 500 |
| `--n2` | phase-2 endpoints collected | 1000 |
| `--k3` | phase-2 endpoints tried in phase 3 | 1000 |
| `--n3` | reductions compared by total length | 2000 |
| `--extra3` | moves past the shortest reduction to keep collecting | 1 |
| `--probes` | 3x3x3 two-phase effort per reduction | 3000 |
| `--exact` | node budget (millions) of the exact 3x3x3 search | 0 (off) |
| `--exact-cap` | longest 3x3x3 solution the exact search looks for | 15 |

The exact 3x3x3 search (`src/opt3.rs`) uses a 554 MB table of exact distances
to the domino subgroup, generated on first use in about 20 s. It is off by
default: at about 1 s per 15-move query it costs more than it gains.

Every solution is applied to the full cube model and checked before it is
returned. The model itself is cross-checked against TPR in the benchmark: same
facelets for every scramble, and TPR's solutions solve the cube in this model.

## Credits

Piece numbering, move definitions and the phase design come from TPR by Chen
Shuang (MIT licence).
