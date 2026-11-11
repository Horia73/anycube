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

The files of these two runs were replaced by the ones of "6x6x6: modes as
they stand" below.

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

Changes of structure tried after that, none kept:

- **The wing split inside the last staging step** (one beam search over the
  staging state, the wings of a labelling and the oblique patterns of the
  R/L axis; 16 labellings as starting states). With the split weighing as
  much as a move of staging the beam finishes the split first and then
  cannot move: every staging move undoes as much split as it gains (all
  states at the same score for twenty levels, nothing found). With half
  that weight it finds 38.2 to 39.5 moves of staging and 3.2 to 4.2 of
  split left over, against 35.8 and 5.8 apart: the same sum.
- **More results of step 1 taken into step 2** (64 or 256 instead of 8):
  step 2 falls from 9.70 to 9.20 and step 3 rises from 15.80 to 16.25;
  34.5 moves either way.
- **Choosing among many stagings by the split that follows** (the beam made
  to return every way into the solved state, and the ones of the next
  level; 16 to 24 stagings with split bounds of 3 to 5): 111.90 and 112.30
  against 111.60, fast mode, 20 cubes.
- **One beam for all candidates of a cube** (12 starting states with their
  costs in the score, solutions collected over three levels): two to three
  times quicker, but 109, 113, 109 against 107, 110, 107 on three cubes.
  The reductions come out longer (a beam of 100,000 shared by twelve cubes
  is narrow for each) and the 5x5x5 solver has fewer cubes to choose from.

What does help a little is the far end. The 3x3x3 left over takes 18 moves
on average here against 16.5 when the 5x5x5 solver runs on a 5x5x5 of its
own. More reductions and more 3x3x3 searches (default mode, 20 cubes):

| 5x5x5 settings | Total | Outer 5x5x5 | Mean time |
|---|---|---|---|
| default | 110.05 | 52.40 | 7.4 s |
| `--pool 240 --e-keep 800 --time 8000` | 109.55 | 51.85 | 10.0 s |
| the same with `--c-keep 4000 --p-keep 4000 --probes 10000 --time 0` | 109.20 | 51.45 | 10.5 s |
| `--pool 800 --e-keep 3000 --c-keep 6000 --p-keep 6000 --probes 10000 --time 0 --finals 128` | 109.35 | 51.35 | 15.1 s |

A fault found on the way, in the 5x5x5 solver: phase P needs four edge
classes with no piece in its slice, and when all the results kept from phase
C were alike and offered none, the solve failed ("no reduction found", cube
18 of the 6x6x6 set in the fast mode, 5 runs in 6). Phase C is now asked for
eight times as many results when that happens.

## 6x6x6: modes as they stand (2026-11-11, night)

First 20 cubes of `scrambles666-100.txt`; every file checked with
`cubenxnxn verify --size 6` (20 of 20 each). dwalton76: 136.10 moves and
43 s per cube on the first 10 cubes; the target is 108.9.

| Mode | Staging | Wing split | Inner reduction | Outer 5x5x5 | Total | First 10 cubes | Mean time | Max time | Against dwalton76 |
|---|---|---|---|---|---|---|---|---|---|
| `--mode fast` | 35.80 | 5.95 | 17.35 | 52.90 | 112.00 | 112.50 | 4.1 s | 8.1 s | -17.7% |
| default | 35.00 | 5.65 | 16.80 | 51.75 | 109.20 | 109.30 | 9.7 s | 15.8 s | -19.8% |
| `--mode full` | 34.80 | 5.85 | 16.45 | 51.75 | 108.85 | 108.60 | 52 s | 103 s | -20.0% |

Files: `fast666-20.tsv`, `normal666-20.tsv`, `full666-20.tsv`.

- The fast and default runs had the machine to themselves. The full run
  shared it with two other benchmarks, so its times are too high; its
  search has no clock, so its lengths do not depend on that.
- The fast mode moves by about half a move from run to run (its 5x5x5 part
  works against a clock): 111.40, 111.85 and 112.00 on the same 20 cubes.
- What the modes are: fast carries 4 stagings and takes 3 on, with beams
  of 50,000; default 8 and 6 with beams of 100,000 and the wider 5x5x5 end
  of the table above; full 16 and 12 with beams of 300,000 and a 5x5x5 end
  wider still.
