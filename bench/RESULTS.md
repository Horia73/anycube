# Benchmark results

Machine: Apple M1 Pro (8 performance cores), 16 GB RAM. Date: 2026-11-09.
Set: `scrambles-1000.txt`, 1,000 scrambles of 100 random moves each (seed 444).
Metric: OBTM. Every solution was applied to the cube model and verified.

## Mean length per stage

| Solver | Phase 1 | Phase 2 | Phase 3 | 3x3x3 | Total | Min | Max |
|---|---|---|---|---|---|---|---|
| TPR (Java, `tpr-results.tsv`) | 6.727 | 5.847 | 12.362 | 19.638 | 44.574 | 40 | 48 |
| cubenxnxn `--n3 1 --probes 500` (`m1-results.tsv`) | 6.666 | 6.021 | 12.323 | 19.828 | 44.838 | 40 | 48 |
| cubenxnxn default (`n3-2000-results.tsv`) | 6.652 | 6.223 | 12.605 | 17.438 | 42.918 | 38 | 47 |

## Time per solve

| Solver | Mean | Median | Max | Cubes timed |
|---|---|---|---|---|
| TPR (Java, after JIT warm-up) | 146 ms | 121 ms | 2.4 s | 1,000 |
| cubenxnxn `--n3 1 --probes 500` | 121 ms | 101 ms | 1.4 s | 1,000 |
| cubenxnxn default (`timing-200.tsv`) | 950 ms | 748 ms | 6.6 s | 200 |

The time column in `n3-2000-results.tsv` is not a timing: that run solved
several cubes at once (`--parallel`).

## Pool size experiments (first 100 cubes)

| Flags | Mean length | Mean time |
|---|---|---|
| `--n3 1 --probes 500` | 44.95 | 0.13 s |
| `--n3 200 --extra3 0 --probes 500` | 43.96 | 0.21 s |
| `--n3 200 --extra3 1 --probes 500` | 43.19 | 0.62 s |
| `--n3 2000 --extra3 1 --probes 500` | 43.15 | 0.76 s |
| `--n3 2000 --extra3 2 --probes 500` | 43.05 | 0.99 s |
| `--n3 2000 --extra3 1 --probes 3000` | 42.81 | 0.98 s |
| `--n3 5000 --extra3 2 --probes 3000` | 42.73 | 1.38 s |

Going from 200 to 2,000 reductions barely helps, while a stronger 3x3x3 search
does: the reductions in the pool are closely related, and the simple two-phase
3x3x3 solver misses the shortest solutions.

## Reproducing

```bash
cargo run --release -- bench bench/scrambles-1000.txt --tpr bench/tpr-results.tsv --parallel
```

## Joint phase 1+2 (2026-11-10)

Phases 1 and 2 searched as one phase with the exact centre table.

How early the first endpoint of phases 1+2 appears (first 8 cubes):

| Search | Mean length of the first endpoint |
|---|---|
| Split phases 1 and 2 | 12.4 |
| Joint phase | 11.1 |

Whole solver, cubes solved one at a time:

| Configuration | Cubes | Mean length | Mean time | Max time |
|---|---|---|---|---|
| Split phases, 1,000 endpoints | first 24 | 42.17 | 1.8 s | 7.7 s |
| Joint, 1,000 endpoints | first 24 | 41.33 | 1.6 s | 4.5 s |
| Joint, 3,000 endpoints | first 100 | 41.05 | 2.0 s | 5.1 s |
| + best reductions get 16x the 3x3x3 effort | first 30 | 40.73 | 3.1 s | 7.6 s |
| + shared across frames, 3,000 endpoints | first 30 | 40.77 | 2.7 s | 7.4 s |
| + 10,000 endpoints, 8 s limit (default) | first 30 | 40.67 | 3.9 s | 6.9 s |
| same, `--time 4000` | first 30 | 40.80 | 2.9 s | 4.2 s |

TPR on the first 100 cubes: 44.50.

Phase lengths of the default on the first 30 cubes: joint phase 12.87,
phase 3 10.77, 3x3x3 17.03.

Options tried and left off (first 30 cubes, against 40.67–40.97 without them):

| Option | Mean length | Mean time |
|---|---|---|
| `--probes 30000` (10x the 3x3x3 effort on everything) | 40.73 | 7.7 s |
| `--extra3 2 --n3 20000 --probes 1000` (one more level of reductions) | 40.80 | 7.9 s |
| `--far-g 7` (longer reductions close to a domino subgroup) | 40.60 | 4.0 s |
| `--exact 400 --exact-cap 15` (exact 3x3x3 search, split phases, 20 cubes) | 41.85 against 42.15 | 12 s |

