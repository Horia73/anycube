# cubenxnxn

Formerly `cube444`; renamed when it grew past the 4x4x4.

A Rubik's cube solver in Rust for 2x2x2 to 5x5x5. The 4x4x4 and 5x5x5 solvers
aim for shorter solutions than the existing reduction solvers, within a few
seconds per cube.

Move counts are in OBTM: any face turn or wide turn counts as one move.

## Status

Work in progress. 4x4x4 results on the fixed scrambles in
`bench/scrambles-1000.txt` (Apple M1 Pro); details in `bench/RESULTS.md`.

| Solver | Mean length | Mean time | Cubes |
|---|---|---|---|
| TPR (Java) | 44.57 | 146 ms | 1,000 |
| cubenxnxn `--mode fast` | 41.54 | 0.36 s | first 100 |
| cubenxnxn, default | 40.60 | 2.5 s | 1,000 |
| cubenxnxn `--mode full` | 40.00 | 15.9 s | first 30 |

With the defaults no solution is longer than TPR's for the same cube, the
longest is 44 moves and the slowest solve took 5.3 s. Start-up takes about
25 ms once the tables exist.

| Size | What it does | Time |
|---|---|---|
| 2x2x2 | always optimal | about 2 ms |
| 3x3x3 | two-phase search; `--mode full` searches for the optimal solution | about 10 ms; seconds to minutes for optimal |
| 4x4x4 | see above | |
| 5x5x5 | see below | |
| 6x6x6, 7x7x7 | scrambles and checking only (`src/cuben.rs`) | |

5x5x5 results on the fixed scrambles in `bench/scrambles555-200.txt`, measured
on the same machine while it was busy with other work, so the times are on the
high side:

| Solver | Mean length | Mean time | Longest | Cubes |
|---|---|---|---|---|
| cube555 (Java) with min2phase | 70.58 | 1.1 s | 75 | 200 |
| cubenxnxn `--mode fast` | 70.16 | 0.34 s | 74 | 200 |
| cubenxnxn, default | 68.82 | 1.4 s | 72 | 200 |
| cubenxnxn `--mode full` | 68.20 | 5.3 s | 70 | first 40 |

With the defaults 159 of the 200 solutions are shorter than cube555's, 20 are
equal and 21 are longer; the slowest solve took 3.5 s, and 0.74 s in the fast
mode. The times assume the tables are in memory; the first solve of a process
pays for reading them.

## How the 5x5x5 solver works

- Centres (`src/stage5.rs`): the R/L centres go to the R and L faces with all
  36 moves, then an exact table puts the U/D and F/B centres on their axes.
  Staging all centres in one step (`src/centres5.rs`) is exact and shorter,
  15 moves against 17 on the one cube measured, but took 340 s for it.
- Edges (`src/edges5.rs`), three phases:
  - C: every pair of wings gets one wing in each half of the wing positions,
    matching the flip of its middle edge (face turns and wide half turns);
  - P: four whole edges are built in the middle slice (F and B freely, half
    turns elsewhere), with an exact table for the twelve pieces involved and
    tables of those pieces against the centres;
  - E: the other eight edges and all centres (U and D freely, half turns
    elsewhere), with an exact pairing table.
- The 3x3x3 that is left is solved for many reduced cubes; the shortest total
  wins.
- Each step starts from every result of the step before and keeps the
  continuations that are shortest in total (`src/solver5.rs`). The edge phases
  see the cube from six sides: three choices of axis, two of the slice.

## How the 4x4x4 solver works