- Narrower than fast (`--mode fast --width 2 --tries 1 --beam 20000`):
  114.65 moves at 1.5 s per cube. Nothing here gets under a second.

Checked after these changes: the 5x5x5 default on the first 40 cubes of its
set gives 68.675, the same as before; the 7x7x7 default on its first 10
cubes gives 154.80, the same as before.

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

## Any size: the general solver (2026-11-11)

`cubenxnxn benchn bench/scrambles888-20.txt --size 8` and the same for 9
and 10: 20 scrambles each of 40 random moves per layer of size (seeds 888,
999, 1110). Every solution was applied on the sticker model, and the result
files checked again with `cubenxnxn verify`. dwalton76 ran on the first 5
cubes of each set (`dwalton888-results.tsv` and neighbours; all verified).

| Size | Centres | Edges | 3x3x3 | Total | First 5 cubes | dwalton76, those 5 | Mean time | Max time | dwalton76 time |
|---|---|---|---|---|---|---|---|---|---|
| 8x8x8 | 198.05 | 86.40 | 17.45 | 301.90 | 301.00 | 308.40 | 5.6 s | 9.6 s | 15.8 s |
| 9x9x9 | 248.35 | 102.40 | 17.80 | 368.55 | 371.00 | 368.00 | 3.5 s | 4.3 s | 11.0 s |
| 10x10x10 | 393.00 | 120.70 | 17.30 | 531.00 | 534.60 | 523.40 | 7.3 s | 13.3 s | 24.8 s |

Files: `general888-20.tsv`, `general999-20.tsv`, `general101010-20.tsv`.

Other sizes, three cubes each, all verified: 11x11x11 615.0 moves, 12x12x12
831.3, 14x14x14 1,197.3. The same code on the sizes that have solvers of
their own: 6x6x6 146.0 (own solver 109.2), 7x7x7 190.0 (own solver 154.6).

What a step costs (10x10x10, one cube):

| Step | Moves |
|---|---|
| innermost centres of an even cube, as a 4x4x4 | 24 |
| a ring's first two orbits with its corner, as a 6x6x6: R/L, other axes, home | 16 + 15 + 21 |
| two more oblique orbits, as a 7x7x7: R/L, other axes, home | 21 + 22 + 32 |
| innermost wings, as a 4x4x4 | 17 |
| one orbit of wings, as a 5x5x5 | 35 |
| the last orbit of wings with the 3x3x3 | 34 + 17 |

The way home is the weak part. With two oblique orbits out of place and
four orbits to leave where they are, the lower bound alone is 15 to 17
moves; an exact search did not finish within 3 billion positions, and the
beam search finds 30 to 34. On the 6x6x6-type steps, where one table holds
all four orbits, the beam is at 20 to 22 against a bound of 16 to 18.

Tried on the way:

- Each step staged and brought home before the next one, instead of three
  passes over all steps: 303.5 on two 8x8x8 cubes, 380 and 547 on one
  9x9x9 and one 10x10x10, against 300, 361 and 541 on the same cubes in
  passes. The staging of a step mixes the colours of the centres solved
  before it, which then have to be solved again.
- A third table for the 7x7x7-type way home (the four orbits that are
  already home against each other), and beam scores from the largest of
  the tables instead of their sum: no change (30 to 34 moves).
- A bound stretched by 1.3 or 1.5 in the exact search: 27 to 33 and 33 to
  36 moves.

A fault met once, on the 42nd cube of the first run over the three sets: a staging beam found nothing ("no staging of
a ring of centres found", cube 2 of the 10x10x10 set; it did not happen
again in six more runs of that cube). The staging beams and the way home
now try again wider when they come back empty.

## Any size: centres matched, not brought home (2026-11-11, later)

What makes the 6x6x6 and 7x7x7 solvers short, taken to the general solver:
the centres are staged as before, but then each centre is only made to agree
in colour with the one under it (`src/rings.rs`), row by row from the rim
inwards, and the edge steps, which solve the innermost centres of their
small cube anyway, bring the rest home with them. No centre is brought home
by itself and the 4x4x4 solve at the start of an even cube is gone.

20 cubes each, every solution applied on the sticker model and the files
checked again with `cubenxnxn verify`. "Bound" is the number of positions
of the cube counted in moves (log of the positions over log of the number
of moves): no solver can average less.