Runs differ by a few hundredths from one to the next: the parallel searches
stop at a count or a deadline, so which candidates they collect varies.

## Default configuration on all 1,000 cubes (2026-11-10)

File: `default-results.tsv`. Cubes solved one at a time.

| Solver | Joint phase | Phase 3 | 3x3x3 | Total | Min | Max |
|---|---|---|---|---|---|---|
| TPR (phases 1+2: 12.574) | | 12.362 | 19.638 | 44.574 | 40 | 48 |
| cubenxnxn default | 12.775 | 10.895 | 17.000 | 40.670 | 37 | 43 |

Length histogram: 37: 2, 38: 8, 39: 73, 40: 286, 41: 500, 42: 128, 43: 3.

Against TPR on the same cube: 3.90 moves shorter on average, equal on 10
cubes, longer on none.

Time per solve: mean 3.67 s, median 3.30 s, 99th percentile 8.07 s, max
9.07 s. 12 cubes took more than the 8 s soft limit, none more than 10 s.

## Modes and the exact 3x3x3 search (2026-11-10)

| Mode | Cubes | Mean length | Mean time | Max time |
|---|---|---|---|---|
| `--mode fast` | first 100 | 41.82 | 0.34 s | 0.98 s |
| default | all 1,000 | 40.67 | 3.67 s | 9.07 s |
| `--exact 25 --exact-cap 16` | first 24 | 40.54 | 9.8 s | 13.5 s |
| `--exact 100 --exact-cap 16 --time 30000` (`--mode full`) | first 24 | 40.25 | 13.7 s | 23.1 s |

Exact 3x3x3 search, "is there a solution of at most 15 moves?" on random cubes:

| Lower bound | Nodes | Time |
|---|---|---|
| distance to the domino subgroup (`phase1-exact.bin`) | 0.7–2.2 million | 0.5–1.4 s |
| distance to "no twist, no flip, slice edges in place" (`slice-exact.bin`) | 0.08–0.24 million | 0.10–0.25 s |

A full exact scan of the default pool up to 15 moves found nothing better than
the two-phase search on any of the first 8 cubes (pools of 8 to 265
reductions): 15-move 3x3x3 states are too rare for pools this small. Scanning
up to 16 moves is what pays, and that costs about 2 s per reduction.

Dropping endpoints and reductions whose last move the next stage could make
itself changed nothing measurable (40.73 against 40.67 on the first 30 cubes).

## Speed work, 2x2x2 and 3x3x3 (2026-11-10)

Start-up (all tables already on disk): 540 ms before, about 25 ms after
saving the small tables.

4x4x4, cubes solved one at a time:

| Configuration | Cubes | Mean length | Mean time | Max time |
|---|---|---|---|---|
| before, `--time 4000` | first 40 | 40.83 | 3.03 s | 4.1 s |
| after, `--time 4000 --probes 10000` (new default) | first 100 | 40.64 | 2.55 s | 3.8 s |
| after, `--mode fast` | first 100 | 41.54 | 0.36 s | 1.0 s |
| after, 150,000 endpoints, 20 s limit | first 30 | 40.03 | 9.2 s | 12.3 s |
| after, the same plus exact 3x3x3 (`--mode full`) | first 30 | 40.00 | 15.9 s | 22.3 s |

Per-stage time of the old default (30 cubes): joint phase 2.1 s, phase 3
0.35 s, 3x3x3 1.3 s. Checking the 3x3x3 first phase against the exact domino
table brought the 3x3x3 stage down to 0.25 s for the same effort.

Tried and reverted: running phase 3 per endpoint instead of level by level
(worse in fast mode: 42.14 against 41.79), and a near-goal centre table one
move larger (no gain).

Other sizes: a 2x2x2 solve takes 1–3 ms including start-up and is always
optimal. A 3x3x3 two-phase solve takes about 7 ms; an 18-move scramble with a
17-move optimum was solved optimally in 7 s while another job was running.

## New default on all 1,000 cubes (2026-11-10)

File: `default-results.tsv` (replaces the earlier run). Cubes solved one at a time.