The phase structure started from Chen Shuang's
[TPR solver](https://github.com/cs0x7f/TPR-4x4x4-Solver) (itself based on
Tsai's 8-step method) and now has three stages:

1. **Joint phase** (all moves): stage every centre on a face of its own axis,
   bring the R/L centres into a pattern the next phase can finish, make the
   wing permutation even and the dedges pairable. This replaces TPR's phases
   1 and 2. An exact table over all 9.47 billion centre states makes the centre
   part of the search perfect. The last move is tested against the wings
   first, which rejects almost all of them without looking at the centres.
2. **Phase 3** (`U D F B R2 L2 Uw2 Rw2 Fw2`): pair the 12 dedges and solve the
   centres, as in TPR, with an exact table of the pairing permutation.
3. **3x3x3**: Kociemba's two-phase algorithm on the reduced cube, with an exact
   table for its first phase.

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
./target/release/cubenxnxn solve "Rw U2 F' Uw L2 D Bw' R"
```

```bash
./target/release/cubenxnxn solve --size 3 "R U R' F2 D L B' U2" --mode full
```

```bash
./target/release/cubenxnxn solve --size 2 "R U R' F2 D L B' U2"
```

```bash
./target/release/cubenxnxn solve --size 5 "Rw U2 F' Uw L2 D Bw' R" --mode fast
```

```bash
./target/release/cubenxnxn solve --state <24, 54, 96 or 150 facelets in URFDLB order>
```

```bash
./target/release/cubenxnxn bench5 bench/scrambles555-200.txt
```

```bash
./target/release/cubenxnxn bench bench/scrambles-1000.txt --tpr bench/tpr-results.tsv
```

```bash
./target/release/cubenxnxn facelets --size 5 "Uw R' F2"
```

### Tables

Generated on first use into `tables/` (set `CUBENXNXN_TABLES` to move them),
4.9 GB in all. The large ones take about four minutes on 8 cores, once.

| File | Size | Time on 8 cores | Contents |
|---|---|---|---|
| `centres-exact.bin` | 2.37 GB | 105 s | moves to stage all centres, every centre state |
| `slice-exact.bin` | 1.72 GB | 55 s | 3x3x3 distance to "no twist, no flip, slice edges in place" |
| `phase1-exact.bin` | 554 MB | 20 s | 3x3x3 distance to the domino subgroup |
| `pairing.bin` | 240 MB | 20 s | moves to pair the dedges in phase 3 |
| nine small files | 70 MB | under 1 s | everything else, so that start-up reads instead of rebuilding |

The 2x2x2 solver needs only `cube222.bin` (3.7 MB) and the 3x3x3 solver only
`cube333.bin` (10 MB), plus `slice-exact.bin` for `--mode full`.

The 5x5x5 solver has its own tables, 2.1 GB, generated in about two minutes on
8 cores: `phase4-555-*.bin` (850 MB, phase P), `phase5-555*.bin` (600 MB,
phase E), `phase2-555.bin` (331 MB, centres), `phase3-555-sym.bin` (175 MB,
phase C, one entry per class under its 16 symmetries) and four small ones. It
also uses `phase1-exact.bin` for the 3x3x3 step. `--joint` needs two more,
`centres555-x.bin` and `centres555-t.bin` (300 MB each, 100 s to generate).

### Search flags (5x5x5)

`--mode fast|normal|full` sets all of them; each can be overridden.

| Flag | fast | normal | full | Meaning |
|---|---|---|---|---|
| `--time` | 800 | 4000 | 0 | milliseconds the solve aims at; 0 is no limit |
| `--staged` | 200 | 400 | 2000 | centre stagings kept |
| `--labellings` | 4 | 8 | 16 | ways of splitting the wings tried per staging |
| `--c-keep`, `--p-keep` | 300 | 2000 | 5000 | results phases C and P aim for |
| `--e-keep` | 50 | 200 | 1000 | reduced cubes aimed for |
| `--pool`, `--probes` | 16, 500 | 60, 3000 | 400, 20000 | reduced cubes given the full 3x3x3 effort, and that effort |
| `--joint N` | off | off | off | also stage the centres in one step, exactly, N lengths past the shortest; minutes per cube |

### Search flags (4x4x4)

| Flag | Meaning | Default |
|---|---|---|
| `--mode` | `fast`, `normal` or `full`; sets the flags below | normal |
| `--time` | soft time limit per cube, in ms | 4000 |
| `--n2` | joint-phase endpoints collected | 10000 |
| `--joint-nodes` | node budget of the joint phase, in millions | 1000 |
| `--k3` | endpoints tried in phase 3 | 10000 |
| `--n3` | reductions compared by total length | 2000 |
| `--extra3` | moves past the shortest reduction to keep collecting | 1 |
| `--probes` | 3x3x3 two-phase effort per reduction | 10000 |
| `--refine` | effort multiplier for the best sixteenth of the reductions | 16 |
| `--split` | search phases 1 and 2 separately, as TPR does | off |
| `--n1`, `--k1` | with `--split`: phase-1 solutions enumerated and kept | 10000, 500 |
| `--exact` | node budget (millions) of the exact 3x3x3 search | 0 (off) |
| `--exact-cap` | longest 3x3x3 solution the exact search looks for | 15 |
| `--far-g` | also keep longer reductions this close to a domino subgroup | 0 (off) |

`--mode full` collects fifteen times as many endpoints and adds the exact
3x3x3 search (`src/opt3.rs`), which scans the reductions for 3x3x3 solutions
of up to 16 moves that the two-phase search missed.

## Verification

Every solution is applied to the full cube model and checked before it is
returned. Beyond that:

- The 4x4x4 model gives the same facelets as TPR for every benchmark
  scramble, and TPR's solutions solve the cube in this model.
- The any-size sticker model (`src/cuben.rs`), built from geometry alone,
  agrees with the 4x4x4 model move for move.
- The 2x2x2 table reproduces the known distance distribution (maximum 11).
- The 3x3x3 domino table reproduces Kociemba's published distribution.
- The shared joint search finds the same endpoints as an unshared one.

## Credits

All code here is written for this project; nothing is copied from other
solvers. The ideas are not all ours:

- 4x4x4: piece numbering, move definitions and the design of phase 3 come
  from TPR by Chen Shuang (MIT licence).
- 5x5x5: the split into phases follows cube555 by Chen Shuang.
- 3x3x3: the two-phase algorithm and the exact search bounded by distances
  to a subgroup are Herbert Kociemba's.