| Size | Bound | dwalton76 (5 cubes) | Baseline of this morning | Now | Now, his 5 cubes | Now over bound | Mean time | Max time |
|---|---|---|---|---|---|---|---|---|
| 8x8x8 | 117.1 | 308.40 | 301.90 | 264.50 | 261.60 | 2.26 | 14.3 s | 50.9 s |
| 9x9x9 | 149.2 | 368.00 | 368.55 | 349.45 | 356.40 | 2.34 | 32.6 s | 71.1 s |
| 10x10x10 | 179.1 | 523.40 | 531.00 | 529.40 | 531.60 | 2.96 | 10.2 s | 14.9 s |

Files: `general888-20.tsv`, `general999-20.tsv`, `general101010-20.tsv`
(they replace the ones of the section above).

- 8x8x8: all 20 cubes matched in this run; in an earlier run of the same
  cubes 2 went the long way and the mean was 269.40 (the searches run in
  parallel and do not always keep the same states).
- 9x9x9: 6 cubes of 20 went the long way (no matching found for the row of
  five), and on some of the others the row was matched column by column,
  at 71 to 75 moves instead of 46 to 55. The time for 9x9x9 was measured
  while other runs were going on.
- 10x10x10: its outermost row has six centres and no matching was found
  for it on any cube, so every cube goes the long way; the solver no
  longer tries. The small difference to the morning is run-to-run.
- The same code on 6x6x6: 121.60 on 10 cubes (146.0 before; own solver
  109.2). On 7x7x7: 173.5 on 10 cubes, one the long way (190.0 before; own
  solver 154.6).

For comparison, the own solvers are at 1.38 (4x4x4), 1.44 (5x5x5), 1.63
(6x6x6) and 1.67 (7x7x7) times the bound, dwalton76 at 1.75, 1.67, 2.03,
2.10, 2.63, 2.47 and 2.92 for sizes 4 to 10.

What a matched row costs: a row of two centres 11 to 15 moves, of four 28
to 42, of five 46 to 55 (all columns in one search); column by column a row
of four 37 to 46, of five 71 to 75. Against that, bringing the same centres
home takes 21 moves per 6x6x6-type step and about 32 per 7x7x7-type step,
that is 21, 53 and 85 moves for the rows of an 8x8x8 and a 10x10x10.

How it got there, and what did not work:

- **A row in one search, exact agreement only**: fine on 8x8x8 (39 and 13
  moves on the first cube), 78 s on the first 9x9x9, nothing on the
  10x10x10.
- **Agree or opposite, whichever is nearer at each state**: the beam stays
  undecided; 7 of 10 8x8x8 cubes found no matching (8 tries with other
  ties, 30,000 states). **The choice fixed per axis before the search**,
  the eight choices tried in the order of their starting scores: all ten.
- **Column by column with shallow cuts only** (cuts no deeper than the
  column worked on, so that the columns nearer the middle are never cut):
  found nothing on the 9x9x9. With all cuts allowed it works on some cubes
  and is the second way now.
- **Slab moves** (slab under one face half way round, the row under a
  neighbouring face half way round, both again: six moves that change the
  columns the slab cuts and leave the others as they are, so each further
  pair of columns is a search over two columns only): always finds a
  matching, but 105 to 139 moves for a row of six and 122 for a row of
  five. 520.0 on three 10x10x10 cubes against 533 the long way, at a
  minute and a half per cube; one 9x9x9 cube came out at 420. Removed.
- **A score that also weighs the column furthest from done**: a row of six
  still found nothing.