| Solver | Joint phase | Phase 3 | 3x3x3 | Total | Min | Max |
|---|---|---|---|---|---|---|
| TPR (phases 1+2: 12.574) | | 12.362 | 19.638 | 44.574 | 40 | 48 |
| cubenxnxn, previous default (8 s limit) | 12.775 | 10.895 | 17.000 | 40.670 | 37 | 43 |
| cubenxnxn, new default (4 s limit) | 12.802 | 10.809 | 16.986 | 40.597 | 37 | 44 |

Length histogram: 37: 1, 38: 13, 39: 89, 40: 311, 41: 467, 42: 112, 43: 4, 44: 3.

Against TPR on the same cube: 3.98 moves shorter on average, equal on 16
cubes, longer on none.

Time per solve: mean 2.47 s (was 3.67 s), median 2.32 s, 99th percentile
3.84 s, max 5.32 s. 6 cubes took more than the 4 s soft limit, 1 more than 5 s.

## 5x5x5: baseline and centre staging (2026-11-10)

Set: `scrambles555-200.txt`, 200 scrambles of 150 random moves (seed 555).

cube555 by Chen Shuang with min2phase (`cube555-results.tsv`); all 200
solutions solve the cube in the sticker model:

| Phase | 1 | 2 | 3 | 4 | 5 | Reduction | 3x3x3 | Total |
|---|---|---|---|---|---|---|---|---|
| Mean length | 10.22 | 8.06 | 6.34 | 12.66 | 14.05 | 51.35 | 19.23 | 70.58 |
| Max | 11 | 10 | 8 | 16 | 17 | 55 | | 75 |

Time: 1.08 s per cube for the reduction (median 0.95 s, max 4.6 s).

Centre staging (every centre on a face of its own axis, wing permutation
even), which is what cube555's phases 1 and 2 do, all 200 cubes, every result
checked on the sticker model:

| Solver | Phase A | Phase B | Total | Mean time | Max time |
|---|---|---|---|---|---|
| cube555, phases 1 + 2 | 10.22 | 8.06 | 18.28 | | |
| cubenxnxn `bench5 --extra 0` | 9.63 | 8.56 | 18.19 | 43 ms | 1.1 s |
| cubenxnxn `bench5 --extra 1 --time 300` | 10.11 | 7.47 | 17.58 | 0.23 s | 0.82 s |
| cubenxnxn `bench5 --extra 1` (no limit) | 10.25 | 7.11 | 17.36 | 0.92 s | 23 s |
| cubenxnxn `bench5 --extra 2 --time 2000` | 10.60 | 6.50 | 17.10 | 1.5 s | 2.1 s |

`--extra` is how many moves past the shortest phase A are searched; each
phase-A solution is completed by an optimal phase B from its exact table.
Two extra moves without a time limit gave 16.85 on the first 20 cubes, at
15 s per cube.

## 5x5x5: complete solver (2026-11-10)

All 200 cubes of `scrambles555-200.txt`, every solution checked on the sticker
model. The machine was busy with other work during these runs (load average
15 to 26 on 8 cores), so the times are upper estimates.

| Solver | Centres | Edges (three phases) | 3x3x3 | Total | Mean time | Max time | Cubes |
|---|---|---|---|---|---|---|---|
| cube555 + min2phase | 10.22 + 8.06 | 6.34 + 12.66 + 14.05 | 19.23 | 70.58 | 1.08 s | 4.6 s | 200 |
| cubenxnxn `--mode fast` | 10.37 + 8.21 | 7.02 + 12.55 + 14.10 | 17.93 | 70.17 | 0.44 s | 2.3 s | 200 |
| cubenxnxn, default | 10.51 + 8.02 | 7.08 + 12.48 + 13.72 | 17.16 | 68.98 | 2.2 s | 4.9 s | 200 |
| cubenxnxn `--mode full` | 10.50 + 7.95 | 7.62 + 12.43 + 13.50 | 16.20 | 68.20 | 8.7 s | 26 s | first 40 |

Cube by cube against cube555 (its mean on the first 40 cubes is 70.85):

| Mode | Shorter | Equal | Longer | Longest |
|---|---|---|---|---|
| fast | 102 | 40 | 58 | 73 |
| default | 151 | 27 | 22 | 72 |
| full (40 cubes) | 36 | 1 | 3 | 70 |

Files: `fast555-results.tsv`, `normal555-results.tsv`, `full555-40.tsv`
(cube, lengths per step, total, milliseconds, solution).

The maximum of the fast mode is the first cube of the run, which reads the
tables into memory; the median is 0.40 s.