- **Staging all orbits at once before the steps** (a beam on the sum of
  each orbit's distance to its staged state, in small drops): the sum goes
  from 83 down for 9 moves and stops, and the steps after it are no
  shorter: 545 moves on a 10x10x10 against 515 without. Removed. The same
  thing was seen on the 7x7x7 and on the R/L axis of the 6x6x6.

Where the moves of a 10x10x10 go now: staging about 220 (six steps of about
36), the way home about 170, edges 120, 3x3x3 18. On an 8x8x8: staging
about 110, matching about 48, edges 88, 3x3x3 18.

## Any size: a ring onto its faces once (2026-11-11, night)

Until now every staging step (two mirror orbits of obliques as a 6x6x6 or
7x7x7 made of layers) both formed its rows of centres (every centre of a
row of the axis being staged, or none) and brought them onto their faces.
The second part is the same job for every step of a ring, done over and
over. Now the steps only form the rows, and when all rows of a ring are
formed the ring's corner centres and its rows go onto their faces in one
step, as the x and t centres of a 5x5x5 (phase A, then phase B, of the
5x5x5 solver).

20 cubes each, every solution applied on the sticker model and the files
checked again with `cubenxnxn verify`.

| Size | Bound | dwalton76 (5 cubes) | This morning | Before this change | Now | Now, his 5 cubes | Against dwalton76 | Now over bound | Moves per piece | Mean time |
|---|---|---|---|---|---|---|---|---|---|---|
| 8x8x8 | 117.1 | 308.40 | 301.90 | 264.50 | 246.50 | 251.00 | -20.1% | 2.10 | 0.83 | 24 s |
| 9x9x9 | 149.2 | 368.00 | 368.55 | 349.45 | 333.80 | 345.20 | -9.3% | 2.24 | 0.86 | 23 s |
| 10x10x10 | 179.1 | 523.40 | 531.00 | 529.40 | 482.75 | 478.80 | -7.8% | 2.70 | 0.99 | 18 s |

Files: `general888-20.tsv`, `general999-20.tsv`, `general101010-20.tsv`.
The 8x8x8 and 9x9x9 were timed while other runs were going on. Every
10x10x10 cube goes the long way (its outermost row is too long to match).

Staging on one 10x10x10: 165 moves where it was about 220 (R/L: rows
formed in 9, 7 + 10 and 6 + 12 + 13 moves for the three rings, each ring
onto its faces in 10 or 11; the other axes the same). On one 8x8x8: 95
where it was about 110.

The solvers of sizes 4 to 7 make 0.70 to 0.73 moves per piece (a cube of
size n has 6n² - 12n + 8 pieces); at that rate an 8x8x8 would take about
210 moves, a 9x9x9 about 274 and a 10x10x10 about 346.

Tried once more for rows of six centres, in a build of its own: a score
that counts, per axis, the column furthest from done once instead of adding
all columns (so that columns disturbed together are not counted many
times). The searches did not finish one cube in the four and a half minutes
allowed. Not kept.


## 5x5x5: shorter and with a third of the work (2026-11-11, branch `faster-555`)

The machine ran five or six benchmarks of other sessions at once during all
of this (load average 60 to 240 on 10 cores), so wall-clock times mean
nothing here. What is compared is lengths without a clock (`--time 0`,
the same cube then gives the same solution however busy the machine is),
processor time of old and new run side by side on the same cubes, and
positions visited, which `bench5` now prints per phase.

Old and new at the same time, first 12 cubes, `--time 0`:

| Mode | Old length | New length | Old processor time | New processor time |
|---|---|---|---|---|
| fast | 70.00 | 69.00 | 74.7 s | 22.6 s |
| default | 68.83 | 67.75 | 196.5 s | 61.7 s |

On more cubes, new code, `--time 0`:

| Mode | Centres | Edges | 3x3x3 | Total | Recorded before | Positions per cube (centres, C, P, E), millions | Cubes |
|---|---|---|---|---|---|---|---|
| fast | 11.20 + 6.92 | 6.85 + 11.91 + 14.15 | 17.80 | 68.84 | 70.16 | 0.68, 0.10, 0.58, 0.92 | 200 |
| default | 11.63 + 6.30 | 7.00 + 11.70 + 14.03 | 17.23 | 67.90 | 68.6 on the same cubes | 2.17, 0.33, 1.15, 1.65 | first 30 |

The fast mode is now as short as the default was (68.84 against 68.82 on
the 200 cubes; longest 71, was 72 for the default). The default was stopped
after 34 of the 200 cubes (67.91) to free the machine; it and the full mode
are still to be run on the whole set, and all times on a quiet machine.
File: `fast555-results.tsv` is from the old code; the new runs are not in
the folder yet.

What changed, in the order of what it was worth:

- **Phase A of the centres, one length further.** With no clock, the old
  search spent 62% of the processor time in phase A, and its second length
  (optimum + 1) was worth 1.85 moves (70.65 with the optimum only, 68.80
  with the next length, 20 cubes). A third length is worth 0.65 more
  (68.15) and a fourth 0.3 (67.70 on 10 cubes), but each costs about thirty
  times the one before. Three things make them affordable:
  - the table of states near the goal reaches 7 moves instead of 6 in the
    same 64 MB, by storing one state of every 16 that are images of one
    another under the symmetries that keep the R/L axis (20,561,744 states;
    the 17,168,476 states within 6 moves that the unreduced table found
    are counted again from the reduced one as a check), and one bit per
    state says "within 8 moves" (256 MB, 15% of the bits set);
  - the length of phase B is read from its table at every phase-A solution
    from the positions of the U/D centres alone; only the few hundred
    stagings that are kept are worked out on the cube;
  - lengths past optimum + 1 are searched in part: a branch is followed or
    dropped after three moves by a hash of them, a small first part
    measures how large the level is, a second part fills the budget
    (`--a-budget`, positions per length). Part of length three within the
    budget of length two gives 67.93 where the whole of length two gave
    68.10 (30 cubes).
  The two per-orbit tables are indexed by the rank of the mask (0.7 MB
  each, in the cache) instead of by the mask (16 MB each).
- **Edge phases: least slack first.** A start whose estimate is far below
  the total being searched is searched with that much slack, and every move
  of slack makes its search about ten times as large. Within one total the
  starts now go in order of slack, and the level ends when the phase has
  what it was asked for. Before, the last level was always searched to the
  end and returned up to eight times what was asked. Positions in P and E
  went from about 19 and 6.5 million per cube (3 cubes) to 5.2 and 3.5 at
  the same widths and the same length (68.90 against 68.80, 20 cubes).
- **Widths.** With that, the widths that matter are not the ones that were
  large: `p-keep` 700, 2000 and 6000 give 68.25, 68.20, 68.25 and `e-keep`
  100, 200, 600 give 68.30, 68.33, 68.20, while more stagings, labellings
  and phase-C results (cheap) and more 3x3x3 searches each give 0.1 to 0.3.
  The modes are set accordingly (see the README).
- **3x3x3, second phase.** An exact table of corners and U/D edges together
  (2,768 classes of corner permutations under 16 symmetries times 40,320
  edge permutations, 57 MB, 3 s to generate, checked by a test). The same
  solutions in two thirds of the time at 20,000 probes (4.2 s against
  6.1 s for 40 cubes, one thread); used by every size.
- The children of a position are looked up together in C, P, E and A, and
  the last move is tested on the coordinates alone: about 20% in P,
  nothing measurable in E.

Tried, not kept:

- Tables of whole states near the goal for phases P and E (exact to 6
  moves and one bit for 7 in P, 8 and 9 in E, 400 MB each): the number of
  positions did not change. The existing tables are already exact that
  close to the goal; the work is in the first three or four moves of each
  start, where every table says "possible".
- Phase A five lengths past its optimum, sampled: the sampling after three
  moves is too coarse there and a single branch takes minutes.

For the other sizes:

- A phase now returns about what it is asked for, not up to eight times
  as much. Solvers that pass their own widths (6x6x6, 7x7x7) get fewer
  reductions for the same numbers. On the first 10 cubes of the 7x7x7 set
  the unchanged widths gave 155.0 against 154.8 recorded (3x3x3 17.2
  against 16.6); its outer cube now asks for 2,000 and 600 instead of
  inheriting 700 and 100, which gave 155.4 on the first 8 cubes (155.1
  recorded for those), so nothing is gained there yet and nothing lost
  beyond the noise. The 6x6x6 was not measured with this code.
- A cube that arrives with its centres staged (every 5x5x5 the larger
  cubes hand over) skips the lengths of phase A past optimum + 1; without
  that the 6x6x6 spent minutes there.
- General solver: every 5x5x5 step runs with the default widths and no
  clock (reduction alone on 30 scrambles: 51.30 moves with the old fast
  widths, 50.57 with the new fast ones, 49.83 with the default), and the
  last rows of a ring are chosen among the ways found by their length plus
  that of the ring going onto its faces. Measured on few cubes, each
  against the recorded run of the same cubes: 10x10x10 475.5 against
  478.0 (6 cubes), 8x8x8 253 and 248 against 269 and 254, 9x9x9 332 and
  321 against 358 and 324. The centres differ by up to ten moves between
  two runs of the same code, so these say "not worse", not how much
  better. In these steps the centres arrive solved or nearly so (an inner
  5x5x5 step of an 8x8x8 takes 35 moves before and after), so what the
  5x5x5 gained in its centres does not reach them.