What the pieces were worth, on the first 40 cubes:

| Version | Mean length | Mean time |
|---|---|---|
| first complete solver (phase D in two steps, a few starts per phase) | 73.03 | 4.2 s |
| phase P with its exact table, same search | 72.5 (8 cubes) | 38 s |
| + every phase from all results of the one before, six orientations | 68.83 | about 8 s |
| + edge-against-centre tables for phases P and E | 68.83 | about 1 s |
| 20 times wider (`--c-keep 10000 --p-keep 10000 --e-keep 1000 --staged 1000`) | 68.20 | about 10 s |

Reproduce with `cubenxnxn bench5 bench/scrambles555-200.txt [--mode fast|full] [--limit 40]`.

## 5x5x5 after the phase C table was reduced by symmetry (2026-11-10)

Same set and checks; the machine was even busier (load average 12 to 34).
The result files in this folder are from this run.

| Mode | Centres | Edges | 3x3x3 | Total | Mean time | Median | Max | Cubes |
|---|---|---|---|---|---|---|---|---|
| fast | 10.36 + 8.21 | 7.04 + 12.60 + 13.96 | 17.98 | 70.16 | 0.34 s | 0.31 s | 0.74 s | 200 |
| default | 10.54 + 7.90 | 7.22 + 12.42 + 13.56 | 17.18 | 68.82 | 1.4 s | 1.3 s | 3.5 s | 200 |
| full | 10.50 + 7.95 | 7.62 + 12.43 + 13.50 | 16.20 | 68.20 | 5.3 s | 4.9 s | 21 s | first 40 |

Against cube555 cube by cube: default 159 shorter, 20 equal, 21 longer; fast
104 shorter, 43 equal, 53 longer; full (40 cubes) 36, 1, 3.

The phase C table went from 2.77 GB to 175 MB (170,971 classes of wing masks
under 16 symmetries, times 2,048 flips). It agreed with the old table on
20,000,000 random entries before the old one was deleted. The lengths differ
slightly from the previous run because a phase cut short by the clock keeps
whatever it has found by then.

## 5x5x5 centres in one step (2026-11-10)

Exact tables per orbit, all 36 moves, reduced by 16 symmetries:

| Orbit | Classes | Entries | Largest distance | Generation |
|---|---|---|---|---|
| x centres | 46,371 | 596,794,770 | 11 | 50 s |
| t centres | 46,935 | 604,053,450 | 12 | 50 s |

Search bounded by the larger of the two distances, first scramble of the set:
bounds 9 and 9, shortest staging 15 moves (2 of them), 340 s on 8 cores. The
two-phase staging gives 17 for that cube in 0.1 s. Dropping branches when the
moves left are fewer than the larger distance plus a share of the smaller
(6/16 to 10/16) found nothing under 17 in 20 s on any of 8 cubes. One cube is
not a mean; the search is too slow to measure more.

## dwalton76 on our scramble sets (2026-11-10)

His solver at commit of Horia's local copy, run with the lookup tables
already on this machine; every solution checked on the sticker model. Files:
`dwalton444-results.tsv` and so on (cube, length, seconds, solution).

| Size | Cubes | dwalton76 mean length | Mean time | Ours on the same cubes |
|---|---|---|---|---|
| 4x4x4 | first 100 | 51.71 | 1.6 s | 40.6 (default, mean of all 1,000) |
| 5x5x5 | first 40 | 79.78 | 4.6 s | 68.67 (default) |
| 6x6x6 | first 10 | 136.10 | 43 s | not yet |
| 7x7x7 | first 10 | 194.40 | 9.4 s | not yet |

The table in his README (4x4x4 51.6, 5x5x5 78.9, 6x6x6 135.7, 7x7x7 193.8)
is captioned as moves "to reduce the cube to a 3x3x3", with about 20 more
for the 3x3x3. These runs show the numbers are totals: a 52-move 4x4x4
solution of his is 32 moves of reduction and 20 of 3x3x3.

## 6x6x6: centre staging (2026-11-10)

`cubenxnxn stage6 bench/scrambles666-100.txt --limit 20`: every centre of the
four orbits on a face of its own axis, both wing orbits even; every result
checked on the sticker model. dwalton76 spends about 38 moves on the same job
(his own figure for the four phases, from comments in his code).

| Version | Steps | Total | Mean time |
|---|---|---|---|
| six steps, obliques staged orbit by orbit | 6.2 + 10.4 + 9.0 + 5.6 + 10.6 + 8.8 | 50.6 | 2.9 s |
| U/D obliques paired with an exact table instead | 6.3 + 10.6 + 8.9 + 5.8 + 5.1 + 9.9 | 46.6 | 12 s |
| R/L inner x and oblique pairing in one search | 8.8 + 10.0 + 5.5 + 5.5 + 9.3 | 39.2 | 0.46 s |
| U/D inner x, parity and pairing in one search, width 4 | 9.05 + 9.85 + 8.25 + 9.40 | 36.55 | 0.48 s |
| the same, width 32 | 9.05 + 9.45 + 8.10 + 9.05 | 35.65 | 2.2 s |

Staging the R/L inner x centres and both oblique orbits in one search with
the three exact orbit tables as bound did not finish one cube in ten minutes.

## 6x6x6: first complete solver (2026-11-10)

`cubenxnxn bench6 bench/scrambles666-100.txt --width 4`, first 6 cubes (the run
was cut off after ten minutes), every solution checked on the sticker model.
File: `six666-first6.tsv`. dwalton76 on the first 10 cubes of the same set:
136.10 moves, 43 s per cube.

| Cube | Staging | Wing split | Inner reduction | Outer 5x5x5 | Total | Time |
|---|---|---|---|---|---|---|
| 0 | 36 | 5 | 18 | 52 | 111 | 3.6 s |
| 1 | 37 | 7 | 17 | 54 | 115 | 8.3 s |
| 2 | 34 | 7 | 18 | 51 | 110 | 22 s |
| 3 | 35 | 7 | 18 | 54 | 114 | 14 s |
| 4 | 36 | 6 | 21 | 55 | 118 | 239 s |
| 5 | 33 | 7 | 18 | 53 | 111 | 46 s |
| mean | 35.2 | 6.5 | 18.3 | 53.2 | 113.2 | 56 s |

That is 17% below dwalton76, against a target of 20% (108.9 moves), and far
too slow on some cubes.

How it got here, each on a handful of cubes:

| Version | Mean length | Time per cube |
|---|---|---|
| obliques matched to the inner x centres, then our 4x4x4 and 5x5x5 solvers as they are | 118.8 | 67 s, then about 5 s with an additive bound for the matching |
| inner reduction that also pairs the obliques by colour, exact | 111 (one cube; 16 moves for that step) | 2.5 s to more than 10 minutes |
| the same with the two bounds partly added, and a budget | 113.2 | see above |

Things tried that did not pay:

- Splitting the outer wings in the same search as the inner ones: 12 moves
  instead of 5, to save 3 in the first edge phase of the 5x5x5.
- Adding three quarters of the oblique bound to the inner bound everywhere:
  the inner reduction came out at 19 moves instead of 16.
- Staging with the cube held in each of the six ways that put different
  colours on the axes: the same lengths on the cubes tried, for six times
  the staging time.

Step 3 from many starts with a small budget each (`six666-many-starts.tsv`,
24 starts, 50 million positions each, the run cut off after five cubes):
111, 112, 112, 113, 115, mean 112.6, 34 to 170 s per cube. With 6 starts and
100 million positions each: 110, 113, 113, 113, 111, mean 112.0, 42 to 124 s.
The length does not move; this step is about 17 moves whichever way it is
searched, and the whole solve stays near 112.

## 6x6x6: three modes (2026-11-11)

First 20 cubes of `scrambles666-100.txt`, every solution checked on the
sticker model; another benchmark was running on the same machine, so the
times are on the high side. dwalton76: 136.10 moves and 43 s per cube on the
first 10 cubes.

| Mode | Staging | Preparing | Inner reduction | Outer 5x5x5 | Total | Mean time | Max time | Against dwalton76 |
|---|---|---|---|---|---|---|---|---|
| `--mode fast` | 36.20 | 16.00 | 13.15 | 53.65 | 119.00 | 2.8 s | 7.8 s | -12.6% |
| default | 36.05 | 15.85 | 12.80 | 52.25 | 116.95 | 5.6 s | 9.4 s | -14.1% |
| exact inner reduction (what `--mode full` does), first 5 or 6 cubes | 35.2 | 6.5 | 17 to 18 | 53 | 112 to 113 | 40 s to minutes | | -17% |

Files: `fast666-20.tsv`, `normal666-20.tsv`.

Fast and default take the quick way for the inner cube: the obliques are
matched to the inner x centres first (about 11 moves, found in
milliseconds thanks to a bound that adds over the three axes), so that the
inner reduction need not watch them. The exact way pairs the obliques by
colour inside the inner reduction and saves about 5 moves, but its lower
bound is about 10 where the answer is 16 to 18. With a budget of 40 million
positions per staging it finished for 2 cubes in 12 and the mean was 116.75
at 15 s per cube, so the default no longer tries it.

More things that did not pay:

- A bound for the exact inner reduction that joins the inner x centres with
  the obliques of the same axis: the bound stayed at the pairing table's 10.
- Choosing where the inner reduction starts by the sum of its two bounds
  instead of the larger: same lengths, same times.
- Giving the 5x5x5 solver many reduced cubes to choose from instead of one:
  120.8 to 119.2 in the fast mode, less than hoped.
- A beam search for the exact inner reduction (keep the positions that look
  closest after each move): with 20,000 positions per level it found
  nothing; with 200,000 it gave 114.2 moves on 6 cubes at 29 s per cube.
- Choosing among 64 matchings by how easily the inner wings then split:
  117.25 against 116.95 before, on the same 20 cubes; no gain.

## 6x6x6: beam searches (2026-11-11, later)

First 20 cubes of `scrambles666-100.txt`, nothing else running; both files
checked with `cubenxnxn verify --size 6` (20 of 20 each). dwalton76: 136.10
moves on the first 10 cubes; the target is 108.9.

| Mode | Staging | Wing split | Inner reduction | Outer 5x5x5 | Total | Mean time | Max time | Against dwalton76 |
|---|---|---|---|---|---|---|---|---|
| `--mode fast` | 35.25 | 6.10 | 17.20 | 53.20 | 111.75 | 3.1 s | 7.5 s | -17.9% |
| default | 35.10 | 5.60 | 16.95 | 52.40 | 110.05 | 7.4 s | 12.3 s | -19.1% |

Files: `fast666-20.tsv`, `normal666-20.tsv` (they replace the ones of the
section above: 119.00 and 116.95).

What changed, measured in the fast mode on the same 20 cubes:

| Version | Total |
|---|---|
| as in the section above (quick way) | 118.80 |
| steps 3 and 4 of the staging in one beam search (17.25 moves became 15.75) | 117.40 |
| inner reduction with the obliques by beam search (`search::beam`, written for the 7x7x7), 100,000 states per level, 10 cubes | 112.50 against 117.40 |

- The staging step: the U/D inner x centres, the inner wing parity, the U/D
  pairing and phase B of the 5x5x5 are one search with the moves of the old
  step 3. The score adds exact tables: twice the inner x distance, twice the
  pairing distance, and the phase-B distance taken with each oblique orbit
  as the t centres. 20,000 states per level; 200,000 gave 15.55.
- The inner reduction: the score is three times the bound of the inner cube
  plus the bound of the obliques in half moves. The exact search needed up
  to many minutes for 17 to 18 moves; the beam finds 17 in a fraction of a
  second. An earlier beam of mine (states kept in the order of the states
  themselves) found nothing at 20,000 and 114.2 at 200,000 and 29 s; the
  difference is that this one keeps a sample of the states with equal
  scores, by a hash.

Tried on top, no gain (default mode, 20 cubes unless said):

- Matching and wing split of the quick way in one search: exact, it did not
  finish 8 cubes in ten minutes; by beam, 117.25 against 117.40 (fast mode).
- Steps 1 and 2 of the staging in one beam search (the whole R/L axis, sum
  of single-orbit distances): 22.05 moves against 18.85, and nothing found
  at 20,000 states per level. The same failure as on the 7x7x7.
- Staging beam of 100,000: 110.10. 16 stagings, 12 carried on: 110.00 at
  16.7 s. 32 reductions per beam and 256 cubes for the 5x5x5: 109.90; the
  same with no clock and wider phases in the 5x5x5: 109.75.
- Other weights in the reduction score (2:1, 3:2, pairing and centres
  separately): 111.9 to 113.9 against 111.9 to 112.3 for 3:1 and 2:1 (fast
  mode, 10 cubes, 50,000 states).

So the default sits at about 110 whatever is widened; the last move to the
target needs another change of structure.

A fault found on the way, in the 5x5x5 solver: phase P needs four edge
classes with no piece in its slice, and when all the results kept from phase
C were alike and offered none, the solve failed ("no reduction found", cube
18 of the 6x6x6 set in the fast mode, 5 runs in 6). Phase C is now asked for
eight times as many results when that happens.

## 7x7x7: first complete solver (2026-11-11)

Set: `scrambles777-100.txt`, 250 random moves each. Metric: outer block turns
(a turn of one, two or three layers of a face is one move). Every solution
was applied to the scrambled cube on the sticker model and left it solved;
the file below was also checked with `cubenxnxn verify --size 7`. Another
benchmark was running on the same machine and these runs had 4 of its 8
cores (`RAYON_NUM_THREADS=4`), so the times are rough.

dwalton76 on the first 10 cubes: 194.40 moves, 9.4 s per cube. The target
was 20% fewer, 155.5.

| Mode | Staging | Matching | Inner edges | Outer edges | 3x3x3 | Total | Mean time | Max time | Cubes | Against dwalton76 |
|---|---|---|---|---|---|---|---|---|---|---|
| `--mode fast` (beams of 20,000) | 54.20 | 16.40 | 34.40 | 34.60 | 17.50 | 157.10 | 5.7 s | 7.3 s | first 10 | -19.2% |
| default (beams of 100,000) | 53.30 | 16.10 | 34.10 | 34.70 | 16.60 | 154.80 | 11.3 s | 12.4 s | first 10 | -20.4% |
| default | 52.57 | 15.77 | 34.17 | 34.93 | 16.83 | 154.27 | 11.4 s | 13.8 s | first 30 | |
| `--restarts 3` | 53.10 | 15.60 | 34.00 | 34.40 | 17.20 | 154.30 | 30 s | 35 s | first 10 | -20.6% |

Per cube, default: 154, 155, 157, 155, 155, 154, 157, 154, 151, 156 against
195, 198, 191, 196, 192, 193, 194, 195, 199, 191. On 30 cubes the shortest
solution has 151 moves and the longest 158. File: `seven777-results.tsv`,
the 30 cubes of the default (cube, lengths per step, total, milliseconds,
solution).

The pipeline (`src/stage7.rs`, `src/solver7.rs`, `src/search.rs`):

1. Staging, five steps: R/L inner x and t centres onto R and L (phase A of
   the 5x5x5 on the inner cube, layers 1, 3, 4, 5, 7); R/L triples formed
   (oblique, outer t centre, oblique: all three R/L or none); R/L outer x
   centres and triples onto R and L (phase A on the outer cube, layers 1, 2,
   4, 6, 7); U/D inner centres, inner wing parity and U/D triples in one
   search; U/D outer centres and outer wing parity (phase B on the outer
   cube). Mean lengths with beams of 20,000 on 6 cubes: 10.2 + 10.3 + 10.0
   + 15.3 + 7.7 = 53.5.
2. Matching: half turns of two layers make every oblique and outer t centre
   agree in colour with the inner centre beside it.
3. The three edge phases of the 5x5x5 solver on the inner cube, with turns of
   one and three layers: the matched centres come along with the inner ones.
4. The same phases and the 3x3x3 on the outer cube.

Each step keeps several results (8 stagings, up to 4 matchings of each, 16
reduced inner cubes) and the next one continues the ones shortest in total.

### Beam search

Three searches do two jobs that share few moves, and their lower bounds are
far below the answers: forming the R/L triples (bound 5 to 6, answer 11 or
more), U/D inner centres with U/D triples (bound about 10, answer 14 to
16), matching (bound about 9, answer 14 to 18). Iterative deepening:

| Search | Result |
|---|---|
| R/L triples, one cube | no solution of 10 moves or fewer (109 million positions); stopped during depth 11 |
| U/D inner centres and triples | 14 moves in 4 s on one cube; not finished after a minute and a half on the next |
| matching, one cube | no solution of 14 moves or fewer (109 million positions); stopped during depth 15 |

A beam search (`search::beam`: per level, keep the states a score likes
best; ties kept as a sample, by a hash) finds answers of the lengths
iterative deepening could not rule out:

| Beam width | R/L triples (6 starts) | Time per search |
|---|---|---|
| 2,000 | 12 to 13 moves | 0.1 s |
| 20,000 | 11 to 12 | 0.3 s |
| 200,000 | 11 | 2 s |

The scores: for the triples, the sum of two relaxed bounds (obliques
against outer t centres) and the count of places that disagree; for the
U/D step, twice the exact phase-B distance of the inner centres plus three
exact two-orbit distances; for matching, the sum over the axes of exact
per-axis distances with each move's cost shared between the axes.

Computing the scores of a level first and building only the states that
stay took the solve from 23 s to 11 s at the same lengths.

### How it got here

| Version | Cubes | Mean length | Time per cube |
|---|---|---|---|
| first complete: one staging, matching by beam of 20,000 | cube 0 | 164 (55 + 20 + 37 + 34 + 18) | 12 s |
| triples riding along the inner edge phases instead of matching (below) | first 10 | 159.20 | 11.6 s |
| matching again, 4 stagings, 2 matchings each | first 4 | 159.75 | 7 s |
| 8 stagings, 4 matchings each | first 4 | 157.75 | 8 s |
| beams of 100,000 | first 10 | 154.60 | 23.6 s |
| scores first, a quicker hash (same search) | first 10 | 154.80 | 11.3 s |

Staging alone, beams of 20,000, 6 cubes: one candidate 56.67; 4 candidates
carried from step to step 54.33; 8 candidates 53.33 to 53.50 (3.5 s).

### Tried, did not pay

- **Whole axes in one beam search** (all six orbits' R/L centres onto R and
  L with all moves, ranked by the sum of the single-orbit distances; the
  same for U/D): the sum falls from 32 to about 9 in 17 moves and then
  stalls at 3 to 5 for 40 levels without a solution. Near the end the moves
  that finish one orbit undo another, and a sum of single-orbit distances
  does not see it.
- **No matching step; the triples ride along the inner edge phases** as a
  second state with its own exact tables, each face half turn of the inner
  cube made with one layer or with two. Three variants:
  - one axis per phase (stripes on the axis a phase turns by quarters, which
    later half turns leave alone; one colour per triple on the last axis).
    Exact, by iterative deepening: phase C with its axis 13 moves in 17 s
    (alone 7; the triples alone 8.5), phase E 18 moves in 4 s (its own
    bound 12), phase P not finished in minutes. By beam of 100,000: 12 to 14, 17
    to 23 and 19 to 25 moves. Whole solve 159.20 on 10 cubes, inner cube
    53.9 moves against 50.2 for matching plus plain phases.
  - all three axes left to the last phase, the earlier ones only making
    that possible: inner cube 51 and 56 moves on two cubes.
  - both side by side with matching, the best reduction of either going on:
    the riders never won on 4 cubes.
  The beam was also fragile here: with other orders of equal states it
  found nothing in 40 levels on some cubes, because states with one job
  done and the other one move away fill the beam and cannot be finished. A
  penalty for such states and a limit on the share of one score helped on
  some cubes and hurt on others.
- **Choosing a staging step's results by a quick narrow search of the next
  step**: 53.83 in 9 s against 53.50 in 3.5 s without.
- **More reduced inner cubes for the outer cube (32), no time limit in the
  outer cube, wider inner edge phases (2,000 results per phase)**: the same
  lengths to the move on 4 cubes.
- **Three runs of every beam with ties broken differently**: 154.30
  against 154.80, for three times the time. **Beams of 300,000**: 154.70,
  28 s per cube. The lengths stop falling at about 100,000.

### What remains weak

- Staging is 53 moves, a third of the solve. A joint search of an axis
  would need a score that sees how the orbits hinder each other.
- Matching is 16 moves of pure overhead. The riders show that the edge
  phases cannot absorb it cheaply (the two jobs share few moves), and its
  beam is at most a move or two above what iterative deepening rules out.
- The lengths move by a move or two with the order of equal states in the
  beams, so the mean of 10 cubes is good to about half a move.

Reproduce with `cubenxnxn bench7 bench/scrambles777-100.txt --limit 10
[--mode fast] [--restarts 3] [--out file]`; tables are generated on first
use (`stage777-*.bin`, about 300 MB).

### Checked again after the merge (2026-11-11)

Run from `shorter-solutions` with all eight cores and nothing else running:

| Mode | Cubes | Mean | Mean time | Max time |
|---|---|---|---|---|
| default | first 10 | 154.80 (the same solutions' lengths as above) | 5.7 s | 6.4 s |
| default | cubes 31 to 50, not used while the solver was written | 155.00 | 6.2 s | 8.1 s |
| `--mode fast` | cubes 31 to 50 | 155.60 | 3.3 s | 4.7 s |

Both default runs were written out as scramble and solution and checked
with `cubenxnxn verify --size 7`: 30 of 30 solved. With the 30 cubes above,
the default is at 154.56 on the first 50 cubes, 20.5% below dwalton76's
194.40 (measured on the first 10 only).

