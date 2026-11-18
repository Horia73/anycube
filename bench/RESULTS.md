# Benchmark results

Machine: Apple M1 Pro (8 performance cores), 16 GB RAM. Date: 2026-11-09.
Set: `scrambles-1000.txt`, 1,000 scrambles of 100 random moves each (seed 444).
Metric: OBTM. Every solution was applied to the cube model and verified.

## Mean length per stage

| Solver | Phase 1 | Phase 2 | Phase 3 | 3x3x3 | Total | Min | Max |
|---|---|---|---|---|---|---|---|
| TPR (Java, `tpr-results.tsv`) | 6.727 | 5.847 | 12.362 | 19.638 | 44.574 | 40 | 48 |
| anycube `--n3 1 --probes 500` (`m1-results.tsv`) | 6.666 | 6.021 | 12.323 | 19.828 | 44.838 | 40 | 48 |
| anycube default (`n3-2000-results.tsv`) | 6.652 | 6.223 | 12.605 | 17.438 | 42.918 | 38 | 47 |

## Time per solve

| Solver | Mean | Median | Max | Cubes timed |
|---|---|---|---|---|
| TPR (Java, after JIT warm-up) | 146 ms | 121 ms | 2.4 s | 1,000 |
| anycube `--n3 1 --probes 500` | 121 ms | 101 ms | 1.4 s | 1,000 |
| anycube default (`timing-200.tsv`) | 950 ms | 748 ms | 6.6 s | 200 |

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
| anycube default | 12.775 | 10.895 | 17.000 | 40.670 | 37 | 43 |

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
| anycube, previous default (8 s limit) | 12.775 | 10.895 | 17.000 | 40.670 | 37 | 43 |
| anycube, new default (4 s limit) | 12.802 | 10.809 | 16.986 | 40.597 | 37 | 44 |

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
| anycube `bench5 --extra 0` | 9.63 | 8.56 | 18.19 | 43 ms | 1.1 s |
| anycube `bench5 --extra 1 --time 300` | 10.11 | 7.47 | 17.58 | 0.23 s | 0.82 s |
| anycube `bench5 --extra 1` (no limit) | 10.25 | 7.11 | 17.36 | 0.92 s | 23 s |
| anycube `bench5 --extra 2 --time 2000` | 10.60 | 6.50 | 17.10 | 1.5 s | 2.1 s |

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
| anycube `--mode fast` | 10.37 + 8.21 | 7.02 + 12.55 + 14.10 | 17.93 | 70.17 | 0.44 s | 2.3 s | 200 |
| anycube, default | 10.51 + 8.02 | 7.08 + 12.48 + 13.72 | 17.16 | 68.98 | 2.2 s | 4.9 s | 200 |
| anycube `--mode full` | 10.50 + 7.95 | 7.62 + 12.43 + 13.50 | 16.20 | 68.20 | 8.7 s | 26 s | first 40 |

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

Reproduce with `anycube bench5 bench/scrambles555-200.txt [--mode fast|full] [--limit 40]`.

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

`anycube stage6 bench/scrambles666-100.txt --limit 20`: every centre of the
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

`anycube bench6 bench/scrambles666-100.txt --width 4`, first 6 cubes (the run
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
checked with `anycube verify --size 6` (20 of 20 each). dwalton76: 136.10
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
`anycube verify --size 6` (20 of 20 each). dwalton76: 136.10 moves and
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
the file below was also checked with `anycube verify --size 7`. Another
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

Reproduce with `anycube bench7 bench/scrambles777-100.txt --limit 10
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
with `anycube verify --size 7`: 30 of 30 solved. With the 30 cubes above,
the default is at 154.56 on the first 50 cubes, 20.5% below dwalton76's
194.40 (measured on the first 10 only).

## Any size: the general solver (2026-11-11)

`anycube benchn bench/scrambles888-20.txt --size 8` and the same for 9
and 10: 20 scrambles each of 40 random moves per layer of size (seeds 888,
999, 1110). Every solution was applied on the sticker model, and the result
files checked again with `anycube verify`. dwalton76 ran on the first 5
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
checked again with `anycube verify`. "Bound" is the number of positions
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
checked again with `anycube verify`.

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

## Any size: rows of any length matched (2026-11-11, afternoon)

The matching of a row of centres to the row under it (`src/rings.rs`) got a
way that works whatever the length of the row: from the middle of the row
outwards, the columns done so far kept not only matched but alike (the same
pattern in the row, the same in the row under it). Columns that are alike
and are not cut apart move alike, so they are one thing to follow however
many they are, and a step deals with that one column and the one or two it
adds. The score of a step is an exact table over the four orbits of a new
column and the column it has to be like, per axis, the three axes added.

On random staged cubes (`anycube rowtest --size N --row 1`), row 1:

| Row of | All columns in one search (8 choices of sign) | From the middle outwards |
|---|---|---|
| 4 centres (8x8x8) | 5 of 6 matched, 30.8 moves | 6 of 6, 37.3 moves |
| 5 centres (9x9x9) | 3 of 6, 46.3 moves | 6 of 6, 54.7 moves |
| 6 centres (10x10x10) | 0 of 6 | 4 of 4, 64.2 moves (17 + 31 + 16 for its three steps) |

Rows of up to four centres still try the one search first.

Whole solves, 20 cubes each, every solution applied on the sticker model
and the files checked again with `anycube verify`:

| Size | Bound | dwalton76 (5 cubes) | Before | Now | Now, his 5 cubes | Against dwalton76 | Now over bound | Moves per piece |
|---|---|---|---|---|---|---|---|---|
| 9x9x9 | 149.2 | 368.00 | 333.80 | 326.90 | 336.40 | -11.2% | 2.19 | 0.85 |
| 10x10x10 | 179.1 | 523.40 | 482.75 | 427.95 | 427.80 | -18.2% | 2.39 | 0.88 |

Files: `general999-20.tsv`, `general101010-20.tsv`. No cube of either size
went the long way. On a 10x10x10 the centres now take 284 moves (165 of
staging, about 64 + 39 + 13 of matching), the edges 126, the 3x3x3 18.

Not measured again: the 8x8x8 (its rows have four and two centres, so
nothing changes for it by design; a run was stopped after 14 cubes at
249.07 against the 246.50 on file).

Times are not given: three other sessions were running benchmarks on the
same machine, and the solves came out at one to two minutes per cube. The
tables of the new way (24 MB each, 42 files, 962 MB for sizes 8 to 10) are
made on first use, about half a minute each.

What was tried for long rows before this and did not work:

- Column by column with the columns done before counted once in the score
  (the one furthest from done) instead of each: 1 of 6 rows of five, 0 of 6
  rows of six.
- The new way with a score from pairs of orbits instead of the table over
  four: each requirement alone is met in 6 to 8 moves, two of them in 10
  or 11, all three together not at all within 50 moves.



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


## 6x6x6: faster searches, no copies among the candidates (2026-11-11, afternoon)

Branch `six-faster`. Four other benchmarks ran on the machine the whole
afternoon (load 30 to 300 on 10 cores), so the time on the wall says
nothing; the figures below are processor time, all threads together
(`src/clock.rs`, printed by `bench6` and `stage6`). On an idle machine the
time on the wall was about an eighth of it before these changes. First 20
cubes of `scrambles666-100.txt`, every solution checked on the sticker model.

| Version | Mode | Mean length | Staging | Wing split | Inner reduction | Outer 5x5x5 | Processor time per cube |
|---|---|---|---|---|---|---|---|
| before (3 cubes timed) | fast | 112.00 | 35.80 | 5.95 | 17.35 | 52.90 | about 30 s |
| before (3 cubes timed) | default | 109.20 | 35.00 | 5.65 | 16.80 | 51.75 | about 72 s |
| searches that wait less for memory | fast | 111.65 | 35.45 | 6.05 | 17.15 | 53.00 | 15.0 s |
| the same | default | 109.35 | 35.10 | 5.70 | 16.95 | 51.60 | 51.7 s |
| no copies, wider last staging step | fast | 110.60 | 34.95 | 5.75 | 17.00 | 52.90 | 16.8 s |
| the same | default | 108.35 | 34.55 | 5.55 | 16.90 | 51.35 | 70.8 s (at a load of 200; the two rows above at 30) |

What made it faster, with the same solutions cube for cube (the lengths
differ from the published ones only where the staging searches, which stop
at the first few results of many threads, returned other results):

- **Beam searches** (`search.rs`). Half of a beam's time was one read of a
  table of 240 MB per state (a sample of the running program: 33,000 of
  67,000 samples in the bound). Now the children of eight states are made
  first and their table entries asked for (`Problem::prefetch`,
  `search::fetch`), then rated. The last staging step went from 0.9 s of
  processor time per beam to 0.3 s, the inner reduction from 1.2 s to
  0.8 s. `Problem::solved` lets a problem say "certainly not" before its
  bound is read (for the reduction: the two oblique orbits differ).
- **A beam of the inner reduction that fails** used to run all 40 levels
  four times over: 8 to 9.5 s of processor time, on one beam in six to
  twelve. What it runs onto is a plateau: the inner cube one 3-layer half
  turn short of reduced, every pair of obliques made, and that turn would
  unmake the pairs. The score says three half moves; every state kept has
  it, level after level. Such a beam now gives up after four levels of one
  score (`Problem::patience`) and is not run again: about 1 s.
  The rule is the reduction's own. Applied to every beam it cut the
  matching of long rows short: 3 cubes of 20 on the 8x8x8 went the long
  way that had been matched before.
- The pairing rank is kept in the state of the reduction (it was computed
  three times per state), and the pairing move is two 16-byte table
  lookups on ARM (`tables::sigma_move`).
- The first staging step asks for its two tables for all 54 moves at once.

**Copies.** The candidates carried from step to step were often the same
cube reached by other moves. Counted on six cubes: of the results of the
first staging step 4 to 8 of 8 were different cubes, of the second 32 of
64, 40 of 56, 16 of 32; of the eight stagings handed on, 3, 8, 7, 8, 6 and
8 were different. Each copy took the place of another cube and ran the
same searches again. Now only different cubes are carried on (in the
staging, among the wing splits, among the cubes given to the 5x5x5), and
the last staging step, which sees only the pattern of the centres, is
searched once per pattern. That step also takes four times as many
candidates (`--fan`), which costs little now:

| Staging, 20 cubes, 8 kept | Best | Second | Third | Fourth | Fifth | Sixth | Processor time |
|---|---|---|---|---|---|---|---|
| copies removed, 8 candidates into the last step | 34.55 | 34.95 | 35.40 | 35.68 | 36.22 | 36.33 | 6.9 s |
| copies removed, 32 candidates | 34.20 | 34.55 | 34.95 | 35.11 | 35.47 | 35.63 | 11.5 s |
| copies removed, 64 candidates, 16 results of the first step | 34.25 | 34.55 | 34.90 | 34.89 | 35.37 | 35.53 | 14.6 s |

Tried, no gain:

- **Narrow beams for many candidates** in the last staging step (before
  the copies were found): 64 candidates at 5,000 states 34.50, 32 at 2,000
  34.85, against 34.40 for 8 at 20,000. The beam of that step needs its
  width.
- **The cube held another way** (`--holds`; `Geometry::held`). A 6x6x6 has
  no fixed centres, so any pair of colours may lie on any axis, and the
  steps treat the axes differently: six holds are six problems with the
  same answer. The best staging of six holds is 34.0 against 34.5 for one
  (12 cubes). With the same number of beams, three holds with two
  stagings each give 109.10 against 109.35 for one hold with six: nothing.
  As extra work it pays about as widening does: the shorter of the two
  runs, cube by cube, is 108.8 on the first 11 cubes against 109.45 for
  either. The flag stays, off by default.
- **More solutions per start in phase E of the 5x5x5** (to give the 3x3x3
  more reductions of the shortest length): 68.650, 68.625 and 68.550 on
  40 cubes of the 5x5x5 set for 2, 8 and 32. Not kept.
- **More results of the first staging step** (24 for a width of 8): 34.60
  against 34.55.

## Any size: what the 6x6x6 solver gives the larger cubes (2026-11-11, afternoon)

20 cubes each, every solution applied on the sticker model.

| Size | Before | Now | What changed |
|---|---|---|---|
| 8x8x8 | 246.50 | 243.15 | the innermost ring as a 6x6x6 |
| 10x10x10 | 482.75 | 467.00 | the innermost centres home after the staging |

- **Even cubes, when the centres are matched** (8x8x8): the layers of the
  innermost ring and the four middle ones are a 6x6x6 with staged centres.
  The inner steps of the 6x6x6 solver (wing split, then the reduction of
  the inner 4x4x4 that also makes each pair of obliques one colour) take
  the place of the matching of that ring and of the 4x4x4 solver on the
  innermost wings: about 26 moves where the two took about 30. All 20
  cubes were matched. The gain is smaller than on the 6x6x6 itself, where
  the reduction is chosen among a dozen stagings; here there is one.
- **Even cubes, the long way** (every 10x10x10): the solve began with a
  whole reduction of the innermost 4x4x4, about 24 moves, only to learn
  which colour belongs on which face; the staging then took its edges
  apart again. The staging does not need to know (any frame will do), so
  the innermost centres are now brought home after it, by a table of the
  343,000 patterns of staged centres under face turns and wide half
  turns: 7 moves on the cubes looked at.
- Everything in `search.rs` and in the staging searches serves every size.

Not measured again here: 9x9x9 (odd, nothing but the speed changes).


## 7x7x7 and any size: one search for all candidates (2026-11-11, afternoon)

Branch `faster-777`. Four sessions were benchmarking on the machine at the
same time (load average 30 to 290 on 10 cores), so wall-clock times below
mean little; where a time is given it is processor time, taken back to back
with what it is compared to. Lengths do not depend on the load. The final
times are to be measured after the branches are joined, one run at a time.

### Speed, same solutions

Profile of the 7x7x7 staging as it was: the beam search of step 4 a third,
the exact phase A of the 5x5x5 (`Stage5::dfs_a`) 30%, the beam of step 2
22%. All three wait for memory: their tables (331 MB, 3 x 83 MB, 3 x 16 MB,
64 MB) are far larger than the cache.

| Change | Staging of 5 cubes, processor time |
|---|---|
| as it was | 149 s |
| beam: the children of a state made first and their table entries asked for (`Problem::prefetch`, `search::fetch`), 16 at a time | 102 s |
| the same, 48 at a time (all the children of a state) | 93 s |
| the same in the exact phase A search | 84 s |

The solutions are the same to the move. `axes::Beam` (the way home of the
general solver, tables of 24 MB) fetches ahead as well.

### One beam search from all the candidates

`search::beam_roots`: several roots in one beam, each with the moves it is
behind the shortest (it joins at that level), every solution returned with
the number of its root. Staging of 10 cubes, the five steps as they were:

| Beam searches of a step | Mean length | Processor time per cube |
|---|---|---|
| one of 100,000 for each of 8 candidates | 52.30 | 17.6 s |
| one of 100,000 for all | 53.20 | 4.7 s |
| one of 300,000 for all | 52.50 | 8.4 s |
| one of 100,000 for all, 32 candidates carried | 52.90 | 7.3 s |

So about twice the work for the same length, not eight times. Full solves
of 20 cubes with this and nothing else changed: beams of 300,000 for staging
and matching 153.95, 100,000 and 100,000 155.30, 300,000 and 100,000
154.05, 100,000 and 300,000 154.75, 30,000 and 30,000 156.80 (the code as
it was: 154.80 on the first 10, 154.27 on the first 30, at about twice the
processor time of the first of these).

### Fewer, larger staging steps

Staging of 10 cubes, beams of 100,000:

| Staging | Mean length | Steps |
|---|---|---|
| five steps, as it was | 52.90 | 10.3 + 10.1 + 9.7 + 14.5 + 8.3 |
| a beam step hands on 400 solutions per length, of two lengths, and the step after it picks | 52.30 | step 4 one longer, step 5 one shorter |
| the order of the axes chosen, steps as they were | 53.10 | no gain: each step picks by its own length |
| steps 1 and 2 in one search with every move, fixed order | 52.90 | 20.2 |
| the same, the three axes as three roots of that search | 51.50 | 18.8 + 10.0 + 14.6 + 8.1 |
| and steps 4 and 5 in one search, either other axis as a root | 50.70 | 18.6 + 10.1 + 22.0 |
| steps 1 to 3 in one search instead | 51.60 | 28.2 + 23.4 |

- The beam of steps 1 and 2 does not stall, with a weak bound for the
  triples (tables for every move: at most 3 and 6 moves) and the phase A
  table that is exact within six moves of its end (`Stage5::a_close`) for
  the inner centres, counted twice. Counted once: 57.4; three times: 53.0;
  with the stronger tables of step 2 for the triples: 53.1.
- The choice of axis pays only inside a search. Three roots in one beam let
  the beam find out which axis is short; picking by the length of step 1
  does not.
- Steps 4 and 5 in one: the score of step 4 plus what step 5 would take if
  the triples were formed. That counted twice: 52.8.
- Steps 1 to 3 in one is no better (300,000: 50.6 against 50.4 at 1.7 times
  the processor time), and its results are more alike than the eight
  exact solutions of step 3, which is what the last search wants to start
  from. Removed.
- Wider beams change little here: 50.7 at 100,000, 50.4 at 300,000.

### The matching wants different cubes, not many

Full solves, 20 cubes, staging beam 100,000, matching beam 300,000:

| Staging | Staging | Matching | Both | Total |
|---|---|---|---|---|
| five steps, one beam per step for all | 53.20 | 15.55 | 68.75 | 154.75 |
| merged, axes chosen, 8 stagings to the matching | 50.25 | 16.90 | 67.15 | 153.45 |
| the same, 256 to the matching | 50.45 | 17.10 | 67.55 | 153.05 |
| the same, every way to the staged state kept (below) | 50.30 | 16.40 | 66.70 | 153.15 |

The staging is 2.9 moves shorter and the matching 1.4 longer. The trace
showed why: the goal of the last search is one state of its coordinates,
a beam keeps a state once, so it returned one staging per length: 2 cubes
for the matching to start from, where the five steps gave it 7 or 8
different ones. The matching of a staged cube is 13 to 20 moves, and it is
the best of the cubes offered that counts. `beam_roots` now keeps a solved
state once for every way it was reached (they end in different cubes), one
way to each solved state first. That is the last row: the matching comes
down by 0.7 and the two together are the shortest so far, but the ways to
one state differ only in their last moves, and the matching is still 0.85
above what eight stagings that differ early gave it. (The total did not
follow in this run: the edge steps came out 0.5 longer each, which is
their run-to-run spread on 20 cubes.)

Where the 7x7x7 stands: 153.1 to 153.5 on the first 20 cubes against 154.3
to 154.8 for the code as it was, at about half the processor time (25 s a
cube against 53 s, both under the same load; 7.5 s by the clock at a load
of 115). The default is now a staging beam of 100,000 and a matching beam
of 300,000. `--mode fast` (20,000 and 100,000) has not been measured.

Lengths of matchings for cubes that share most of their staging are
close: what helps is stagings that differ early.

### The general solver (`src/staging.rs`)

The staging passes of `nnn` are now a pipeline over candidates: cubes
carried from step to step, the row-forming steps of 7x7x7 type one
`beam_roots` search for all of them (`Stage7::rows`), the cube started
along each of its three axes and continued with either of the other two.
`anycube stagen <scrambles> --size N` runs the staging alone and checks
it. Staging only, same cubes:

| Size | Cubes | As it was | 1 candidate, 1 order | 8 carried, 4 kept of each, beam 20,000 | 16, 8, beam 100,000 |
|---|---|---|---|---|---|
| 8x8x8 | 5 (as it was: 4) | 92.0 | 90.0 | 85.0 | 81.2 |
| 9x9x9 | 5 | 128.0 | 122.6 | 113.8 | 111.2 |
| 10x10x10 | 3 | 168.3 | 163.0 | 154.0 | 151.0 |

("As it was" for 10x10x10 is the long way, onto the faces of the centres'
own colours; the other columns let an even cube's axes end on any faces.)
Processor time for the staging of a 9x9x9: 2.3 s with one candidate, 4.3 s
and 8.3 s for the last two columns. An 8x8x8 takes 17 s and 28 s, because
its 6x6x6-type steps (`stage6`) still run once per candidate.

- **Rows of the first axis with every move** (steps 1 and 2 of the 7x7x7
  in one): 115.8 against 113.8 on the 9x9x9. Its inner centres are staged
  already there; nothing to share. Removed.
- **Wing parity.** Step 4 makes the wings of its third layer even. Leaving
  that alone in every step (the quarter turns of later steps undo it
  anyway) shortened the staging by a move or two per step and lengthened
  the edges by eleven: 132 against 121 on two 10x10x10 cubes, because each
  edge step then starts by fixing the parity of its orbit from solved
  centres. Now only the step of the outermost ring fixes it, after which
  nothing changes it again: 9x9x9 112.6, 10x10x10 155.0 (was 111.2 and
  151.0 with no step fixing it).
- **Renaming colours from a cell of the cube** to see it along another
  axis failed half way through a staging (no cell is sure to show its
  face's colour then); the names come from the turn itself.

Whole solves with the new staging, the cubes run so far: 9x9x9 cube 0 at
326 (358 before), cube 1 at 347 the long way (324 before, matched); 8x8x8
cube 0 at 259 (269). Whether the rows are matched decides more than the
staging does, and that part is another branch's work. Sizes 8 to 10 have
to be run again on the joined code.

### Not done

- The edge steps of the general solver still carry one cube. The 7x7x7
  carries 16 reductions from its inner cube to its outer one; the same
  needs `Solver5` to hand back several reductions with their cubes.
- The 6x6x6-type steps of the staging as one search for all candidates.
- A matching search better than a wide beam. Only half turns of two layers
  change which outer centre lies beside which inner one; face turns and
  half turns of three layers move whole blocks. Its length falls steadily
  with the width (16.9, 16.1, 15.3 moves at 30,000, 100,000 and 300,000),
  which says the beam is still far from the shortest.

## 4x4x4: cubes with a known short reduction (2026-11-10)

`anycube known --walk K --count 30 --time 30000`: a random 3x3x3 state on
the 4x4x4 (40 random face turns) gets K random moves out of all 36, so a
reduction of at most K moves exists: the walk undone. The walk starts with a
wide move, keeps face turns off the axis of its first moves and never has two
opposite wide moves in a row, so the known reduction cannot be shortened at
sight. It is still only an upper bound: on a few cubes the solver found a
shorter one. Every known solution and every solver solution was checked.

"Shortest found" is the shortest reduction the search came across; the
solution may use a longer one with a shorter 3x3x3. The known total is the
walk plus a two-phase 3x3x3 solution of the base, which is not optimised.
Files: `known444-wK.tsv` (cube, K, 3x3x3 of the base, shortest reduction
found, reduction used, 3x3x3 used, total, ms, base, walk, solution).

| Walk K | Known reduction | Shortest found | Found minus known | Known total | Solver total |
|---|---|---|---|---|---|
| 14 | at most 14 | 18.00 | +4.0 | 32.80 | 35.77 |
| 16 | at most 16 | 20.90 | +4.9 | 34.47 | 38.47 |
| 18 | at most 18 | 22.43 | +4.4 | 36.70 | 39.80 |
| 20 | at most 20 | 22.73 | +2.7 | 38.93 | 40.00 |
| 60 (random cubes) | unknown | 23.10 | | | 40.27 |

Found minus known, cube by cube:

| Walk K | Shorter | Equal | 1 to 3 longer | 4 to 6 longer | 7 to 10 longer |
|---|---|---|---|---|---|
| 14 | 3 | 9 | 3 | 3 | 12 |
| 16 | 2 | 4 | 1 | 10 | 13 |
| 18 | 0 | 2 | 2 | 26 | 0 |
| 20 | 0 | 0 | 26 | 4 | 0 |

- A cube 20 moves from a reduced cube is almost a random cube to the solver:
  22.73 against 23.10. On those the three-phase reduction is 2.7 moves longer
  than one that is known to exist.
- At 14 moves the result splits in two: on 12 of the 30 cubes the solver finds
  the known reduction or a shorter one, on another 12 it lands at 21 to 24, no
  better than on a random cube. Being close to a reduced cube does not help
  unless the short path happens to fit the phases.

The machine was busy during these runs (load average 25 to 32 on 8 cores).
The time limit was raised to 30 s so that the endpoint counts, not the clock,
end each stage; solves took 3 to 4.5 s on average.

## 4x4x4: reductions with all moves (2026-11-11)

Code in `src/free.rs`. Two cubes have the same reductions when one is the
other preceded by face turns and whole-cube rotations; `key` names that class
(centres, which positions hold the halves of the same dedge, two parities).
The key was checked on 2,000 random cubes: it does not change under face turns
and rotations done first, and it equals the key of the solved cube exactly
when the cube is reduced.

Classes by distance from a reduced cube, with the 27 moves that matter (face
turns, Uw, Rw, Fw), generated in 5 s:

| Moves | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| Classes | 1 | 9 | 162 | 3,168 | 61,764 | 1,198,247 | 23,572,977 |

The count grows about 19.6 times per move. With 4.1×10^27 classes in all, a
random cube should have its shortest reduction at about 21 moves.

### Phase 3 against the shortest reduction with any move

`anycube free3 --walk W --forward F --ball 6`: a reduced cube gets W random
phase-3 moves; the shortest phase-3 solution is compared with the shortest
reduction with all moves, which is exact up to F + 6 moves (every sequence of
F moves is tried against the table above).

| Run | Cubes | Phase 3 at most F + 6 | Same length with all moves | Shorter with all moves |
|---|---|---|---|---|
| `--walk 16 --forward 4` (exact to 10) | 40 | 25 | 25 | 0 |
| `--walk 24 --forward 6` (exact to 12) | 24 | 7 | 7 | 0 |

In the second run one cube with a 13-move phase 3 has a 12-move reduction with
all moves; the other 16 cubes with phase 3 of 13 or 14 moves have none of 12
or fewer. So phase 3 is as short as any reduction from the states it starts
from, at least up to 12 moves: what the three-phase reduction loses, it loses
by having to pass through a state with staged centres.

### What a cube K moves from a reduced cube looks like

`anycube free-features`, 200 cubes per K: exact staging distance of the
centres (best of the six ways of giving the colour pairs an axis) and dedges
already paired.

| K | Staging distance | Share at most 6 | Dedges paired |
|---|---|---|---|
| 8 | 5.31 | 0.65 | 2.83 |
| 10 | 6.33 | 0.44 | 2.38 |
| 12 | 7.20 | 0.29 | 1.82 |
| 14 | 7.84 | 0.18 | 1.36 |
| 16 | 8.12 | 0.15 | 1.17 |
| 18 | 8.61 | 0.10 | 0.99 |
| 20 | 8.52 | 0.09 | 1.01 |
| 60 (random) | 9.09 | 0.00 | 0.48 |

### Beam search with all moves on the known-reduction cubes

`anycube free-beam --walk K --depth K-2 --solver bench/known444-wK.tsv`:
each level keeps the 300,000 cubes with the lowest 2 × staging distance −
dedges paired, and a path ends as soon as it is within 6 moves of a reduced
cube. It can find reductions of up to K + 4 moves (24 for the random cubes).
Same cubes as the `known` runs above; "solver" is the shortest reduction the
three-phase search came across. Every reduction was checked. Logs:
`beam444-wK.txt`.

| K | Cubes | Beam found any | Beam at most K | Solver at most K | Either at most K | Beam shorter than solver | Solver mean | Best of both | Beam time |
|---|---|---|---|---|---|---|---|---|---|
| 14 | 30 | 15 | 13 | 12 | 17 | 6 | 18.00 | 17.10 | 9.9 s |
| 16 | 20 | 3 | 2 | 3 | 3 | 0 | 21.35 | 21.35 | 13.0 s |
| 18 | 12 | 5 | 3 | 2 | 4 | 3 | 21.67 | 20.67 | 16.5 s |
| 20 | 12 | 3 | 0 | 0 | 0 | 1 | 22.83 | 22.75 | 14.2 s |
| 60 (random) | 6 | 2 | | | | 1 | 22.67 | 22.50 | 17.5 s |

- At 14 moves the beam finds the known reduction about as often as the solver
  and on different cubes. From 16 on it finds nothing on most cubes, and on
  random cubes it is worse than the solver at four times the time.
- The signals fade: 14 moves from a reduced cube the staging distance is 7.8
  against 9.1 on a random cube. Nothing in this score tells the beam which
  cubes are 15 moves away rather than 21.

The machine was very busy during these runs (load average 50 to 95 on 8
cores), so the times are upper estimates.

## 4x4x4: the inverse cube and a wide move in front do NOT work (2026-11-11)

Two ideas were measured by solving other cubes and counting lengths, and
both were wrong. This section replaces the figures first written here
(40.37 and 39.67 for "cube and inverse", 39.85 for "a wide move in front"):
none of those is a real result.

- **Inverse cube**: the solution of the inverse scramble, undone, was taken
  to solve the scramble.
- **Wide move in front**: the solution of "w then scramble", followed by one
  wide move, was taken to solve the scramble.

Applied to the real cubes with `anycube verify --size 4`:

| Claimed solutions | Solve the cube |
|---|---|
| plain solutions of the first 60 cubes (control) | 60 of 60 |
| undone solutions of the 60 inverse scrambles | 0 of 60 |
| 180 solutions of cubes with a wide move in front, each tried with all 18 wide moves as the last move | 0 of 3,240 |

The reason is that the four centres of a colour cannot be told apart. A
solution of the inverse cube leaves every centre on a face of its colour but
not in its own place; undone on the real cube, that hidden permutation of
centres is carried through the scramble and comes out as centres on wrong
faces (the wings and corners do end up solved). The same happens with a move
in front. It would only work if the solver put every centre back in its own
place, which is a much longer solve. For the same reason "does not try the
inverse scramble" is not a loss of cube555 on the 5x5x5.

What remains true from those runs: the default mode on the first 60 cubes
gives 40.88 in 2.75 s, twice the effort in one direction 40.57 in 4.42 s, and
`--mode full` 40.10 on the first 30 cubes (18.2 s on a busy machine).

## 4x4x4: is the end of the reduction as short as it could be? (2026-11-11)

`anycube free-tail bench/scrambles-1000.txt --limit 24 --back 3`: the cube
is solved with the default flags, the reduction of the solution is cut j moves
before the end of the joint phase, and what follows (j moves of the joint
phase plus phase 3) is compared with the shortest reduction from that point
with all moves, exact up to 12 moves (so decided when the rest is at most 13).

| Cut j moves before the end of the joint phase | Cubes decided | Shorter with all moves | Moves saved |
|---|---|---|---|
| 0 | 23 | 2 | 2 |
| 1 | 19 | 1 | 1 |
| 2 | 12 | 1 | 1 |
| 3 | 8 | 0 | 0 |

The one cube that is shorter at j = 1 and j = 2 is one of the two at j = 0.
So the last 12 to 13 moves of the solver's reduction are almost always as
short as any reduction from that point: whatever the three-phase reduction
loses against a free one, it loses in its first ten moves or so.

## 4x4x4: a small network as the estimate of the distance to a reduced cube (2026-11-11)

`ml/reduce_net.py`: a ResMLP with 4.4 million parameters, trained on the GPU
of the M1 Pro for 36 minutes on 192 million cubes from `free-dump` (a reduced
cube plus K random moves, K from 1 to 26; the label is K, or the exact
distance when it is at most 6). Its input is what decides the reductions of a
cube: which pairs of centres have the same or opposite colours, where the
other half of each wing's dedge is, and the wing parity.

Share of 300,000 random cubes that look at least as close to a reduced cube
as the median cube K moves away (3,000 cubes per K, none seen in training);
the hand score is the one of the beam search above:

| K | Network's estimate | Network | Hand score |
|---|---|---|---|
| 8 | 9.20 ± 2.04 | 0.0003% | 0.001% |
| 10 | 12.01 ± 3.04 | 0.011% | 0.25% |
| 12 | 14.49 ± 3.44 | 0.26% | 2.7% |
| 14 | 16.34 ± 3.27 | 1.6% | 19.6% |
| 16 | 17.56 ± 3.09 | 5.8% | 19.6% |
| 18 | 18.56 ± 2.72 | 13.1% | 38.4% |
| 20 | 19.33 ± 2.29 | 22.7% | 38.4% |
| random | 20.66 ± 1.32 | | |

`ml/reduce_beam.py`: beam search with all moves guided by the network alone
(no table near the goal), up to 30 moves, on the first cubes of
`scrambles-1000.txt`; every reduction was checked with `free-verify`. The
solver's shortest reduction on these 20 cubes averages 23.05.

| Beam width | Cubes | Reduction found | Lengths | Time per cube (Mac GPU) |
|---|---|---|---|---|
| 1,024 | 20 | 1 | 29 | 5 s |
| 4,096 | 20 | 4 | 28, 29, 30, 30 | 42 s |
| 16,384 | 10 | 2 | 27, 29 | 47 s |

The network is about ten times more selective than the hand score, and a beam
guided by it is still far behind the solver: 27 to 30 moves on a minority of
cubes against 23. On cubes close to a reduced cube it works (an 11-move walk
is found in 11 moves at width 4,096).

## 4x4x4: lightly scrambled cubes (2026-11-11)

`anycube known --walk N --base solved --count 20`, default flags: the
solved cube gets N random moves.

| N | Solver, mean length |
|---|---|
| 10 | 10.00 |
| 15 | 28.25 |
| 20 | 41.75 |
| 25 | 41.70 |

The machine was running the beam search above on its GPU during these runs.

`anycube short --walk N --forward F --ball 5`: the shortest solution found
exactly (every sequence of up to F moves against a table of the 5.3 million
cubes within 5 moves of solved), next to the solver's default solution. Every
exact solution was applied to the cube and solves it.

| N | Cubes | Exact | Solver, cube by cube |
|---|---|---|---|
| 11 (`--forward 6`) | 6 | 11 on all | 11 on all |
| 12 (`--forward 7`) | 6 | 12 on all | 12, 12, 12, 12, 30, 42 |
| 13 (`--forward 7`, exact only up to 12) | 4 | nothing up to 12 | 13, 13, 13, 42 |

The solver either finds the short solution or falls back to a normal one of
30 to 42 moves; from 12 moves on it starts to miss. The exact search took
about 30 s per cube at 11 moves and 3 to 20 minutes at 12 on a machine under
very heavy load (load average 100 to 150), so the times say little; it is not
pruned at all.

## 4x4x4: four times the effort of full mode (2026-11-11, not conclusive)

`bench --limit 12 --mode full --n2 600000 --k3 600000 --joint-nodes 80000
--time 120000 --exact 240`: 40.42 against 40.08 for `--mode full` on the same
12 cubes, at 202 s per cube. The machine was under very heavy load (load
average 90 to 140) and the stages were cut by the clock, so this says nothing
about what more effort is worth; it has to be repeated on a quiet machine.

## 4x4x4: our solutions in single-layer quarter turns (2026-11-11)

CayleyPy (arXiv 2502.13266) reports 4x4x4 results in the metric of the Kaggle
Santa 2023 challenge: every quarter turn of a single layer is one move, no
piece is fixed. On 43 scrambles of that challenge: 53.49 for the best
submissions combined, 48.98 for one network with beam width 2^24, 46.51 for
29 networks. On its first 20 scrambles, one network gives about 58 at width
2^18, 55 at 2^20, 52 at 2^22 and 50 at 2^24.

Our 1,000 default solutions (`default-results.tsv`, 40.60 in OBTM) counted in
that metric: 71.36 move by move (wide quarter turn 2, half turn 2, wide half
turn 4), 66.50 when each run of turns on one axis is written in the cheapest
way. The solver does not optimise for this metric, and their solutions are
not published, so they cannot be counted in OBTM.

Counting bound, the same for both metrics: canonical sequences grow 20.73
times per move in OBTM and 18.52 times per single-layer quarter turn; their
number reaches the 7.4×10^45 positions at length 35 in OBTM and 37 in
single-layer quarter turns.

## 6x6x6 on the merged code (2026-11-11, evening, branch `together`)

All branches of the day together. First 20 cubes of `scrambles666-100.txt`,
every solution checked on the sticker model, one run at a time; one other
search was still running on the machine (load 15 to 20 on 10 cores), so
the times are upper estimates.

| Mode | 5x5x5 end | Mean length | Staging | Wing split | Inner reduction | Outer 5x5x5 | Mean time | Max time | Processor time |
|---|---|---|---|---|---|---|---|---|---|
| default | `p_keep` 4000, `e_keep` 800 (as before the merge) | 108.85 | 34.45 | 5.90 | 16.60 | 51.90 | 8.2 s | 11.8 s | 40.5 s |
| default | `e_keep` 2400 | 108.60 | 34.50 | 5.75 | 16.55 | 51.80 | 7.1 s | 10.5 s | 52.1 s |
| **default, as it is now** | `p_keep` 12000, `e_keep` 2400 | **108.35** | 34.50 | 5.90 | 16.40 | 51.55 | 9.4 s | 15.9 s | 61.5 s |
| fast | `p_keep` 300, `e_keep` 200, clock of 1.5 s (as before the merge) | 110.70 | 34.85 | 5.95 | 17.00 | 52.90 | 2.2 s | 7.4 s | 14.2 s |
| fast | `p_keep` 1000, `e_keep` 600, clock of 1.5 s | 110.80 | 34.75 | 6.05 | 17.45 | 52.55 | 2.6 s | 4.9 s | 18.4 s |
| **fast, as it is now** | `p_keep` 1000, `e_keep` 600, no clock | **110.65** | 34.75 | 6.15 | 17.40 | 52.35 | 2.4 s | 3.7 s | 17.5 s |

Against this morning (109.20 in 9.7 s by default, 112.00 in 4.1 s fast, on
an idle machine): 0.85 moves shorter by default in about the same time,
1.35 moves shorter and nearly twice as quick in the fast mode. dwalton76:
136.10; the target of 20% fewer moves (108.9) is now met by default.

- The edge phases of the 5x5x5 branch return about as many results as they
  are asked for, where they returned several times as many. With the
  numbers the 6x6x6 solver had, the 5x5x5 end got a third of the
  reductions and the solutions were half a move longer; with three times
  the numbers the length of the 6x6x6 branch (108.35) is back.
- The fast mode has no clock any more: the same cube gives the same
  solution however busy the machine is, and the slowest solve is shorter.
- `--mode full` has its numbers scaled the same way and has not been run
  on the merged code.

A fault of the merge, found by the first runs and put right: on an even
cube the staging of the 7x7x7 branch leaves any pair of colours on any
pair of faces, and the 6x6x6 steps for the innermost ring took each pair
to be on its own. With two pairs exchanged they brought the centres home
as a mirror image of the cube's colours; no way of holding the cube fits
that, and the cube went the long way (5 of the first 8 of the 8x8x8 set,
and the 10x10x10 cubes that were seen to take it). The frame is now turned
onto the colours' own axes first: 8 of 8 matched, 236.75 moves on those 8
cubes (247.0 this morning).

## 2026-11-11, evening: one version, every size from 2 to 21

The branches of the day are one commit (10b980f), and one session measures,
one run after another on an otherwise idle machine (M1 Pro, 10 cores).

The fixed sets of sizes 4 to 10 on that commit, default modes, every
solution applied on the sticker model:

| Size | Mean length | Seconds per cube | Cubes |
|---|---|---|---|
| 4x4x4 | 40.67 | 2.8 | first 100 |
| 5x5x5 | 67.77 | 0.55 | 200 |
| 6x6x6 | 108.45 | 8.6 | first 20 |
| 7x7x7 | 152.75 | 3.6 | first 20 |
| 8x8x8 | 235.80 | 13.0 | 20 |
| 9x9x9 | 314.65 | 4.0 | 20 |
| 10x10x10 | 402.45 (centres 253.90, edges 131.05, 3x3x3 17.50) | 16.6 | 20 |

No cube of 8 to 10 went the long way.

Then every size from 2 to 21 with `scripts/release.py measure v0.1.0`
(commit b2f038a: the same solvers, the `row` command and ring tables shared
between sizes); the numbers are in `bench/releases.tsv` and in the table of
README.md. New scramble sets: 100 cubes of 2x2x2 and 3x3x3 (40 random
moves), 5 cubes of each size from 11 to 21 (50 random moves per layer of
size).

What the two runs show together:

- The odd sizes repeat to the hundredth (7x7x7 152.75 twice, 9x9x9 314.65
  twice); the even ones do not (6x6x6 108.45 and 108.85, 8x8x8 235.80 and
  235.65, 10x10x10 402.45 and 406.05). So what is not repeatable is in the
  6x6x6 steps, which every even cube goes through. The 4x4x4 and 5x5x5
  move in the second decimal; their default modes have a clock.
- Even cubes take 6 to 9 seconds more than the odd size after them, about
  one 6x6x6 solve, whatever the size. The rest of the time grows with the
  number of rings for both.
- Moves per piece (6n² - 12n + 8 pieces): 0.69 to 0.73 for sizes 4 to 7,
  0.80 at 8, rising slowly to 0.91 at 19 and 0.94 at 21. From 13 up the odd
  sizes are 1 to 3% above their even neighbours.
- Sizes 11 to 21 needed no ring table that sizes 8 to 10 had not made:
  twelve files, 275 MB, for every size.


## 5x5x5: a search for a shorter structure (2026-11-11, night, branch `claude/five-shorter-dc1676`)

The aim was a mean of about 60 on `scrambles555-200.txt` (67.75 today; So
Hirata's estimate of God's number is 58). Nothing tried here shortened the
solutions, and the measurements say why. What stays in the code is the
instrument, `known5`; everything else was removed again, tables included.

Another session ran benchmarks on this machine the whole time (load average
30 to 130 on 10 cores), so lengths are compared without a clock (`--time 0`)
and times are only orders of magnitude.

Baseline, first 40 cubes, `--time 0`: 67.98 (centres 11.60 + 6.20, edges
7.15 + 11.75 + 14.05, 3x3x3 17.23). `--mode full` on the first 12: 67.17
against 67.75 for the default on the same cubes, with 7 times the positions
in the centres and 3 to 4 times in the edge phases.

### Counting, as PLAN.md did for the 4x4x4

A phase that tells S states apart with b continuations per move needs
about ln S / ln b moves. The 5x5x5 has ln(2.83e74) = 171.4; b is the growth
of move sequences that are not one another in a different order.

| Moves | How many | b | ln b |
|---|---|---|---|
| all | 36 | 28.1 | 3.34 |
| faces and wide half turns (centres stay staged) | 24 | 18.5 | 2.92 |
| U D F B freely, R2 L2, wide half turns (wing halves stay apart) | 20 | 13.7 | 2.62 |
| 3x3x3 | 18 | 13.35 | 2.59 |
| phase P | 16 | 12.05 | 2.49 |
| phase E | 14 | 10.55 | 2.36 |
| second phase of the 3x3x3 | 10 | 6.8 | 1.91 |

| Phase | ln of its states | ln S / ln b | Measured |
|---|---|---|---|
| centres, A + B | 27.0 + 19.6 | 8.1 + 6.4 to 7.4 | 11.63 + 6.30 |
| C | about 18 | 6.2 | 7.00 |
| P | 28.4 | 11.4 | 11.70 |
| E | 33.3 | 14.1 | 14.03 |
| 3x3x3 | 45.2 | 17.5 | 17.23 |
| all | 171.4 | about 65 | 67.75 |

- Every phase but the centre staging is at what its own moves allow. The
  searches are not what loses moves; passing through the phases' goals is.
- 60 moves is 2.86 per move on average. Only "all moves" and "faces and
  wide half turns" have that much, and the 3x3x3 at the end is 45 of the
  171 at 2.59. So 60 would take a search with all 36 moves that is close
  to the shortest over the first forty moves.
- The end as the second phase of the 3x3x3 alone (corner twist, edge flip
  and the slice edges seen to before the last edge phase): its 23.7 cost
  12.4 moves at 6.8 continuations, and the 21.5 moved earlier cost 7.5 to
  8.5: about 20 against 17.5. Choosing among many ends by its exact table
  wins back part of it. Measured below (7): 3.5 moves longer.
- The count is a ceiling and proved too generous where it promised gains:
  see the two searches with 20 moves below.

### `known5`: cubes a known number of moves from a reduced cube

`anycube known5 --walk K --moves all|h|g [--base solved]`, 12 cubes each,
default mode, no clock. "Reduction found" is centres plus edges.

| Walk | Exists | Reduction found |
|---|---|---|
| 12, all 36 moves | 12 | 20.25 |
| 18, all | 18 | 43.08 |
| 24, all | 24 | 49.58 |
| 30, all | 30 | 49.83 (random cubes: 50.5) |
| 10, faces and wide half turns | 10 | 12.42 |
| 14 | 14 | 18.17 |
| 18 | 18 | 23.25 |
| 22 | 22 | 27.75 |
| 26 | 26 | 30.25 |
| 8, moves that keep the wing halves apart | 8 | 8.08 |
| 12 | 12 | 13.83 |
| 16 | 16 | 19.83 |
| 20 | 20 | 24.08 |
| 24 | 24 | 25.42 |

From the solved cube: 20 random moves are solved in 61.5, 30 in 67.1, 40
in 67.5.

The solver does not see that a cube is close: 18 moves from a reduced cube
it finds 43. That is a fact about such cubes, not about random ones: a
search that does find the short way on them (the beam below) finds nothing
shorter than phases P and E on random cubes.

### Tried, none kept

1. **Phase E with all 20 moves that keep the wing halves apart** (F and B
   freely as well, Uw2 and Dw2). Bounds: the exact number of moves that
   bring the low wings to their middle edges, for all 12! arrangements, the
   same for the high wings (240 MB each, about 100 s each to build here),
   the 4x4x4's pairing table for low against high, and the centres two
   pairs of faces at a time. From states of phase E the shortest way with
   20 moves is as long as with 14: 15 and 15 on two cubes, none of at most
   16 on a third; 5 to 33 million positions each, the bound at the start 8
   or 9. Moves that leave a subgroup pay only over a stretch long enough to
   make up for leaving it, and 15 moves is not.
2. **Reductions chosen by the first-phase distance of their 3x3x3** (the
   twist, flip and slice class is fixed from the end of phase P on). Over
   4,800 reductions of 12 cubes the 3x3x3 found after 5,000 probes is
   18.78, 18.93, 19.12, 19.37, 19.66 for a distance of 7, 8, 9, 10, 11:
   0.2 per unit, and 6% of the reductions are at 8 or less. Nothing to
   select by.
3. **The centres staged in one exact search with more bounds**: next to
   the two orbit tables, for each of the three axes all eight centres of
   one orbit with the four of one colour of the other (two tables of 493
   and 499 million entries, 247 MB each, 40 s each), and the exact table of
   phase B wherever one axis is in place. The mixed tables average 7.6
   where the orbit tables average 9.1, so they cut nothing: the first cube
   was not done after 10 minutes on 5 cores, no better than the 340 s
   recorded for the search with the orbit tables alone.
   From the tables themselves: 0.1%, 1.2%, 13% and 73% of one orbit's
   states are within 6, 7, 8 and 9 moves of staged. A search to 15 moves
   therefore visits about 6e8 positions, 5e8 of them 8 or 9 moves from the
   end, where each orbit alone says "possible" and only a table of both
   orbits could say no. Such a table would need the states within 9 moves
   of staged, several thousand million.
4. **Pairing by a beam search with the 20 moves**, in place of phases P
   and E, from all the results of phase C at once. Score: the largest of
   the three edge tables of (1) and the three centre tables, then their
   sum. On the `known5` cubes it finds the way that exists (width 100,000):

   | Walk | Beam | Solver (same kind of cubes) |
   |---|---|---|
   | 12 | 11.50 | 13.83 |
   | 16 | 16.08 | 19.83 |
   | 20 | 20.67 | 24.08 |
   | 24 | 23.67 | 25.42 |

   On the first five cubes of the benchmark it needs 25 to 31 moves (28.2)
   where phases P and E need 25.6, and it returns 2 to 10 reductions, too
   few to choose a 3x3x3 from: 71.6 in all against 67.4. At a width of a
   million (two cubes): 25.5 against 26, 40 s of processor per cube, 2 to 4
   reductions. So on random cubes phases P and E are within half a move of
   what a wide search with all 20 moves finds.
5. **The centres staged by a beam search** with all moves (score from the
   orbit tables and the mixed tables of (3), width 100,000, about 3 s per
   cube here): 17.38 against 16.50 for the two-phase staging on 8 cubes.
6. **Phase B of the centres and phase C of the edges as one search** from
   the ends of phase A (the 4x4x4 gained 1.2 moves from joining its first
   two phases). Bounds: the exact table of phase B and the table of phase C
   made again for the moves of both (175 MB). The two share almost no
   moves: the bound at the start is 5 below the answer, the first cube
   took 336 million positions from 100 ends of phase A with four
   labellings each, and its shortest
   was 23 moves to the end of phase C, the same as the two phases one
   after the other.

7. **The reduction ending in a cube that the second phase of the 3x3x3
   solves alone.** Phase C keeps the wings as they are named (one
   labelling instead of the best of 2,048, which leaves every edge
   unflipped), phase P builds the four edges that belong in its slice and
   brings the corner twist to nothing (a table of the twist under its
   moves as a further bound), phase E is unchanged and the 3x3x3 solver
   finds the cube already in the subgroup. First four cubes:

   | | Centres | C | P | E | 3x3x3 | Total |
   |---|---|---|---|---|---|---|
   | as it is | 10.75 + 6.75 | 7.00 | 11.50 | 14.25 | 17.25 | 67.50 |
   | ending in the subgroup | 11.50 + 7.25 | 11.25 | 15.25 | 15.00 | 10.75 | 71.00 |

   The end is 6.5 moves shorter and the way to it 10 longer, as the
   count said. Phase P took 72 million positions per cube instead of 1.1.

### Where that leaves 60

- Within the present phases, more effort buys tenths: 0.6 for three to
  seven times the positions searched (full mode above).
- Each phase is at or near the shortest for its own moves on random cubes
  (the count, and the beam of (4) for the pairing). What is lost is lost
  between phases, and joining two of them needs a bound that sees both:
  A with B is 15 against 17 on the one cube measured at 340 s; B with C
  gains nothing; C with P and P with E have no table that covers both
  sides.
- So about 66 looks reachable at a minute per cube (the centres in one
  exact search, once it also returns enough stagings to choose from; an
  estimate from the one cube measured), and 67 to 67.5 in seconds. 60 is not reachable by a solver made of phases;
  it would be a search with all moves through most of the solve, which no
  table that fits in memory can bound.

The code of the seven attempts is not in the repository; the diffs are in
the notes folder of the sessions (`five-shorter-tried/`).

## 3x3x3: every first phase of 14 moves, on all cores (2026-11-11, night, branch `three-shorter`)

The 3x3x3 of the release stopped after 30,000 second phases on one core:
19.03 moves in 0.22 s on the 100 scrambles of `scrambles333-100.txt`, where
the optimum of a random cube is 17.7 on average. `--mode full` found the
optimum in 5 to 30 seconds a cube, some in two minutes.

Where the moves were: the same search with 10 and 100 times as many second
phases gave 18.61 and 18.28, so the length follows the number of ways into
the domino subgroup that are tried. The search needed to be cheaper per
way, not different.

The new search (`src/two3.rs`, `src/first3.rs`; how it works is in
`docs/how-it-works.md`) takes the ways by length, all of a length for all
six variants (three axes, the cube and its inverse). 100 cubes, every
solution applied on the sticker model, the machine otherwise idle except
where it says:

| First phase up to | Ways per cube | Moves | Seconds per cube | Longest |
|---|---:|---:|---:|---:|
| the release, default | 30 thousand | 19.03 | 0.22 | 0.42 |
| the release, fast | 500 | 19.93 | 0.003 | 0.01 |
| 12, all variants (`--mode fast`) | 49 thousand | 18.84 | 0.002 | 0.01 |
| 13, three variants | | 18.60 | | |
| 13, all | 0.67 million | 18.41 | 0.02 | 0.03 |
| 14, one variant | | 18.33 | | |
| 14, two | | 18.28 | | |
| 14, three | | 18.21 | | |
| 14, four | | 18.17 | | |
| 14, all (default) | 9.2 million | 18.13 | 0.20 | 0.38 |
| 15, one variant | | 18.07 | | |
| 15, two | | 18.04 | | |
| 15, all | 125 million | 17.87 | 2.9 (first 30) | |

Lengths at 14: 2 cubes of 16 moves, 13 of 17, 56 of 18, 28 of 19, 1 of 20.
The fast mode is now shorter than the default was, in a hundredth of the
time; the default is 0.9 moves shorter in the same time. Memory is 208 MB
(77 MB before): the table of the first phase is 122 MB on disk, and the
three tables of the 3x3x3 take 4.3 s to make from nothing, once.

The optimal mode starts from the two-phase solution and searches only the
lengths below it; if the two-phase search found nothing under 19 moves it
is first run one move deeper, for 18 or fewer. First 20 cubes (7 of 17
moves, 11 of 18, 2 of 19; mean 17.75, the same lengths from both):

| | Mean seconds | Longest |
|---|---:|---:|
| the release | 18.8 | 112 |
| now | 7.1 | 43 |

A cube that needs 19 moves is what takes long now: every length up to 18
has to be searched to the end to show it. A cube of 18 whose two-phase
solutions are all of 19 is the other slow case: the 18 is then found by
the exact search, as before (the example in the README is one: 37 s, against 25 s when the release was measured, here with other sessions running).

What it took to make a way cheap, in the order it was found. Counted on
the first two cubes at 14 moves (17.8 million ways), one core, or side by
side on the first 30 cubes on all cores:

| Step | Instructions per way | Processor time |
|---|---:|---|
| exact distances from the 554 MB table, a table entry per move tried | 2040 | 9.2 s |
| the same from a 35 MB table by symmetry, moves listed near the subgroup | about the same | |
| shortest ways of the positions within 7 moves written out | 1400 | 6.9 s |
| inner loop without the checks and copies it did not need | 1040 | 4.7 s |
| positions with listed ways looked up 64 at a time | 1040 | a quarter less, measured side by side under load |
| moves that lead nearer listed for the positions at 8 moves | | 65 s to 55 s on 30 cubes, all cores |

- The number of ways was the same to the last one after each step
  (6,736,006 and 11,081,968 for the two cubes; 253,215,066 for the 30):
  that is how each step was checked, beside every solution being applied.
- Most of what is left is waiting for memory: the table entries read to
  find which moves lead nearer from the positions 9 and more moves from the
  subgroup, where nothing is listed.

Tried and dropped:

- Turning moves away with the two small tables (twist with slice, flip
  with slice) before asking the large one: more instructions, no fewer
  entries read.
- Walking the near positions move by move with their listed moves, one
  list entry per position: correct, but a way is a chain of six or seven
  such positions, each waiting for memory in turn. Writing the ways out
  replaced it.
- The class of a flip and slice in two steps from tables of half a
  megabyte together (a symmetry for the slice, then the class of the flip
  it leaves) instead of one table of 4 MB: the same ways, a tenth more
  processor time (201 to 214 against 183 to 194 thousand million cycles
  on 30 cubes). The 4 MB table is not what waits.

## Every size: the 3x3x3 that is left, with the new search (2026-11-12, branch `three-shorter`)

Every larger solver ends with many reduced cubes and wants the shortest
total of the moves spent on one and its 3x3x3. They did a quick two-phase
solve of each on one core, then a longer one of the most promising. Now
they call `Two3::finish`: every reduced cube with a short first phase,
then, step by step, a longer first phase for fewer of them, each searched
only for a solution that does not make the best total longer. The 554 MB
table of the old search (`phase1-exact.bin`) is no longer read by any
solver.

4x4x4, the stage alone, ways of doing it one after the other on the same
reductions (60 cubes, 160 distinct reductions a cube on average, 8 of them
of the shortest length):

| 3x3x3 stage | Shortest total | Time of the stage |
|---|---:|---:|
| as it was: 10,000 second phases each, 160,000 for the best sixteenth | 40.733 | 884 ms |
| first phase to 10, then 12 for a sixteenth | 40.933 | 55 ms |
| to 11, then 12 for a sixteenth | 40.850 | 88 ms |
| to 11, then 13 for a sixteenth | 40.633 | 547 ms |
| to 11, then 13 for the best 48 (the default now) | 40.517 | 813 ms |
| to 11, 12 for all, 13 for a sixteenth | 40.567 | 780 ms |
| to 11, 13 for a sixteenth, 14 for 4 | 40.500 | 1840 ms |
| to 11, 12 for all, 13 for a sixteenth, 14 for 8 | 40.417 | 3184 ms |

On 40 other cubes the exact search of `--mode full` on top (solutions of up
to 16 moves, 60 million positions) took the total from 41.000 to 40.800 in
5.3 s more, and nothing with 5 million: for the time, deeper two-phase
searches of the best reductions give more.

5x5x5, 40 cubes, no clock (`--time 0`), the whole solve:

| 3x3x3 stage (quick solve of all, then) | Length | 3x3x3 part | Seconds |
|---|---:|---:|---:|
| as it was: 5000 second phases for the best 100 | 67.875 | 17.15 | 0.45 |
| first phase to 11 for 100 | 67.950 | 17.60 | 0.42 |
| to 12 for 100 | 67.650 | 17.32 | 0.54 |
| to 12 for 300 | 67.550 | 17.20 | 0.60 |
| to 13 for 100 | 67.225 | 16.98 | 2.01 |
| to 11 for 100, 13 for 12 | 67.375 | 17.12 | 0.68 |
| to 11 for 100, 12 for 30, 13 for 8 (the default now) | 67.300 | 16.95 | 0.64 |
| to 12 for 100, 13 for 8 | 67.275 | 17.05 | 0.70 |
| to 11 for 100, 13 for 24 | 67.250 | 17.10 | 0.89 |

(The parts before the 3x3x3 differ from row to row because the solution is
chosen by its total.)

Old and new binaries one after the other on the same cubes, default modes
unless it says, other sessions running at times:

| Size | Cubes | Before | Now |
|---|---:|---|---|
| 4x4x4 | 60 | 40.68 in 2.53 s | 40.45 in 2.61 s |
| 4x4x4 fast | 60 | 41.48 in 0.31 s | 41.32 in 0.27 s |
| 5x5x5 | 60 | 67.87 in 0.45 s | 67.23 in 0.62 s |
| 5x5x5, no clock | 40 | 67.95 in 0.45 s | 67.22 in 0.63 s |
| 5x5x5 fast, no clock | 40 | 68.83 in 0.24 s | 68.58 in 0.18 s |
| 6x6x6 | 8 | 108.88 in 6.95 s | 108.25 in 6.93 s |
| 7x7x7 | 8 | 153.25 in 3.62 s | 152.75 in 3.43 s |
| 8x8x8 | 5 | 231.20 in 11.9 s | 232.60 in 10.0 s |
| 9x9x9 | 5 | 314.40 in 2.78 s | 313.80 in 2.65 s |

The 8x8x8 is the one that came out longer; on five cubes that is within
what two runs of the same even size differ by (see the release measured
twice above). The full sets are in the table of the release.

Calls with a short first phase run on one core: a search to 10 moves is
0.2 ms, where waking the other cores for it cost several times that.

Then every size on the fixed sets with `scripts/release.py measure v0.1.1
--minor` (commit 03710a2; the 3x3x3 row again on bdd6b44, after its table
was made with less memory), the machine idle. A minor release takes the
place of the newest row in the tables of the README, so the figures of
v0.1.0 are kept here:

| Size | Moves, v0.1.0 | Moves, v0.1.1 | Seconds, v0.1.0 | Seconds, v0.1.1 |
|---:|---:|---:|---:|---:|
| 2 | 8.79 | 8.79 | 0.00 | 0.00 |
| 3 | 19.03 | 18.13 | 0.22 | 0.20 |
| 4 | 40.71 | 40.44 | 2.76 | 2.62 |
| 5 | 67.75 | 67.30 | 0.58 | 0.65 |
| 6 | 108.85 | 107.90 | 8.40 | 7.55 |
| 7 | 152.75 | 152.25 | 3.81 | 3.59 |
| 8 | 235.65 | 233.75 | 12.88 | 12.59 |
| 9 | 314.65 | 313.90 | 3.67 | 3.63 |
| 10 | 406.05 | 399.55 | 16.55 | 12.78 |
| 11 | 510.40 | 509.60 | 7.47 | 5.94 |
| 12 | 619.80 | 611.20 | 21.68 | 20.30 |
| 13 | 757.80 | 757.60 | 15.84 | 13.68 |
| 14 | 888.40 | 871.20 | 30.38 | 18.94 |
| 15 | 1031.80 | 1031.80 | 21.45 | 16.79 |
| 16 | 1185.60 | 1194.80 | 40.34 | 27.20 |
| 17 | 1392.00 | 1391.80 | 36.98 | 33.82 |
| 18 | 1548.00 | 1534.60 | 45.82 | 38.28 |
| 19 | 1763.80 | 1763.00 | 49.50 | 46.24 |
| 20 | 1938.60 | 1922.40 | 56.52 | 56.50 |
| 21 | 2254.00 | 2248.20 | 84.15 | 68.15 |

Shorter on every size from 3 up but two: the 15x15x15 is the same and the
16x16x16 is 9 moves longer on its five cubes, an even size, where two runs
of the same code differ by that much. The times are the same or lower,
but for the 5x5x5 (0.58 s to 0.65 s); sizes 4 to 7 hold about 0.4 GB less,
the table of 554 MB being gone.

## Any size: why the even cubes took three times as long, and the last moves of a matching (2026-11-14, branch `even-faster`)

An 8x8x8 took 12.6 s where a 9x9x9 took 3.6 s. Step by step on the first
cubes of each set (`CUBEN_TRACE=1`), the 8x8x8 spent 5 to 18 s matching its
one row of centres (`src/rings.rs`), 3 s staging, 1.7 s on the innermost
ring as a 6x6x6 and 2.5 s on the edges; the 9x9x9 matched its two rows in
1.7 s.

The row of an even cube that is matched with all its columns in one search
has four centres, that of an odd cube three. Of the eight choices of sign,
tried one after the other, a beam of 30,000 states arrived for one in four
or five on a row of four (0.5 s when it did, 1.5 to 2 s when it did not),
and for three in four on a row of three. Which ones arrive does not follow
the starting score or the width (100,000 states: three in ten, and not the
same ones).

What a search that does not arrive ends on: from about its 30th level to
its 60th every state it holds has the score 1, one pair of centres of one
column on one axis that a single separating turn (the half turn exactly as
deep as the row) would exchange. That turn exchanges the centres of the row
in every column and on both axes it cuts, so it undoes the rest; the score,
a sum over the columns, cannot see that. From nine such states an
exhaustive search found the way every time: 3 to 5 moves that part nothing,
then the turn.

So the matching of a short row is now (`Ring::whole`):

- one beam search per choice of sign, all eight a level at a time;
- on every level, the states with at most two pairs of centres left (48 per
  choice) are tried for an ending: up to 5 moves that part nothing and one
  separating turn, searched exhaustively with, per column, axis and turn, a
  table of the moves that column needs by itself (`Ending`);
- the shortest matching of all choices is kept, and the searches stop one
  level after the first.

Two pairs, because on one or two of the twenty 8x8x8 cubes no search ever
holds a state with one pair left, whatever the sign: there the last turn
exchanges two pairs in two columns at once. (Random staged cubes made with face turns and
half turns from the solved cube, as `rowtest` makes them, never showed
this; `rowtest --states` takes cubes from a file.)

The states of a short row are kept as ranks (24 bytes where the general
state is 192), and the scores of all the moves from a state are the sum of
one row of a table per column and axis.

Row of four centres, random staged cubes (`anycube rowtest --size 8 --row 1`):

| | Matched in one search | Moves | Seconds |
|---|---|---:|---:|
| Before: the choices one after the other (12 rows) | 8 of 12, the rest from the middle outwards | 32.8 | 5 to 18 on the cubes of the set |
| Now (60 rows) | 60 of 60 | 28.6 | 0.7 |

What the numbers of the new search are worth, 60 rows of four, moves:

| Changed | Values | Moves |
|---|---|---|
| Pairs left where an ending is tried | 1, **2**, 3 | 30.8, 28.6, 27.8 (a third more time) |
| Moves of an ending before its turn | 3, 4, **5**, 6 | 30.6, 29.6, 28.6, 28.5 (a third more time) |
| States tried per choice and level | **48**, 200 | 28.6, 28.4 |
| Levels after the first matching | 0, **1**, 2, 4 | 28.7, 28.7, 28.6, 28.6 |
| States per level (in an earlier form of the search) | 2,000, 5,000, 10,000, **30,000** | 38.4, 38.2, 34.4, 29.2 |
| Choices dropped when behind by so many half moves | 4, 6, 8, 12, **never** | 30.0, 29.0, 29.0, 28.8, 28.6 |

A row of five centres (odd cubes from 9x9x9) is searched the same way now:
18 of 20 rows in one search, 42.6 moves, where the way from the middle
outwards took 54.7. A row of six is not: 1 or 2 rows in 10.

The innermost ring of an even cube, as a 6x6x6 (`Solver6::inner_joint`):
one beam search from up to 64 splits of the wings at once, before the six
searches of one split each. On the 20 cubes of the 8x8x8 set:

| | Found on | Moves | Seconds |
|---|---|---:|---:|
| Six searches, 100,000 states each (as before) | 20 of 20 | 23.05 | 1.3 |
| One search from 6 splits, 100,000 states | 16 | 23.50 | 0.20 |
| One search from 16 splits, 200,000 states | 17 | 23.00 | 0.36 |
| One search from 64 splits, 200,000 states (now, the six searches where it finds nothing) | 17 | 22.71 | 0.34 |

A beam that finds nothing there ends, as its comment says, one 3-layer half
turn short with every pair of obliques made: the same kind of ending as the
rows have, not yet searched exactly.

Whole solves on the fixed sets (`anycube row`), the binary of `origin/main`
(e6ba6b1) and the new one run one after the other for each size, nothing
else running. Of an 8x8x8 the 3.7 s are staging 1.7, the row 0.7, the
innermost ring 0.6, edges 0.7.

| Size | Cubes | Moves before | Moves now | Seconds before | Seconds now | Slowest before | Slowest now |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 | 20 | 233.50 | 228.10 | 13.10 | 3.82 | 19.83 | 7.00 |
| 9 | 20 | 313.90 | 301.40 | 4.10 | 4.15 | 12.62 | 12.90 |
| 10 | 20 | 401.20 | 396.35 | 13.07 | 6.22 | 26.81 | 14.29 |
| 11 | 5 | 509.60 | 503.80 | 6.28 | 6.59 | 11.12 | 11.70 |
| 12 | 5 | 619.40 | 608.20 | 19.66 | 11.13 | 29.27 | 16.24 |
| 13 | 5 | 757.60 | 743.20 | 15.56 | 15.56 | 22.85 | 24.04 |
| 14 | 5 | 867.00 | 875.60 | 23.29 | 16.35 | 27.27 | 22.24 |

The even sizes take a half to a third of the time. The odd sizes take the
same and are 6 to 14 moves shorter, by the row of five. The 14x14x14 is 9
moves longer on its five cubes, which is what two runs of one version
differed by on an even size (its staging was not repeatable; see the
next section).

What an even cube still pays that an odd one does not: the first step of
its staging is the exact search of the 6x6x6 (0.5 to 1 s, three cubes of
nine moves each), its second axis starts with a beam search for each
candidate where the 7x7x7 steps have one for all, and the innermost ring
takes 0.6 s. The 4x4x4 and the 6x6x6 have solvers of their own and are as
they were: the 4x4x4 runs to its clock of 4 s, and two thirds of a 6x6x6
are the 5x5x5 searches of its 64 candidates.

## The same cube, the same solution; 4x4x4 and 6x6x6 in less time (2026-11-14, evening, branch `even-faster`)

Two runs of the same cube gave different solutions: every 8x8x8 cube, one
4x4x4 in three, one 5x5x5 in five (with or without its clock). Found by
printing a fingerprint of the moves after every step and comparing runs.
Five things, all of one kind (threads sharing a counter, and the result
being what they had reached when it ran out), and two clocks:

- the first step of the 6x6x6 staging, with which every even cube from
  8x8x8 up begins, and the split of the inner wings (6x6x6, and the
  innermost ring of the even cubes) stopped every thread at the first so
  many solutions found anywhere. They now return the first in the order of
  their jobs, the jobs started in that order (`search::InOrder`): many
  small jobs in a mixed order, so that the solutions still differ from the
  first move on. (Taking the first in the plain order of the search made
  the 8x8x8 4 moves longer on six cubes: its solutions were all alike.)
- the cap of the 5x5x5 edge phases: the same.
- the staging of the 5x5x5 kept a staging if it was within 3 moves of the
  shortest known when it was found. Now: within 3 of the shortest of all.
- a level of that staging that turned out larger than planned ended at a
  count of positions shared by the threads, on one cube in five. The count
  is now kept in the order of the jobs (`search::in_order_until`).
- the 3x3x3 of many reduced cubes (`Two3::finish`): each cube of a step was
  searched for a solution within the best total of the moment, so the
  cubes handed to the next step differed. Only the solutions that reach
  the best total of their step are kept, which are found on every run.
- the 4x4x4 ran by a clock (4 s, shared out between its phases) and the
  5x5x5 had one (4 s, rarely reached). Neither has one now.

Two runs of the new code, every solution compared move for move: 0 of 100
4x4x4, 0 of 200 5x5x5, 0 of 20 each of 6x6x6, 7x7x7, 8x8x8, 9x9x9 and
10x10x10 differ.

### 4x4x4

Where its 2.7 s went (60 cubes): joint phase 1.56 s, always to its share
of the clock, 54 cubes in 60 (64 million positions, 6300 endpoints of the
10,000 it may collect); phase 3 0.33 s; the 3x3x3 of the reductions 0.91 s
(48 searches to a first phase of 13, see the section on the 3x3x3 above).

The joint phase by itself, on a fixed piece of work (all sequences of up to
13 moves for 8 cubes, 468 million positions): 49 million positions a second
before, 61 million now. Where the wings are along the path is kept from one
sequence to the next (it was worked out from the first move for every
sequence one move from its end), and the entries of the exact centre table
for all next moves are asked for before the first is read. Asking for the
entries of the small near table ahead as well changed nothing.

What ends the joint phase is now a budget of positions, counted in a fixed
order of its jobs (one job for every two first moves, in a mixed order), so
the endpoints are the same on every run and every machine. 100 cubes, no
clock anywhere:

| Joint phase, million positions | Moves | Seconds | Slowest |
|---:|---:|---:|---:|
| 1 (60 cubes) | 41.40 | 0.74 | 1.6 |
| 10 (60 cubes) | 40.73 | 1.24 | 2.1 |
| 20 | 40.59 | 1.47 | 3.1 |
| **30** | **40.55** | **1.66** | 3.1 |
| 45 | 40.52 | 1.90 | 3.5 |
| 64 | 40.46 | 2.27 | 8.6 |
| 150 (60 cubes) | 40.45 | 3.00 | 7.7 |
| before: the clock | 40.42 to 40.44 | 2.70 to 2.81 | 4.1 to 4.8 |

Every doubling is worth about 0.05 moves. The default is 30 million: a
tenth of a move more than before in 1.7 s for 2.7 s. `--joint-nodes 64`
is what the clock used to give, in 2.3 s. `--mode fast`: 41.37 in 0.28 s
(41.42 in 0.29 s). `--mode full` has a budget of 400 million positions
where it had 8 s: 40.00 on 8 cubes both ways, 14.5 s for 16.4 s.

### 6x6x6

Where its 7.6 s went (3 cubes): staging 0.7 to 1.3 s, the inner reduction
2.9 s (twelve beam searches: six stagings, two splits each), then the
5x5x5 of the candidates: phase P 1.1 to 2.1 s, phase E 1.2 to 1.6 s, the
3x3x3 0.5 to 0.8 s.

The inner reduction as one beam search for all the stagings and their
splits (`Solver6::inner_joint_many`), each as many moves behind as it is
longer than the shortest, the twelve searches where it finds nothing. 20
cubes:

| Inner reduction | Moves | Seconds |
|---|---:|---:|
| twelve searches of 100,000 states (as before) | 107.90 | 7.56 |
| one of 100,000, 8 splits a staging, 16 results of two lengths | 108.00 | 6.00 |
| one of 200,000, the same | 107.75 | 5.01 |
| one of 300,000, the same | 107.80 | 5.68 |
| one of 200,000, 16 splits a staging | 107.90 | 5.36 |
| **one of 200,000, 32 results of three lengths** | **107.65** | **5.21** |

In `--mode fast` it does not pay (110.75 in 3.3 s with 100,000 states,
111.30 in 2.5 s with 50,000, against 110.80 in 3.3 s) and is left out.

### Every size, old and new one after the other, nothing else running

| Size | Cubes | Moves before | Moves now | Seconds before | Seconds now |
|---:|---:|---:|---:|---:|---:|
| 4 | 100 | 40.42 | 40.55 | 2.70 | 1.75 |
| 5 | 200 | 67.28 | 67.34 | 0.85 | 0.70 |
| 6 | 20 | 107.85 | 107.65 | 8.46 | 5.9 |
| 7 | 20 | 152.25 | 152.25 | 3.54 | 3.5 to 3.9 |
| 8 | 20 | 227.95, 228.50 | 227.35 | 3.8 to 4.0 | 3.8 to 4.5 |
| 9 | 20 | 301.40 | 301.40 | 4.3 | 4.7 |
| 10 | 20 | 397.20 | 400.40 | 7.9 | 7.2 to 8.7 |

("Before" is the branch as of this morning's section, which had the
8x8x8 at 3.8 s already.) The seconds of sizes 7 to 10 moved by a fifth
between two runs of the same binary that evening, so their two columns are
the same. The 10x10x10 is 3 moves longer on its 20 cubes, the 8x8x8 a move
shorter: other solutions of the first staging step, no better or worse on
the whole.

A mistake to remember: for most of the day the load average of 10 to 16
was taken for other sessions' work. It was this session's own benchmark,
which keeps all ten cores busy by itself, and twice two of its own jobs
running at once.


## 5x5x5: what more time buys, and why (2026-11-14, night, branch `five-tail`)

Asked: what would take 10 to 20% off the 5x5x5. Nothing found does; this
section is the measurement of what the present solver gives for more time,
so that the question need not be asked of it again. No code of the solver
changed (a trace of the first centre phase was added, `CUBE5_TRACE`).

### The bound, counted exactly

The bounds in the sections above divide the logarithm of the positions by
the logarithm of the number of moves (36 for the 5x5x5), which counts the
same cube several times: two turns of one axis in either order, a turn and
its continuation. Counting only sequences that are not one another
rewritten (the growth is 20.73 a move for the 4x4x4 and 28.12 for the
5x5x5, as in the section on the structure above):

| Size | Positions | Bound, moves | Default mode | Ratio |
|---:|---:|---:|---:|---:|
| 3 | 4.33e19 | 18 | 18.13 | 1.01 |
| 4 | 7.40e45 | 35 | 40.55 | 1.16 |
| 5 | 2.83e74 | 52 | 67.34 | 1.29 |
| 6 | 1.57e116 | 75 | 107.65 | 1.44 |
| 7 | 1.95e160 | 99 | 152.25 | 1.54 |

A random cube is about that far and no nearer: So Hirata's estimate
(arXiv 2404.07337, growth rates 20.67 and 28.08) has nearly all positions
reached at 36 moves for the 4x4x4 and at 53 for the 5x5x5, and puts God's
number at 41 and 58. So 10% off the 5x5x5 (60.6) would put it where the
4x4x4 is, and 20% off (53.9) is the optimum.

For comparison, the shortest published solutions by a learned search
(CayleyPy, arXiv 2502.13266, beams of 2^24 states): 46.51 on the 4x4x4 and
92.16 on the 5x5x5 in single-layer quarter turns with nothing fixed, where
the same count gives bounds of 37 and 55: 1.26 and 1.68 times the bound.

### Where a second goes

`CUBE5_TRACE=1`, first cubes of the set, default mode, wall time and
processor time:

| Step | Wall | Processor |
|---|---:|---:|
| centres (1200 stagings) | 0.18 s | 1.6 s |
| phase C | 0.07 s | 0.6 s |
| phase P | 0.04 s | 0.3 s |
| phase E | 0.05 s | 0.4 s |
| 3x3x3 of the reductions | 0.17 to 0.34 s | 1.5 to 2.6 s |

The first centre phase visits about a million positions a second on one
core at its long lengths (20 million positions of length 13 in 20 s of
processor on the third cube): every position looks up its thirty or so
neighbours, the survivors once more in the table near the goal (64 MB and
256 MB, by their smallest image under 16 symmetries). The reductions at
the shortest length are few (1 to 13 on these cubes), a hundred at the
next, and the 3x3x3 picks among them.

### What more time buys

No clock in any of this; seconds are of one run each, the machine doing
nothing else. First 40 cubes:

| Settings | Moves | Seconds |
|---|---:|---:|
| default | 67.45 | 0.66 |
| `--c-keep 20000` | 67.35 | 0.74 |
| `--p-keep 2000 --e-keep 400` | 67.48 | 1.08 |
| `--pool 400 --deeper 12:100,13:24` | 67.38 | 1.38 |
| `--deeper 12:100,13:24,14:4` | 67.33 | 2.36 |
| `--a-budget 10000000 --staged 4000` | 67.10 | 2.20 |
| `--extra 4 --a-budget 3000000 --staged 4000` | 67.18 | 1.92 |
| the last but one with `--c-keep 20000 --deeper 12:100,13:24` | 66.93 | 3.30 |
| `--mode full` | 66.73 | 3.86 |

All 200 cubes (the default took 0.85 s in this run, 0.70 s when it was
recorded):

| Settings | Moves | Seconds |
|---|---:|---:|
| default | 67.335 | 0.85 |
| `--c-keep 20000` | 67.235 | 0.82 |
| `--c-keep 60000` | 67.185 | 0.85 |
| `--c-keep 20000 --labellings 32` | 67.235 | 0.80 |
| `--c-keep 20000 --staged 2400` | 67.180 | 0.79 |
| `--c-keep 60000 --staged 2400` | 67.110 | 0.94 |
| `--c-keep 60000 --staged 2400 --a-budget 3000000` | 66.915 | 1.47 |
| `--c-keep 20000 --staged 2400 --a-budget 3000000` | 66.985 | 1.39 |
| the same without the step `13:8` of the 3x3x3 | 67.170 | 1.19 |
| the same with `--p-keep 400 --e-keep 60` | 67.055 | 1.44 |
| `--mode fast` | 68.590 | 0.17 |
| `--mode fast --c-keep 8000` | 68.555 | 0.19 |

- There is no knee. From the default to the full mode the time is 5.8
  times as long and the solutions 0.73 shorter: 0.28 moves for every
  doubling, whichever widths the time is spent on. Read as chance: the
  solutions of a cube are draws from a spread of about one move (the
  first 40 cubes are 65 to 69 long), and twice as many draws reach a
  little further into its tail. By that reading six moves off would be
  the best of some 10^10 times as many.
- The widths of P and E give nothing, as on 2026-11-11. What gives most
  per second is the first centre phase (its budget and the stagings kept),
  and it is what costs: a second and a half for 0.35.
- One thing is nearly free: three times as many results of phase C
  (`--c-keep 20000`). Old and new binary one after the other, three times,
  first 100 cubes: 67.45 in 0.675 s and 67.34 in 0.705 s; each change by
  itself on the same cubes: twice the stagings 67.45 in 0.72 to 0.75 s
  (nothing), both 67.32 in 0.763 s. **The default mode now keeps 20,000
  results of phase C**: a tenth of a move for 30 ms. Nothing that doubles
  the time was taken into the default.
- The larger cubes do not gain from it, and why: their 5x5x5 steps get
  the centres staged, one staging where a 5x5x5 of its own has 1,200, so
  phase C has nothing more to choose from. With 20,000 there as well, 20
  cubes each: 7x7x7 152.25 (the same), 9x9x9 301.25 against 301.40, 8x8x8
  227.10 against 227.35 in 4.27 s against 3.95 s. They keep 6,000 (the
  7x7x7 outer cube and the general solver name it now), and sizes 6 to 9
  give the same lengths as before on every cube compared.

### Why the centres in one step would not pay either

The solutions chosen have centres of 18.1 moves where the shortest
staging in hand is 16.75 (40 cubes): the shortest staging does not win,
because there are few of it. Stagings of length L in one exact search
number about 28^L over the 9e19 states of the centres: 2 at 14 moves, 56
at 15, 1,600 at 16, 44,000 at 17. To be chosen from like the 1,200 of
today they are needed by the thousand, so two lengths past the optimum,
and one length is 28 times the work of the one before (340 s for the
optimum alone, recorded above). A staging that is shorter but the only
one of its kind loses in the phases after it what it won.

### Cubes a known number of moves from the end of phase C

`anycube known5 --walk K --base split`: a cube as phase C leaves it
(60 random moves of the 20 that keep the wing halves apart), then K
random moves of all 36, the first of them not one of the 20. A way of K
moves to the end of phase C exists. 12 cubes each, default mode, the
shortest way to the end of phase C that the solver has (`CUBE5_TRACE`):

| Walk | Exists | Shortest found |
|---:|---:|---:|
| 14 | 14 | 16.50 |
| 17 | 17 | 21.25 |
| 20 | 20 | 22.17 |
| 23 | 23 | 22.25 |
| random cubes (40) | about 20 by the count (19.3) | 23.20 |

At 23 moves the walk is no longer the short way, so a random cube is
about 20 to 21 moves from the end of phase C and the three phases find 23.2
(and the solution chosen spends 25.2 there, for the choice it leaves the
later phases). These first moves are where the solver is furthest from
what exists, 2 to 3 moves, and they are the only stretch with all 36 moves
in which a search could go on a learned estimate: it is a problem of
65 nats (the logarithm of the count), the 3x3x3 is one of 45, and for
that size learned searches are close to optimal. Not tried here.

## 5x5x5: the centres by a beam search on a learned estimate (2026-11-15, night, an experiment)

Asked: can a learned estimate of the distance lead a search with all 36
moves to stage the centres in fewer moves than the two centre phases do,
the first step towards the 2 to 3 moves the section above found lost
before the end of phase C. Success was set at a shortest staging under
16.0 on the first 40 cubes of `scrambles555-200.txt` with a few hundred
different stagings one move longer; failure at more than 16.5 with a
beam of a million.

**Verdict: the search works about as well as anything can here, and
there was little to find; it is not kept, because the two phases get
as far for less.** What was measured last matters most: the shortest
staging is not 14 or 15 moves, as the count and the one cube searched
exactly had it, but 16 on most cubes. An exact search for stagings of
at most 15 moves finds two on the first cube and proves that the next
seven have none. On those eight cubes the shortest there is lies between
15.88 and 16.13; the two phases of the default mode are at 16.63 and
the beam of a million at 16.25, within a quarter of a move.

On all 40 cubes the beam of a million finds 16.325 where the two phases
find 16.750, and takes 118 s a cube; the two phases with a hundred times
their positions find 16.275 in 22 s. Of the three things that could
have been too small (the curves are below):

- the network was not: from 0.3 to 14.4 million weights the error for
  each K is the same to 0.1 of a move, the beam is no shorter at the
  same width, and the cubes scored a second fall from 1.4 million to
  51 thousand;
- the data was not: a quarter of the records gives the same network, at
  2 million weights and at 14 million;
- the width was, up to a million, and is no longer: the staging gets
  shorter by 0.3 moves for every three times the width up to there, but
  at a million it is a quarter of a move from the shortest there is, and
  the three cubes run at three million gave the lengths they gave at a
  million. This Mac goes to about ten million (5.3 GB and 6 minutes a
  cube at three); it is the cube that ends the curve, not the machine.

So neither "the idea fails" nor "the try was too small": it finds what
is there, and what is there is a third of what was thought. The
condition set at the start, under 16.0, no search can meet on these
cubes. The second step (the same to the end of phase C) was not
started, and its premise needs the same check first: the count that
puts a random cube 19.3 moves from the end of phase C is the count that
put the centres at 14. Nothing in any mode changed; what stays is the
instrument: `src/learned5.rs`, `ml/centres5_net.py` and the commands
`centres5-dump`, `centres5-walks`, `centres5-beam` and `centres5 --max`.

Staging the centres is a problem of 1.8e20 states (46.6 nats): by the
count 14 moves, 15 on the one cube searched exactly (340 s, above),
and 16 on most cubes (below).
Today, the same 40 cubes (`centres5-beam --today`): shortest staging
16.750 (one of 15, fourteen of 16, nineteen of 17, six of 18), 13.0
different stagings of that length and 175.3 one longer (median 76,
least 2) among the 1,200 kept, 0.24 s a cube.

### The label K says little here

As asked first: cubes K random moves from staged centres, K from 1 to
16, no move undoing or continuing the one before; a residual network of
305,409 weights (153 inputs, 256 wide, two blocks) sees the axis of the
centre at each of the 48 positions, the parity of the wings and eight
exact distances (each orbit staged alone, from `centres5`; for each
axis and orbit its eight centres brought onto their faces), and is
trained to say K: 30 passes of 8 million fresh cubes, 6.5 minutes on
the graphics processor. On 1.6 million other cubes:

| K | Estimate | Mean error | | K | Estimate | Mean error |
|---:|---|---:|---|---:|---|---:|
| 1 | 2.72 ± 0.08 | 1.72 | | 9 | 9.48 ± 2.48 | 2.06 |
| 2 | 3.16 ± 0.63 | 1.16 | | 10 | 10.37 ± 2.48 | 2.04 |
| 3 | 3.83 ± 1.05 | 1.06 | | 11 | 11.11 ± 2.38 | 1.90 |
| 4 | 4.63 ± 1.41 | 1.32 | | 12 | 11.73 ± 2.21 | 1.67 |
| 5 | 5.52 ± 1.71 | 1.54 | | 13 | 12.24 ± 2.01 | 1.45 |
| 6 | 6.46 ± 1.95 | 1.69 | | 14 | 12.65 ± 1.81 | 1.49 |
| 7 | 7.44 ± 2.15 | 1.82 | | 15 | 12.96 ± 1.63 | 2.04 |
| 8 | 8.48 ± 2.35 | 1.98 | | 16 | 13.23 ± 1.44 | 2.77 |

It cannot say 1 for K = 1, and the reason is in the moves: 24 of the 36
leave staged centres staged (the face turns and the half turns of two
layers), so a cube one random move away is staged two times in three.
With the first move a quarter turn of two layers the near cubes come
out right, and the rest does not (K to 20, since after 16 moves the
walks are not yet mixed: the orbit tables read 8.8 and 8.2 there and
9.0 and 8.6 after 20):

| K | Estimate | Mean error | | K | Estimate | Mean error |
|---:|---|---:|---|---:|---|---:|
| 1 | 1.06 ± 0.01 | 0.06 | | 10 | 11.72 ± 2.57 | 2.56 |
| 2 | 2.26 ± 0.23 | 0.29 | | 12 | 13.35 ± 2.42 | 2.35 |
| 4 | 4.36 ± 0.73 | 0.61 | | 14 | 14.44 ± 2.16 | 1.81 |
| 6 | 6.53 ± 1.22 | 1.04 | | 16 | 15.19 ± 1.88 | 1.47 |
| 8 | 9.15 ± 1.96 | 1.85 | | 20 | 16.08 ± 1.44 | 3.92 |

A network of 1,135,105 weights (512 wide, 40 passes) has the same
errors to 0.05. They are not the network's: a cube K moves away is
rarely K moves away (`centres5-walks`: 200 cubes for each K, each
searched exactly with the tables as bounds; 100 for each K above 11,
searched to 11 moves):

| K | Mean distance | As far as K | | K | Within 11 moves |
|---:|---:|---:|---|---:|---:|
| 3 | 2.88 | 88% | | 12 | 54% |
| 5 | 4.60 | 71% | | 13 | 44% |
| 7 | 6.39 | 68% | | 14 | 21% |
| 9 | 7.92 | 48% | | 15 | 20% |
| 10 | 8.99 | 52% | | 16 | 10% |
| 11 | 9.81 | 45% | | | |

Half the cubes are nearer than their label, by as much as seven moves,
and one cube in ten is within 11 moves after a walk of 16. The beam on
these networks (`centres5-beam`, width 10,000, stagings of up to 20
moves): on the walks of any moves no staging on 12 of the 40 cubes and
18.79 on the rest; on the walks that start with a wide quarter turn none
on 10 and 18.27 on the rest.

### Exact distances as labels

The search that measured the walks is cheap as far as 11 moves: 0.1 to
11 ms for a cube that near, 20 to 50 ms to know that a cube is further.
`centres5-dump --exact 11` writes, for walks of 8 to 16 moves, the exact
distance if it is at most 11, and every cube on the way of the staging
found with the moves it has left; a cube further than 11 after a walk
of 12 is at 12, after a longer walk it is trained as 13. 400,000 walks
gave 2,277,952 records in a quarter of an hour. Each record is turned by a random
one of the 48 symmetries of the cube every time it is used (the
distances are the same; `centres5-dump --syms`, checked in both
programs). The network above (256 wide, trained on K first), 8 passes,
on the 45,559 records held out:

| Distance | Estimate | Mean error | Before a far cube |
|---:|---|---:|---:|
| 5 | 5.13 ± 0.34 | 0.23 | |
| 6 | 6.31 ± 0.54 | 0.48 | |
| 7 | 7.46 ± 0.65 | 0.68 | |
| 8 | 8.40 ± 0.76 | 0.69 | 98.7% |
| 9 | 9.42 ± 1.15 | 0.98 | 91.4% |
| 10 | 10.16 ± 1.23 | 1.05 | 82.1% |
| 11 | 10.63 ± 1.22 | 1.05 | 73.8% |
| 12 | 11.40 ± 1.03 | 0.89 | 57.2% |
| more than 11, walk of 14 | 11.55 ± 1.04 | | |
| more than 11, walk of 16 | 11.76 ± 0.96 | | |

The last column is what a search needs: how often a cube at that
distance gets a lower estimate than a cube more than 12 away (one
further than 11 after a walk of 13 to 16). It is where everything
tried ends up:

| Estimate | 9 | 10 | 11 |
|---|---:|---:|---:|
| largest of the eight table distances | 80.1% | 69.3% | 62.2% |
| their sum | 84.0% | 75.2% | 68.3% |
| network trained on K, 256 wide | 91.0% | 82.2% | 74.4% |
| the same, 512 wide | 92.1% | 83.4% | 75.3% |
| exact labels, 256 wide (the one searched with) | 91.4% | 82.1% | 73.8% |
| on a sixteenth of the records | 91.3% | 82.0% | 73.9% |
| on a quarter of them | 91.5% | 82.2% | 74.0% |
| 512 wide (on the first 1.9 million records) | 91.9% | 82.8% | 74.0% |
| 256 wide, four blocks, exact labels only | 89.8% | 80.0% | 72.5% |
| the mean over the 48 images of the cube | 91.9% | 82.7% | 74.3% |

- The exact labels set the scale right (the network trained on K says
  12.9 for a cube at 10) and by this measure order the cubes no better.
  The search says otherwise: at a width of 100,000 the network trained
  on K alone gives 17.675 and the one with exact labels 16.925. What
  the labels mend is the order among the near cubes (at 8 moves the
  estimates are 1.5 apart with K and 0.8 with exact labels), which this
  column does not look at.
- The data is not what is missing: a sixteenth of it gives the same.
  Nor is it noise: the 48 images of a cube get estimates 0.2 apart, and
  their mean orders no better.
- Four times the weights buy one point, and a search with them scores a
  third as many cubes in the same time.
- So the network is the tables and seven points: it does not find in the
  positions what a search of ten thousand positions finds.

### The beam

`centres5-beam --net`: level by level the cubes are made with all moves,
the same cube kept once, each scored by the larger of the network and
its exact distances, the best kept. A cube with one axis in place is
also ended by the exact table of the second centre phase, and what
cannot end within a move of the shortest staging found is dropped by
the exact distances. With exact labels, 40 cubes, each staging made on
the cube; in brackets the count when the last seven moves are made in
every way by exact search from the cubes the beam kept (free: under a
tenth of a second):

| | Shortest | Of that length | One longer | Seconds a cube |
|---|---:|---:|---:|---:|
| two phases, default mode | 16.750 | 13.0 | 175.3 (median 76) | 0.24 |
| two phases, ten times the positions, 4,000 kept | 16.525 | 9.5 | 171.2 (median 75) | 1.40 |
| two phases, a hundred times, one length further | 16.275 | 16.0 | 234.5 (median 133) | 22.48 |
| beam of 10,000 (38 cubes; none within 20 on two) | 18.053 | 5.1 (27.4) | 27.7 (417.7) | 1.97 |
| beam of 30,000 | 17.475 | 4.5 (15.9) | 33.5 (473.3, median 186) | 5.99 |
| beam of 100,000 | 16.925 | 2.9 (10.6) | 20.9 (326.7, median 150) | 14.99 |
| beam of 300,000 | 16.650 | 3.0 (10.6) | 23.4 (297.0, median 154) | 52.11 |
| beam of 1,000,000 | 16.325 | 2.9 (9.5) | 23.3 (245.9, median 174) | 118.16 |

Lengths at a million: three cubes of 15, twenty-two of 16, fourteen of
17, one of 18. It scores 1.4 million cubes a second on the processor
(the system's matrix product; the graphics processor is no faster for a
network this small), 160 to 190 million a cube: the first four levels
are whole, the fifth keeps one cube in 13, the next five one in 27.

Every ten times the width is a move at first and 0.6 at the end, for
ten times the time. The stagings are there in number once the last
moves are searched exactly, so the second condition is met and the
first is not: 16.325 is neither under 16.0 nor over 16.5. And the two
phases reach the same place sooner: with `--extra 4 --a-budget
100000000 --staged 4000` they are at 16.275 in a fifth of the time,
with more stagings of the shortest length (two cubes of 15, twenty-five
of 16, thirteen of 17).

### How short a staging there is

`centres5 --max 15`: the exact search of the section on the shorter
structure, stopped after the length 15. A cube with no staging that
short takes one whole search of that length: 129 to 445 s on all cores,
227 on average. (Left to find the optimum of the second cube, the same
search had not finished after 20 minutes: a length more is 28 times
the work.) The first eight cubes:

| Cube | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | Mean |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| stagings of 15 moves or fewer | 2 | none | none | none | none | none | none | none | |
| two phases, default mode | 17 | 16 | 18 | 16 | 16 | 16 | 17 | 17 | 16.63 |
| beam of 1,000,000 | 16 | 16 | 17 | 16 | 16 | 16 | 17 | 16 | 16.25 |
| beam of 3,000,000 | 16 | 16 | 17 | | | | | | |
| the shortest there is | 15 | 16 | 16 or 17 | 16 | 16 | 16 | 16 or 17 | 16 | 15.88 to 16.13 |

- The default mode already has the shortest staging there is on four of
  the eight cubes, and the beam of a million on five or more.
- The count is two moves short. It takes every sequence for a different
  cube, and near staged centres most are not: 24 of the 36 moves leave
  them staged, and half the cubes K moves away are nearer than K. The
  section before this one counted 2 stagings of 14 moves and 56 of 15
  for a cube; there are none of 14 on these eight and 2 of 15 on one.
- So the centres lose 0.5 to 0.75 of a move to the optimum in the
  default mode (16.63 against 15.88 to 16.13 here), not the 2 that 14 or
  15 would mean, and the beam of a million wins back 0.4 of it. The beam
  that misses is the first cube's: its two stagings of 15 are not found
  at a million or at three million.

### Too small, or the wrong idea: three curves

The try on the 4x4x4 (2026-11-11) could not tell the two apart. This one
can, for the network and the data; for the width it reaches the limit of
the machine first.

**The network.** Four sizes, each trained on the same 64 million cubes
labelled K (8 passes of 8 million, walks that start with a wide quarter
turn, K to 20), then 8 passes over the same 6,691,056 records with exact
labels (1.2 million walks). Read on 1.6 million other cubes labelled K,
on 340,707 exact records of other walks, and on 200,000 random cubes (a
walk of 60 moves). "Kept" is the share of the cubes at a distance whose
estimate is below that of all but one in 27 random cubes: what a beam
of a million does at its sixth to tenth level.

| Weights | Error for K: mean, root mean square | Kept at 10, 11, 12 moves, trained on K | The same, exact labels | Cubes scored a second | Beam |
|---|---|---|---|---:|---|
| 305,409 (256 wide, 2 blocks) | 1.749, 2.377 | 87.9%, 77.3%, 56.9% | 84.7%, 72.1%, 50.5% | 1,400,000 | 16.80 at 100,000; 18.00 at 10,000 |
| 1,135,105 (512, 2) | 1.734, 2.359 | 89.2%, 79.3%, 58.5% | 86.2%, 74.2%, 52.7% | 470,000 | |
| 2,101,441 (704, 2) | 1.725, 2.348 | 89.6%, 79.5%, 59.9% | 86.8%, 74.8%, 52.7% | 330,000 | 16.90 at 100,000 |
| 14,424,577 (1536, 3) | 1.710, 2.332 | 90.6%, 80.5%, 60.7% | 87.4%, 75.7%, 54.1% | 51,000 | 18.13 at 10,000 |

The beams are on the first 20 cubes at 100,000 and the first 8 at
10,000, the same cubes for both networks of a pair (the small one is
the network of the section above). Forty-seven times the weights move
the error for K by 0.04 and the cubes kept by 3 points; in the search
they give nothing at the same width, and in the same time they search
a width 4 or 27 times smaller, which costs half a move or more than
one. The larger networks are not kept.

**The data.** The exact records, a quarter, a half and all of them, the
passes scaled so that the work is alike (16, 11 and 8); the error is the
mean square on the 136,552 records held out of the training:

| Records | 2.1 million weights: error, kept at 10, 11, 12 | 14.4 million weights: error, kept at 10, 11, 12 |
|---|---|---|
| a quarter (1.6 million) | 0.568; 86.2%, 74.4%, 52.4% | 0.550; 87.3%, 75.6%, 54.8% |
| a half (3.3 million) | 0.564; 86.6%, 74.9%, 52.8% | 0.547; 87.3%, 75.6%, 54.6% |
| all (6.6 million) | 0.553; 86.8%, 74.8%, 52.7% | 0.534; 87.4%, 75.7%, 54.1% |

The error falls by 3% from a quarter to all, the cubes kept not at all.
The labels K are without end (every pass is new cubes) and their error
does not fall with size either: it is the label's, half the cubes being
nearer than K.

**The estimate next to random cubes.** The networks trained on K (the
same table as for the 4x4x4): the estimate for cubes K moves away, and
the share of random cubes that look as near as the median such cube and
as near as nine in ten of them.

| K | 0.3 million weights | Random as near as the median | As nine in ten | 14.4 million weights | Random as near as the median |
|---:|---|---:|---:|---|---:|
| 6 | 6.57 ± 1.25 | none | none | 6.53 ± 1.22 | none |
| 8 | 9.24 ± 2.05 | 0.0015% | 0.04% | 9.18 ± 1.98 | 0.0005% |
| 9 | 10.67 ± 2.53 | 0.007% | 0.65% | 10.61 ± 2.46 | 0.0025% |
| 10 | 11.81 ± 2.63 | 0.06% | 3.1% | 11.76 ± 2.58 | 0.04% |
| 11 | 12.67 ± 2.58 | 0.22% | 7.9% | 12.64 ± 2.55 | 0.14% |
| 12 | 13.40 ± 2.45 | 0.52% | 14.9% | 13.39 ± 2.43 | 0.39% |
| 13 | 13.97 ± 2.31 | 1.1% | 23.5% | 13.98 ± 2.31 | 0.88% |
| 14 | 14.45 ± 2.16 | 2.1% | 32.7% | 14.47 ± 2.16 | 1.7% |
| 15 | 14.84 ± 2.01 | 3.6% | 40.4% | 14.89 ± 2.01 | 3.1% |
| 16 | 15.16 ± 1.88 | 5.5% | 48.1% | 15.22 ± 1.88 | 5.0% |
| 18 | 15.67 ± 1.64 | 10.9% | 60.2% | 15.75 ± 1.64 | 10.0% |
| 20 | 16.02 ± 1.44 | 16.8% | 69.0% | 16.12 ± 1.43 | 16.0% |
| random cubes | 17.01 ± 0.81 | | | 17.12 ± 0.79 | |

And by the distance itself, the small network with both kinds of
label (kept: at one cube in 27):

| Distance | Trained on K | Random as near as the median | Kept | Exact labels | Random as near as the median | Kept |
|---:|---|---:|---:|---|---:|---:|
| 7 | 8.68 ± 1.15 | 0.0005% | 100% | 7.48 ± 0.67 | none | 100% |
| 8 | 10.38 ± 1.51 | 0.0035% | 99.9% | 8.39 ± 0.77 | 0.001% | 100% |
| 9 | 11.98 ± 1.90 | 0.065% | 96.0% | 9.42 ± 1.16 | 0.062% | 95.7% |
| 10 | 13.03 ± 1.92 | 0.25% | 87.9% | 10.13 ± 1.24 | 0.39% | 84.7% |
| 11 | 13.81 ± 1.83 | 0.61% | 77.3% | 10.66 ± 1.20 | 0.95% | 72.1% |
| 12 | 14.74 ± 1.72 | 2.5% | 56.9% | 11.29 ± 1.11 | 3.6% | 50.5% |
| more than 11, walks of 13 to 16 | 15.25 ± 1.60 | 5.1% | 44.4% | 11.60 ± 1.02 | 7.3% | 38.4% |
| random cubes | 17.01 ± 0.81 | | | 12.54 ± 0.45 | | |

Up to 9 moves a cube stands clear of the random ones. At 10 and 11 it
is kept seven to nine times in ten, at 12 one time in two: from 12 moves
on the search is blind, and a staging of 16 has its cube 12 away after
four moves and 11 away where a beam of a million first has to choose.
Against random cubes the labels K separate a little better than the
exact ones (the exact records stop at walks of 16 and are trained as 13
beyond 12 moves), and the search is 0.75 shorter with the exact ones:
what loses the staging is less the first choice among random cubes than
the later ones among cubes that were kept for looking near.

**The width.** In the table of the beam above: 18.05, 17.48, 16.93,
16.65, 16.33 from 10,000 to a million, each step three times as wide:
0.58, 0.55, 0.28, 0.33 shorter. Three million was run on three cubes
(16, 16, 17, as at a million; 444 to 544 million cubes scored, 5.7
minutes and 5.3 GB a cube, where a million is 2 minutes and 3.2 GB), so
this machine would go to about ten million. There is no reason to: on
the cubes whose shortest staging is known the beam of a million is
within a quarter of a move of it, so what is left of the curve is that
quarter. With the network trained on K the width of 100,000 gives
17.675 (the labels are worth more than a factor of three in width).

### In the solver

The stagings of the beam (of the shortest length and one longer) given
to the default mode with its own 1,200, the shortest 1,200 of all kept
(`bench5 --stagings`), and alone (`--stagings-only`); 40 cubes, every
solution checked on the sticker model:

| Stagings | Moves | Centres | Edges | 3x3x3 |
|---|---:|---:|---:|---:|
| the solver's own (default mode) | 67.350 | 18.13 | 32.15 | 17.07 |
| with those of the beam of 100,000 | 67.100 | 17.82 | 32.13 | 17.15 |
| with those of the beam of 1,000,000 | 66.675 | 17.35 | 32.40 | 16.93 |
| only those of the beam of 100,000 | 67.900 | 17.65 | 32.87 | 17.38 |
| only those of the beam of 1,000,000 | 66.800 | 17.10 | 32.77 | 16.93 |

0.68 moves for 118 s a cube. The full mode gets 0.63 for 3 s more
(66.725 in 3.7 s on these cubes, measured again). The shorter stagings do
carry through: the staging chosen is 0.78 shorter and the solution
0.68.

### What follows

For the centres there is nothing more to take: about 0.6 of a move in
the default mode, of which the two phases with more positions and the
beam of a million each win back 0.4, at 22 s and 118 s a cube, and the
solver turns 0.8 of shorter staging into 0.7 of shorter solution. The
full mode gets that much for 3 s by other means.

For the stretch to the end of phase C the section before this one read
a loss of 2 to 3 moves from the count (19.3) and from cubes a known walk
away. Both overstate, the same way: the centres, counted at 14, are at
16, and a cube a walk of K away is nearer than K (above). What is left
after the centres' 0.6 is unknown, and the way to know is the one used
here: an exact search stopped at a length, on a few cubes, before
anything is built to win it back.

On the method, for a problem where it might be needed: the network is
not the lever (the estimate is the tables and a few points at any size
tried: what it lacks is how the two orbits hinder each other); exact
labels are, where a bounded search can make them, worth three times the
width; and the width is, at three times the cost for 0.3 of a move
while there is room. This machine scores 1.4 million cubes a second on
the processor (the graphics processor is no faster for a network this
small) and holds a beam of about ten million.

To make it again (the network is not in the repository):

```bash
./target/release/anycube centres5-dump --syms W/syms.bin
```

```bash
./target/release/anycube centres5-dump --exact 11 --kmin 8 --kmax 16 --count 400000 --seed 11 --out W/x-11.bin
```

```bash
python ml/centres5_net.py train --work W --name k --width 256 --blocks 2 --epochs 30 --kmax 20
```

```bash
python ml/centres5_net.py tune --work W --name exact --init k --epochs 8 --batch 4096
```

```bash
./target/release/anycube centres5-dump --exact 11 --kmin 8 --kmax 16 --count 60000 --seed 99 --out W/h-99.bin
```

```bash
python ml/centres5_net.py report --work W --names k,exact
```

```bash
./target/release/anycube centres5-beam bench/scrambles555-200.txt --limit 40 --net W/exact.bin --widths 10000,100000,1000000 --out W/found
```

## 5x5x5: how far a cube is from the end of phase C, counted level by level (2026-11-15, an experiment)

Asked: the section before found the count two moves short for the
centres (14 against 16) and left open whether the same count is as far
out for the end of phase C, where it read 19.3 moves against the 23.2
the solver finds, a loss of 2 to 3 moves on which a search with all 36
moves to the end of phase C would be built.

**Verdict: there the count is 0.8 of a move short, not two. By the
levels a random cube is about 20 moves from the end of phase C (19.8 at
least) and the solver finds 23.2, so the 3 moves are there.** The centres alone account for 0.6 of
them. What makes the difference between the two goals is measured below:
with only the centres to tell cubes apart, one sequence in six leads to
a cube already counted at every length (the levels grow 23.2 times a
move where the sequences grow 28.12); with the wings and the middle
edges in the picture nearly every sequence is a cube of its own (27.2 at
seven moves and rising).

Nothing in any mode changed. What stays is the instrument:
`src/levels5.rs` and the commands `levels5` and `labellings5`.

### The levels

`anycube levels5 --goal centres|split`: the cubes at each number of
moves from a goal, with all 36 moves. The first levels are made whole
(every cube kept by a 64-bit key). The next are estimated from random
descents: from a cube of the last whole level one child a level deeper
is taken at random, and so on, the weight multiplied by the number of
such children and divided by the number of parents the chosen child has;
how deep a cube is, a few moves past the whole levels, is found by
trying every path back to them. The mean weight after j steps is the
size of level j past the whole ones (a test makes a whole level both
ways, and a run that stopped the whole levels at five gave 33,258,110
± 0.9% for the sixth, which is 33,464,710).

"Sequences" is what the count takes for different cubes: the sequences
of that length in which no move undoes or reorders the one before, from
every goal cube.

The centres staged (one goal cube among 1.79e20: the axis of the centre
at each of the 48 positions and the parity of the wings); whole to six
moves, 6,000 descents for the rest, 73 s:

| Moves | Cubes | Growth | Sequences | Cubes a sequence |
|---:|---:|---:|---:|---:|
| 1 | 6 | 6.00 | 36 | 0.167 |
| 2 | 147 | 24.50 | 1,026 | 0.143 |
| 3 | 3,054 | 20.78 | 28,836 | 0.106 |
| 4 | 65,550 | 21.46 | 810,890 | 0.081 |
| 5 | 1,471,230 | 22.44 | 2.28e7 | 0.065 |
| 6 | 33,464,710 | 22.75 | 6.41e8 | 0.052 |
| 7 | 7.73e8 ± 0.5% | 23.09 | 1.80e10 | 0.043 |
| 8 | 1.79e10 ± 0.9% | 23.19 | 5.07e11 | 0.035 |
| 9 | 4.15e11 ± 1.1% | 23.16 | 1.43e13 | 0.029 |
| 10 | 9.66e12 ± 1.4% | 23.29 | 4.01e14 | 0.024 |

Six cubes at one move, not 36: 24 of the moves keep staged centres
staged, and a wide quarter turn and its inverse leave the same axes
everywhere. The growth then settles at 23.2 and stays there: of the
sequences one move longer, one in six keeps ending on a cube that a
sequence no longer has reached.

The end of phase C for one labelling of the wings (432 goal cubes among
4.86e33: the centres by axis and the R centres among those of the R/L
axis, in each of the patterns the later phases can finish; the positions
of the low wings; the flips of the middle edges; the parity of the
wings). The state is the one phase C searches, with all 36 moves on it,
and a test follows a cube through 400 random moves in both. Whole to
four moves, 6,000 descents, 20 s:

| Moves | Cubes | Growth | Sequences | Cubes a sequence |
|---:|---:|---:|---:|---:|
| 0 | 432 | | 432 | 1 |
| 1 | 3,456 | 8.00 | 15,552 | 0.222 |
| 2 | 86,400 | 25.00 | 443,230 | 0.195 |
| 3 | 2,141,600 | 24.79 | 1.25e7 | 0.172 |
| 4 | 55,506,744 | 25.92 | 3.50e8 | 0.158 |
| 5 | 1.48e9 ± 0.4% | 26.68 | 9.85e9 | 0.150 |
| 6 | 4.00e10 ± 0.6% | 27.00 | 2.77e11 | 0.144 |
| 7 | 1.09e12 ± 0.8% | 27.24 | 7.79e12 | 0.140 |

Here the growth climbs towards that of the sequences and the share lost
stops falling: what is lost is lost in the first moves (20 of the 36
moves keep the goal; a quarter turn of R or L and its inverse leave the
same halves).

### What the levels make of a random cube

A cube has one goal for the centres, and 6,144 for the end of phase C:
2,048 labellings of its wings on each of three axes. The share of random
cubes within d moves is at most the goals times the cubes within d over
all the cubes; the levels past the last measured one are taken to grow
either as the sequences do or as the last level did.

| | Centres | End of phase C |
|---|---:|---:|
| every sequence a different cube (the count) | 13.95 | 18.99 |
| the levels, then 28.12 a move | 14.98 | 19.69 |
| the levels, then as the last one grew (23.29, 27.24) | 15.60 | 19.78 |
| share of cubes one move nearer than most, last row | 39% at 15 | 21% at 19 |
| exact search, eight cubes (the section before) | 15.88 to 16.13, one in eight at 15 | |
| the solver, default mode, 40 cubes | 16.75 | 23.20 |

(The 19.3 of the earlier section was the count without the three axes;
with them it is 19.0.)

- On the centres the levels give 15.6 where the exact searches give 15.9
  to 16.1: they explain 1.65 of the count's two moves, and are still 0.3
  to 0.5 short (with 39% of the cubes at 15, one or none in eight has one
  chance in nine).
- For the end of phase C they give 19.8. If they are as far short there
  as on the centres, a random cube is 20.1 to 20.3 moves away. No exact
  search can say: the one that proved a cube has no staging of 15 moves
  took four minutes, and each move more is 28 times that.
- These are lower bounds on the distance in one more way: a cube's 6,144
  goals are counted as so many separate chances. For the 2,048 labellings
  they nearly are (next), and the three axes share the centres.

### The labellings of a cube are separate chances

`anycube labellings5`: 20,000 random cubes, and for each the exact
number of moves of phase C its edges alone need under each of the 2,048
labellings (the table of phase C). The share of labellings within d
moves, the share of cubes that would have a labelling that near if the
2,048 were independent, the share that have, and the number of
independent labellings that share is worth:

| Moves | Labellings within | Cubes if independent | Cubes | Worth |
|---:|---:|---:|---:|---:|
| 4 | 9.8e-7 | 0.20% | 0.20% | 2,050 |
| 5 | 9.9e-6 | 2.0% | 1.9% | 1,914 |
| 6 | 1.1e-4 | 20.3% | 16.8% | 1,667 |
| 7 | 1.1e-3 | 89.6% | 70.8% | 1,114 |
| 8 | 9.1e-3 | 100% | 99.2% | 523 |

Where few cubes have a labelling that near, the 2,048 count in full;
where one cube in five has, they are worth 1,670, which is 0.06 of a
move. (The nearest labelling is 7.11 moves of phase C from its goal on
average.)

### The solver, with more stagings

`CUBE5_TRACE=1 anycube bench5`, first 40 cubes: the shortest staging and
the shortest way to the end of phase C that the solver has.

| Stagings | Shortest staging | To the end of phase C | Solution |
|---|---:|---:|---:|
| 1,200 (default mode) | 16.750 | 23.200 | 67.350 |
| 12,000, ten times the positions of the first centre phase | 16.525 | 22.825 | 66.975 |
| 120,000, a hundred times, one length further | 16.275 | 22.500 | 66.325 |

Every ten times the stagings is 0.3 to 0.4 of a move, and the last row
took 38 s a cube (with other work on the machine). The three moves are
not within reach of more of the same: the solver stages the centres first (16.75)
and then needs 6.45 moves that keep them staged for the wings, where by
the levels a way that does both at once is about 20 moves long.

### Cubes a known walk from the end of phase C, 60 of each

`anycube known5 --walk K --base split --count 60 --seed 7` (the table of
the section on what more time buys had 12 of each). A way of K moves
exists; the shortest way the solver has:

| Walk | Shortest found | Found in K or fewer |
|---:|---:|---:|
| 8 | 8.33 | 50 of 60 |
| 11 | 12.00 | 40 |
| 14 | 16.30 | 25 |
| 17 | 20.13 | 13 |
| 20 | 21.80 | 10 |
| 23 | 22.48 | 54 |
| 26 | 22.75 | |
| 30 | 22.97 | |
| random cubes (40) | 23.20 | |

Up to 11 moves away the solver finds the way that exists two times in
three. At 14 it is 2.3 moves longer on average and at 17 three moves,
with the way of 17 found on 13 cubes of 60: the loss grows with the
distance and is 3 moves before the walks are as far as a random cube.
From 20 on the walk is no longer the short way, and the cubes are not
yet random ones at 30 (22.97 against 23.20): the centres of a walk stay
nearer than those of a random cube for that long, as the section before
found.

### What follows

The premise of a search with all 36 moves to the end of phase C holds:
about 3 moves of the 67 are lost there, 0.6 of them on the centres. What
the experiment on the centres says about its cost holds too: an estimate
that sees 12 moves ahead, a beam of a million and two minutes a cube for
a goal 16 moves away; this goal is 20 moves away. It would be a mode of
its own, of minutes a cube, for a part of those 3 moves; the default
mode cannot have it. Not started.

```bash
./target/release/anycube levels5 --goal centres --cap 40000000 --deeper 4 --samples 6000
```

```bash
./target/release/anycube levels5 --goal split --cap 80000000 --deeper 3 --samples 6000
```

```bash
./target/release/anycube labellings5 --count 20000
```

## 5x5x5: a search with all 36 moves to the end of phase C (2026-11-15, an experiment)

Asked: by the levels of the section before a random cube is about 20
moves from the end of phase C and the solver finds 23.2. Can a search
with all 36 moves from the cube straight to the end of phase C, in place
of the two centre phases and phase C, win those moves back, and at what
cost. Set beforehand: a success is a mean of 21.5 or less on random
cubes at two minutes a cube or less; a failure is 22.5 or more at a beam
of a million.

**Verdict: it fails. With the estimate of the first try a beam of a
million finds a way for one random cube in six, of 25 moves, in 438 s a
cube. With a network that reads the whole cube, trained on 480 million
cubes, a beam of 10,000 finds one for 8 cubes in 20, of 28 to 30 moves.
The solver finds 23.2 on every cube in under a second.** On cubes with a
known way to the end of phase C the search does find it: nearly always
up to 12 moves with the first estimate, up to 14 with the network, half
the time at 16. A random cube is at 20. Letting the network choose only the first
moves and the solver do the rest loses as well: 69.65 moves against
67.35 with six moves chosen, 67.95 with three.

What moved the result and what did not, which is what tells "the idea
fails" from "the try was too small":

- The width of the beam: from 10,000 to 100,000 is worth one to two
  moves of reach (a way no longer than the walk for 11 and 17 cubes in
  20 at 12 moves, 6 and 10 at 14, 1 and 4 at 16). A million gave no more
  on the ten cubes tried (4 in 10 at 14).
- The estimate: reading the whole cube instead of four numbers is worth
  about two moves of reach (at a width of 10,000 it does what the table
  does at 100,000), and forty times the training cubes, in a network
  twice as wide, two more. By how far each sees, the cubes are most of
  that and the width of the network little; on the same 12 million cubes
  the wide one is worse.
- Training on what the search itself found: nothing, in one round.
- From the best of these to a random cube there are still four moves of
  reach to find, and the lengths found on random cubes are 5 to 7 moves
  over the solver's, not near it.

Nothing in any mode changed. What stays: `src/split5.rs`, the commands
`split5-beam`, `split5-data`, `split5-hybrid`, and `ml/split5_net.py`.
The times below were taken while other runs shared the machine (M1 Pro,
10 cores); they are good to a factor of two.

### The search

A beam: level by level every cube kept is moved in every way, the same
cube kept once (a 64-bit key of what the end of phase C depends on), and
the `width` cubes that the estimate puts nearest are kept. The cube is
searched in three frames, one for each axis. The end is exact: a cube
whose centres are within three moves of staged is looked up, under the
labellings of the wings that its middle edges allow, in the whole levels
of `levels5` (2,141,600 cubes at three moves), and the way is finished
down those levels. What the exact centre distances say cannot end sooner
than the shortest way found is dropped. Every way found is applied to
the cube and checked to end phase C.

A first version kept cubes under a labelling of the wings, starting from
all 2,048 on each axis. It failed by construction: at the start all the
labellings of a cube look alike, the beam fills with the wrong ones, and
it found the way for 5 cubes in 20 at eight moves and none at 14 (width
10,000). The beam holds cubes since, and the labelling is chosen at the
end.

The test cubes: a cube as phase C leaves it, then a walk of K moves, the
first not one that keeps the goal, none undoing or reordering the one
before (as `known5 --base split`). On 60 such cubes the solver finds
8.33 at K = 8, 12.00 at 11, 16.30 at 14, 20.13 at 17.

### The first estimate: a table of four numbers

For cubes a walk of K from the goal, the mean K by: the exact moves that
stage each centre orbit alone (two numbers), the classes of wings with
both wings in one half, and the classes whose low wing does not agree
with the flip of their middle edge. 8 million walks of 1 to 22 moves.

How far it sees (`--see`): the share of cubes K moves from the goal that
the estimate puts nearer than all but one in 28 random cubes (20,000
cubes of each).

| K | 8 | 10 | 12 | 14 | 16 | 18 | 20 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Mean estimate | 9.44 | 12.00 | 13.79 | 15.02 | 15.82 | 16.32 | 16.66 |
| Share put that near | 100% | 81.1% | 60.3% | 43.4% | 31.1% | 22.6% | 17.1% |

(Random cubes: 17.59, one in 28 below 15.37.)

On 20 cubes of each walk, whole levels to three moves: cubes with a way
found, and their mean (which flatters: the cubes not found are the hard
ones).

| Walk | Width 10,000 | Width 100,000 | Width 1,000,000 |
|---:|---|---|---|
| 8 | 20, 7.80 | 20, 7.70 | |
| 10 | 19, 9.95, 0.3 s | 20, 9.65, 0.7 s | |
| 12 | 14, 12.64, 1.0 s | 20, 12.25, 1.7 s | |
| 14 | 11, 15.73, 1.1 s | 16, 15.19, 5.2 s | 9 of 10, 16.11, 48 s |
| 16 | 2, 19.50, 1.3 s | 10, 17.20, 9.4 s | |

A way no longer than the walk: 11 and 17 cubes in 20 at 12 moves, 6 and
10 at 14 (4 in 10 at a million), 1 and 4 at 16. When the short way is
lost the search finds a much longer one or none.

Whole levels to four moves (55.5 million cubes) instead of three, width
100,000: 20, 17 and 11 cubes found at walks of 12, 14 and 16 (12.20,
15.77, 17.18) in 8, 58 and 104 s a cube: the same cubes at ten times the
time, the lookups under 230 labellings being what costs.

Random cubes (the first of `bench/scrambles555-200.txt`), ways of up to
30 moves:

| Width | Cubes with a way | Lengths | Time a cube |
|---:|---:|---|---:|
| 10,000 | 0 of 20 | | 1.7 s |
| 100,000 | 2 of 20 | 25, 27 | 25 s |
| 100,000, whole levels to four | 4 of 20 | 23, 25, 26, 26 | 264 s |
| 1,000,000 | 1 of 6 | 25 | 438 s |

### A network that reads the whole cube

Inputs, 1,058 of which 85 are on: the kind of the centre at each of the
48 centre positions (U/D, R, L, F/B), the wing in each of the 24 wing
positions, the middle edge and its flip in each of the 12 slots, the
parity of the wings. Three hidden layers, the label is the length of
the walk (0 to 24). Trained by `ml/split5_net.py` on the GPU of the Mac;
the solver reads the weights and evaluates the network on the processor.

That the walk's length is a fair label even far out: a cube 17 moves
from the goal is far more likely the end of a walk of 17 than of one of
22, since a walk of 22 can end on any cube. What flattened beyond 12
moves in the table above was the four numbers, not the label.

| Network | Cubes trained on | Time | Share put that near at K = 12 | 14 | 16 | 18 | 20 |
|---|---|---:|---:|---:|---:|---:|---:|
| the table of four numbers | 8 million | | 60.3% | 43.4% | 31.1% | 22.6% | 17.1% |
| 512, 256, 64 (0.7 million weights) | 12 million, 12 times | 11 min | 94.5% | 84.5% | 70.9% | 57.5% | 44.8% |
| 1024, 512, 128 (1.7 million) | 12 million, 14 times | 15 min | 91.8% | 80.0% | 65.3% | 52.2% | 39.9% |
| 512, 256, 64 | 480 million, once | 46 min | 97.4% | 89.7% | 77.2% | 63.3% | 49.8% |
| 1024, 512, 128 | 480 million, once | 59 min | 98.0% | 91.5% | 79.6% | 66.0% | 52.0% |

On walks never trained on the last two are off by 1.82 and 1.77 moves.
The wide network on 12 million cubes read many times learned them by
heart: more cubes are what helps, the size of the network little.

The search with it, 20 cubes of each walk: cubes with a way no longer
than the walk (cubes with any way).

| Walk | Table, width 100,000 | Network of 12 million, 1,000 | 10,000 | Network of 480 million (wide), 1,000 | 10,000 |
|---:|---|---|---|---|---|
| 14 | 10 (16) | 7 (10) | 11 (15), 11 s | 12 (16) | 18 (20), 17 s |
| 16 | 4 (10) | 2 (5) | 3 (10), 14 s | 4 (9) | 10 (15), 26 s |
| 18 | | | | 0 (3) | 2 (7), 30 s |

The small network at a width of 10,000 does what the table does at
100,000; the one trained on 480 million cubes does at 16 moves what the
small one does at 14.

Random cubes, width 10,000, ways of up to 30 moves: the small network
finds one for 3 cubes in 20 (29, 30, 30; 24 s a cube), the wide one of
480 million for 8 in 20 (28, 29 and six of 30; 30 s a cube).

### Trained on its own searches

The idea: what the search with the network finds is a better lesson than
the walks. 1,280 searches at a width of 1,000 with the small network, on
160 cubes of each walk from 10 to 17; a way no longer than the walk was
found for 143, 125, 99, 60, 48, 26, 13 and 6 of them. Kept as training
records: the cubes of those ways with the moves left (6,400), and 64
cubes of every level of every search, half those the network put
nearest, with the moves they have left at least (843,200; a cube d
moves into a search of a cube D moves away has D - d left at least; D
is the way found, or the walk less two where none was). A lower bound
costs only where the network says less. Four more epochs on the 12
million walks and these records four times over.

| | Before | After |
|---|---:|---:|
| Share put that near at K = 14, 16, 20 | 84.5%, 70.9%, 44.8% | 84.0%, 70.5%, 44.4% |
| A way no longer than the walk, walk 14, width 1,000 and 10,000 | 7, 11 | 7, 11 |
| The same, walk 16 | 2, 3 | 3, 4 |

Nothing moved. What the records say of the network before: of the cubes
its searches kept, 17% were said nearer than they can be, by 1.1 moves
on average; after, 8% by 0.5, and the estimate of every cube 0.2 to 0.5
higher, those on the way found as well. It learned an offset. Its real
error is the other way: the cubes on the way found with 10 to 13 moves
left are said 1.3 moves farther than they are, which puts them behind
the rest of the beam, and of those there are few to learn from. One
round and a small one; it gives no sign.

### The network for the first moves, the solver for the rest

`split5-hybrid`: a beam of 10,000 with the network keeps, after `depth`
moves, the 64 cubes it puts nearest; the solver (default mode,
`solve_any`) solves whichever of them gives the shortest total. Against
the solver alone and against 64 cubes reached by random moves of the
same number. 20 random cubes; every solution applied to the cube.

| | To the end of phase C | Solution |
|---|---:|---:|
| the solver alone | 25.15 | 67.35 |
| 3 moves by the network of 12 million | 25.35 | 67.95 |
| 3 random moves | 26.30 | 68.15 |
| 6 moves by the network of 12 million | 27.60 | 69.75 |
| 6 moves by the network of 480 million | 27.30 | 69.65 |
| 6 random moves | 28.90 | 71.35 |

(To the end of phase C: of the solution chosen, which is not the
shortest way there. With 64 cubes to start from the solver stages each
in 18 ways and not 1,200; the random moves carry the same handicap.)

The network chooses better than chance, by 1.7 moves in six, and the
solver alone is 2.3 better still: of six moves spent 3.7 are progress.
The cube is 20 moves from the goal and at 19 and 20 the network puts
the right cube among the nearest half the time, six times in a row.

### What a larger machine would change

The same was tried on the 4x4x4 on 2026-11-11 (a network of 4.4 million
weights on 192 million cubes, a beam on it alone): reductions of 27 to
30 moves for a minority of cubes against the solver's 23. Here: 28 to 30
for 8 cubes in 20 against 23.2. Forty times the cubes moved the reach by
two moves on cubes with a known way and left random cubes where they
were. The published searches of this kind (CayleyPy, arXiv 2502.13266)
give 8 billion training cubes for their results, 17 times more than
here, say that more stop helping, and at a beam of 16.7 million their
4x4x4 solutions still get 1.5 moves shorter for four times the width
(61.9, 58.1, 54.7, 51.9, 50.4 from 65,536 to 16.7 million, in their
metric). So a few hours of a large GPU would buy perhaps two more moves
of reach where four are missing, and a search that needs that GPU for
every cube.

### Commands

```
anycube split5-beam --walk 14 --count 20 --widths 10000,100000        # the table, cubes with a known way
anycube split5-beam --walk 14 --count 10 --widths 1000000
anycube split5-beam --walk 14 --count 20 --widths 100000 --cap 80000000   # whole levels to four
anycube split5-beam bench/scrambles555-200.txt --limit 20 --widths 10000,100000 --max 30
anycube split5-beam bench/scrambles555-200.txt --limit 6 --widths 1000000 --max 30
anycube split5-beam --walk 1 --count 1 --see [--net net.bin]          # how far the estimate sees

anycube split5-data --count 12000000 --kmax 24 --seed 1 --out walks.bin
ml/split5_net.py --data walks.bin --out net0.bin --epochs 12
ml/split5_net.py --data fresh.bin --fresh target/release/anycube --out net-fresh.bin --sizes 1024,512,128 --epochs 40
anycube split5-beam --walk 16 --count 20 --widths 1000,10000 --net net-fresh.bin
anycube split5-beam bench/scrambles555-200.txt --limit 20 --widths 10000 --max 30 --net net-fresh.bin

for K in 10 11 12 13 14 15 16 17; do
  anycube split5-beam --walk $K --count 160 --seed $((100+K)) --widths 1000 --net net0.bin --dump found-$K.bin --take 64
done
ml/split5_net.py --data walks.bin found-*.bin --repeat 4 --init net0.bin --out net1.bin --epochs 4 --rate 3e-4

anycube split5-hybrid bench/scrambles555-200.txt --limit 20 --net net-fresh.bin --depth 6 --width 10000 --keep 64
```

## Any size: long rows of centres made alike, then matched (2026-11-16, branch `centres-large`)

The question: can the cubes of sizes 8 to 21 still be made shorter, by
working on the centres. They can, in the matching of the long rows, and
most of what is gained at the largest sizes is that no cube goes the long
way any more.

### Where the moves went

`CUBEN_TRACE=1 anycube benchn <file> --size N` on the release files (20
cubes at 9x9x9, five each at the others), the code of `origin/main`
(3fd52cd). Moves, and moves a piece of what the step is for; the cubes that
went the long way are left out of the step columns:

| Size | Total | Staging | a centre | Matching | a centre it matches | Edges | an edge piece | 3x3x3 | The long way |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 9 | 301.40 | 114.05 | 0.40 | 62.30 | 0.32 | 107.90 | 1.28 | 17.15 | none of 20 |
| 13 | 743.20 | 292.60 | 0.41 | 256.40 | 0.45 | 177.40 | 1.34 | 16.80 | none of 5 |
| 15 | 1025.00 | 403.60 | 0.40 | 391.60 | 0.47 | 213.00 | 1.37 | 16.80 | none of 5 |
| 17 | 1380.00 | 539.75 | 0.40 | 561.25 | 0.49 | 245.50 | 1.36 | 17.00 | 1 of 5 (1,446 moves, 72 s) |
| 21 | 2246.40 | 859.00 | 0.40 | 938.50 | 0.49 | 316.00 | 1.39 | 18.00 | 3 of 5 (2,289 to 2,358 moves, 78 to 174 s) |

The staging costs the same at every size, 0.40 moves a centre, and the
edges 35 moves an orbit of wings. The matching is what grows, and by the
row:

| Row of | 3 | 5 | 7 | 9 | 11 | 13 | 15 | 17 centres |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| moves, as it was | 18.8 | 43 to 49 | 74 to 84 | 105 to 113 | 126 to 143 | 154 to 172 | 177 | 232 |
| a column | 6.3 | 8.7 to 9.8 | 10.6 to 12.0 | 11.6 to 12.6 | 11.5 to 13.0 | 11.8 to 13.3 | 11.8 | 13.7 |

A row of three or five is matched with all its columns in one search; a
longer one from its middle outwards (`Ring::extend`), which took 6.5 moves
for the middle column, 24 to 30 for every two columns after it and 18 to 20
for the two at the ends. The matching was also most of the time: 12 of the
17 s of a 15x15x15.

The long way is not a rare accident at the large sizes. A step of `extend`
is a beam search on a sum of what each axis needs by itself; when it finds
nothing the row is tried again with another of the eight choices of sign,
and when all eight fail every centre is brought home instead, 80 to 230
moves and a minute more. (No step has an exact ending. On the 17x17x17
cube that went the long way four choices failed at the step of the two
ends and four at a step before it.) On two of the five 17x17x17 cubes a row needed its fourth or
fifth choice; of the five 21x21x21 cubes three had a row on which all eight
failed, and of twenty (below) five. That is why the release table's
21x21x21 stands at 2,248.

### What a step was asked for

The columns done so far are kept alike (the same pattern in the row, the
same in the row under it), because columns that are alike and are not cut
apart move alike, so they are one thing to follow. `extend` also keeps them
matched, at every step: of a step that adds two columns it asks that each
is like the first column in both rows (four things, each 18.4 bits: one
pattern of 70 on each of three axes) and that the first column is matched
again (a fifth).

But alike is enough. Columns that are alike in both rows move alike whether
they are matched or not. So (`Ring::alike`):

- a step only makes its two columns like the first one, row for row. The
  table of four orbits is the same with another goal: (x, y, x, y) for any
  two patterns, where it was (x, x, x, x);
- the middle column is not matched before the steps;
- at the end the inner columns are one thing, and that and the two columns
  at the ends are a row of three, matched in one search with its exact
  ending (`Ring::whole`, which was there for the short rows), by the turns
  that cut the ends only. All eight choices of sign are searched side by
  side there, where `extend` chose one before its first step.

And one thing follows that was not looked for. The row under the one being
matched is left with its inner columns alike, and nothing the last search
does cuts them. It is the upper row of the next matching, so every row
after the first starts each step with half of it done:

| Step | Moves, 15x15x15 | 21x21x21 |
|---|---:|---:|
| `extend`: two columns, like the first and matched | 24 to 30 | 20 to 33 (two cubes) |
| `alike`, the first row of a cube | 24.8 to 27.4 | 24.8 to 28.0 |
| `alike`, every row after it | 17.8 to 22.0 | 17.4 to 22.4 |
| the inner columns and the two ends, as a row of three | 17.4 to 19.0 | 18.6 to 20.2 |

A row of five that comes after a longer row is done this way as well (one
step and the last search: 36 to 40 moves where all columns in one search
took 44 to 47). As the first row of a cube (9x9x9) a row of five stays as
it was: 45.7 this way against 43.5, and the shorter of the two (41.4) costs
twice the time of the matching. The same for a row of four: 30.3 against
31.8 after a longer row (10x10x10), 30.9 against 29.4 as the first row
(8x8x8). So sizes 8 and 9 give the same solutions as before.

### A step does not arrive by itself

With the new goal a beam search of 30,000 states found nothing, on any of
six rows of eleven centres: from about its 25th level to its 50th every
state it holds has the score 1, one half turn from done on one axis. The
turns that cut the new columns off are the only moves that change how they
stand to the first column (every other move takes all of them along), and
each of them changes two axes, both rows and both new columns at once:
eight things, of which the score sees one. From such a state the way is
four to eight moves: the other seven have to be set up so that the turn
exchanges centres of one colour there.

What made it work, in the order tried (rows of eleven centres on random
staged cubes, `anycube rowtest --size 15 --row 1`, at the time of each):

| | Rows matched this way | Moves | Seconds a row |
|---|---|---:|---:|
| `extend`, as it was | 6 of 6, after up to three failed choices | 141.5 | 13 |
| beam alone, new goal | 0 of 6 | | |
| states the beam calls close (score up to 2), an exact search of up to 6 moves from each | 4 of 6 | 126 | 86 to 132 |
| the same, endings looked for by length over all the starts | 2 of 3 | 124 | 28 to 45 |
| a table over two axes for a pair of orbits, in the bound of the exact search | 2 of 3, the same solutions | 124 | 7 to 10 |
| that table in the score of the beam as well | 6 of 10 | 134.1, the other four rows as before | 6.4 |
| other ties (six) when no ending is found | 8 or 9 of 10 | 129 to 130 | 6 to 9 |
| the beam a level at a time, the 64 best states of every level tried for an ending of up to 5 moves | 10 of 10 | 129.4 | 4.1 |
| the same, 16 states and up to 6 moves | 10 of 10 | 122.7 | 6.9 |

- **The table over two axes** (`Ring::two_axes`, 24 MB, a plain
  breadth-first search): for one orbit of the new column and the one it is
  to be like, the moves until they agree on two axes together. It sees what
  a half turn ties together within one row of one column, and takes the
  exact search of six moves from 20 to 40 s to 3 to 7 s.
- **Starts chosen by a threshold of the score do not do**: on some steps the
  beam never gets under any fixed score, and those steps fail whatever the
  ties. The best states of every level do.
- **The last move of a step is a turn that cuts the new columns off** (any
  other move leaves alike what was alike), which is a sixth of the last
  level of the exact search.
- **An ending is only looked for where the bound leaves two moves to
  spare**: most of the time went on states that the tables allowed six
  moves and that had no ending that short. 949.4 moves in 16.3 s against
  947.2 in 19.0 s (15x15x15, five cubes); three to spare 948.4 in 21.6 s,
  four 953.2 in 16.8 s.
- **One step in eighty still finds no ending.** A longer ending (7 or 8
  moves) from the states that came nearest was there every time and took
  20 to 86 s. A wider beam finds one in a few seconds: the same search
  three times as wide, then ten times. (With other ties at the same width
  three steps in seven got their ending; one step of a 20x20x20 was not
  finished by seven searches at up to three times the width and was by
  one at ten times, in 4 s: 12 s for the step where the longer ending
  took 86 s.) A search is given up when its lowest score has stood still
  for five levels: of 150 searches the first ending came on the level
  that brought a lower score in 140 and never more than three levels
  after it.

What the numbers of a step are worth (15x15x15, the five cubes, whole
solves):

| Changed | Values | Moves | Seconds a cube |
|---|---|---|---|
| Moves of an ending | 5, **6**, 7 | one cube the long way, 955.8, 953.8 | 23.6, 17.6, 72.8 |
| States tried per level | 8, **16**, 32 (with a beam of 60,000) | 956.8, 947.2, 943.8 | 18.5, 19.0, 25.8 |
| States per level of the beam | 15,000, **30,000** | 955.0, 947.2 | 20.1, 19.0 |
| Levels after the first ending | 0, **2** | 948.8, 947.2 | 18.8, 19.0 |

Did not work:

- **The score from the tables over two axes alone**: nothing found.
- **Choosing among the ways of a step by what they leave the next step**
  (the next step's score at the end of each way, the ways as long as the
  shortest): 945.6 against 947.2, and with every start's ending collected
  947.2 in twice the time. Removed.
- **Both ways for the first row of a 9x9x9 and the shorter kept**: 298.95
  against 301.40 in 6.1 s against 3.7 s. Not in the default.

### Whole solves, old and new one after the other

`anycube benchn <file> --size N --out <file>`, the binary of `origin/main`
(3fd52cd) and the new one in turn for each size, nothing else running;
sizes 8 to 10 on the release files, the others on new files of twenty
cubes (`bench/scrambles131313-20.txt` and its neighbours: the same seed as
the release files, whose five cubes are their first five). Every solution
applied on the sticker model. The new binary's columns are from a second
series three hours later, with the searches of a step as they are now (the
series in turn had 697.40, 1245.20 and 1937.10 moves for sizes 13, 17 and
21 in 12.8 s, 33.1 s and 45.9 s, and the other sizes as below).

| Size | Cubes | Moves before | Moves now | | Seconds before | Seconds now | Slowest before | Slowest now | The long way before | now |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 | 20 | 227.35 | 227.35 | the same solutions | 4.5 | 4.3 | 7.8 | 8.5 | 0 | 0 |
| 9 | 20 | 301.40 | 301.40 | the same solutions | 3.9 | 4.1 | 11.8 | 11.9 | 0 | 0 |
| 10 | 20 | 400.40 | 393.75 | -1.7% | 6.6 | 6.7 | 14.5 | 12.8 | 0 | 0 |
| 13 | 20 | 741.85 | 697.55 | -6.0% | 12.9 | 12.3 | 24.0 | 17.6 | 0 | 0 |
| 15 | 20 | 1034.65 | 953.50 | -7.8% | 21.2 | 18.4 | 58.0 | 24.3 | 1 | 0 |
| 17 | 20 | 1370.45 | 1243.70 | -9.2% | 29.1 | 27.8 | 76.2 | 38.4 | 1 | 0 |
| 21 | 20 | 2194.00 | 1934.75 | -11.8% | 56.2 | 44.2 | 117.2 | 58.7 | 5 | 0 |

Shortest and longest cube: 13x13x13 672 to 715 (710 to 764 before),
15x15x15 925 to 976 (995 to 1,086), 17x17x17 1,225 to 1,279 (1,320 to
1,457), 21x21x21 1,908 to 1,992 (2,120 to 2,358).

Of the 1,240 steps of these runs 15 needed a second search and one a
third (2 to 9 s for such a step, where the others take 0.7 to 1.2 s); none
needed a longer ending, and no row fell back on `extend`. The matching is
still most of the time: 8.8 of 12.3 s at 13x13x13, 21.1 of 27.8 s at 17,
33.7 of 44.2 s at 21.

The same cube gives the same solution: a second run of the new binary
gave every solution again, move for move (sizes 13 and 17, twenty cubes
each, with the searches as they are now; sizes 8, 9, 10, 13 and 15 in the
first series).

The other sizes, on the release files (five cubes each), each binary alone
on the machine:

| Size | Moves before | Moves now | | Seconds before | Seconds now | The long way before | now |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 11 | 504.00 | 478.80 | -5.0% | 8.1 | 7.0 | 0 | 0 |
| 12 | 611.40 | 587.00 | -4.0% | 11.0 | 10.5 | 0 | 0 |
| 14 | 867.40 | 825.60 | -4.8% | 16.7 | 17.5 | 0 | 0 |
| 16 | 1163.40 | 1095.60 | -5.8% | 18.9 | 27.5 | 0 | 0 |
| 18 | 1525.80 | 1412.40 | -7.4% | 28.1 | 32.3 | 0 | 0 |
| 19 | 1747.40 | 1579.40 | -9.6% | 52.1 | 37.9 | 1 | 0 |
| 20 | 1930.00 | 1760.00 | -8.8% | 39.6 | 41.8 | 0 | 0 |

**The 16x16x16 and the 18x18x18 are slower**, by 8.6 s and 4.2 s a cube.
The first row of a cube is where the new matching takes longer than the
old one did when its first choice of sign arrived (8.4 s against 3.0 s at
16x16x16: five steps of about 1.5 s), and these two sizes had no cube the
long way to make up for it. In moves they are 68 and 113 shorter.

Step by step on the release files (the table at the top of this section,
now):

| Size | Total | Staging | Matching | a centre it matches | Edges | 3x3x3 |
|---:|---:|---:|---:|---:|---:|---:|
| 13 | 701.80 | 292.60 | 215.00 | 0.37 | 177.00 | 17.20 |
| 15 | 949.40 | 403.60 | 316.20 | 0.38 | 212.60 | 17.00 |
| 17 | 1238.00 | 537.40 | 437.80 | 0.38 | 245.20 | 17.60 |
| 21 | 1940.20 | 866.40 | 742.80 | 0.39 | 313.60 | 17.40 |

| Row of | 3 | 5 | 7 | 9 | 11 | 13 | 15 | 17 centres |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| moves now, after a longer row | 18.2 to 20.4 | 36.4 to 38.4 | 57.4 to 59.4 | 74.4 to 76.2 | 94.8 to 95.6 | 116.8 | 133.4 | |
| moves now, the first row of a cube | | | | 100.6 | 124.2 | 152.8 | | 207.4 |

### What is left

- **The first row of a cube**: 25 to 28 moves a step where the rows after
  it take 17 to 22, with no row before it to leave it half done: 207 of
  the 743 moves of matching at 21x21x21, and the slowest part.
- **The last search of every row**: 18 to 20 moves for three things (the
  inner columns as one, and the two ends), eight times on a 21x21x21.
- **Counted in what a step has to achieve** (18.4 bits a pattern on three
  axes; a step after the first row has two to make and two to keep), the
  steps are at about four bits a move, where 36 moves that could all
  follow one another would carry five. A shorter matching would have to
  ask for less again, as this one does.
- **The staging** is now the largest item, 866 of 1,940 moves at 21x21x21,
  0.40 a centre at every size (the 7x7x7 solver pays 0.35); the edges are
  at 35 moves an orbit of wings where a 5x5x5 of its own takes 32.8.
- **Time**: the exact endings were 8.4 of the 11.9 s of the steps of a
  15x15x15 before they were kept to states with two moves to spare; not
  measured since. A step keeps its states as the patterns of every column
  of the row (192 bytes) where six orbits are in play; the short rows got
  part of their speed from states of 24 bytes (`Short`). Not tried here.
- The 21x21x21 is under 2,000 moves on every cube of the twenty (1,908 to
  1,992; mean 1,934.75) and not under 1,900.

### Commands

```
CUBEN_TRACE=1 anycube benchn bench/scrambles151515-5.txt --size 15     # every step of every cube
anycube rowtest --size 15 --row 1 --count 10                           # one row on random staged cubes
anycube scrambles --size 21 --count 20 --length 1050 > bench/scrambles212121-20.txt
anycube benchn bench/scrambles212121-20.txt --size 21 --out new-21.tsv # old and new binaries in turn
```

### The rows of the release table

`bench/releases.tsv`, v0.2.0: sizes 2 to 7 by `scripts/release.py measure
v0.2.0 2 3 4 5 6 7` at 63583d3 (this change does not touch them; they stood
at their figures of v0.1.1). Sizes 8 to 21 are the runs of the new binary on
the release files above, not run again; their memory is that of the first
cube alone (`/usr/bin/time -l anycube row <file> --size N --limit 1`), which
is less than a whole run holds (2.9 GB against the 4.3 GB of v0.1.1 at
21x21x21, where a run of five cubes touches more of the tables).

## Projected face turns on 4x4x4 and 5x5x5 (2026-11-17)

The requested mean lengths are 35–37 and 55–60 OBTM. Re-running the v0.2.0
normal solver on the first 100 benchmark scrambles reproduced 40.55 and
67.34. The paired comparison below uses lines 101–130 of the same files.
Both solvers receive only the resulting stickers. The short-scramble
control uses the first 14 turns of each of those same lines.

| Size | Scramble | Holds | Ordinary | Best ordinary hold | After assigning widths | After final face or shell | Seconds per cube |
|---|---|---:|---:|---:|---:|---:|---:|
| 4x4x4 | 100 turns | 6 | 40.40 | 40.10 | 40.10 | 40.07 | 29.58 |
| 5x5x5 | 150 turns | 6 | 66.93 | 66.27 | 66.27 | 66.20 | 8.71 |
| 4x4x4 | 14 turns | 1 | 15.70 | 15.70 | 14.47 | off | 1.45 |
| 5x5x5 | 14 turns | 1 | 23.70 | 23.70 | 17.87 | off | 11.16 |

On the long scrambles, trying six orientations helped 6 of 30 4x4x4s and
14 of 30 5x5x5s. Trying a last face saved another move on one 4x4x4
(source line 114). The embedded 4x4x4 saved another move on two 5x5x5s
(source lines 106 and 126). The shortest solutions were 38 and 64. On
the short scrambles, width assignment improved 5 of 30 4x4x4s and 18 of
30 5x5x5s. The number solved in at most 14 moves rose from 24 to 27 on
the 4x4x4 and from 10 to 25 on the 5x5x5. On line 107 of the 5x5x5 set
the ordinary solution took 42 moves and the new search found 14. All 120
solutions in this first comparison were replayed with `anycube verify`.
Times cover the whole search on an M1 Pro after loading the tables. Other experiments ran
concurrently, so these timings describe the runs shown here.
[Moves for every cube](lift-45.tsv)

### Twenty further long scrambles per size

With the search settings fixed, we ran full-length scrambles from lines
151–170. The pooled rows include those cubes and the first 30.

| Long scrambles | Size | Ordinary | Best of six holds | Projected 3x3x3 | Final |
|---|---|---:|---:|---:|---:|
| Further 20 | 4x4x4 | 40.55 | 40.40 | 40.30 | 40.25 |
| Further 20 | 5x5x5 | 67.25 | 66.80 | 66.80 | 66.75 |
| All 50 | 4x4x4 | 40.46 | 40.22 | 40.18 | **40.14** |
| All 50 | 5x5x5 | 67.06 | 66.48 | 66.48 | **66.42** |

The complete search shortened 13/50 ordinary 4x4x4 solutions and 23/50
ordinary 5x5x5 solutions. On top of the six orientations, four 4x4x4s
and three 5x5x5s each gained one further move. The shortest solutions
across the 50 cubes were 38 and 64. The 100 long solves and the 80
shorter-scramble controls in this section all passed a separate sticker
replay.

### What the width search can recover

On a 5x5x5, a face turn and its wide version have identical effects on the
corners and middle edges. On a 4x4x4 both have identical effects on the
corners. Given a sequence of faces and powers, each turn has two possible
widths. For each wing and centre, the search propagates the places it
can reach from the initial state and the places from which it can still
reach its goal. It eliminates a width when any piece needs the other
choice. It branches when propagation leaves both choices open.

In a separate control, `bench-widths` scrambles with an L-move prefix and
supplies the faces, directions and powers of its inverse. Width recovery
returned sticker-verified solutions on **1,000/1,000** 4x4x4s at L = 37
and **200/200** 5x5x5s at L = 60. No search reached the 25,000-node
budget. On lines 171–180 we gave the sticker-only solver the same
37- and 60-move positions, with one hold and the size-specific ending
enabled. It found a solution within the planted length on 0/10 cubes at
each size. Its means were 40.40 and 67.60. Supplying the projected word
allowed width recovery on all 20 of those positions.

In a sticker-only test the generator must discover the face sequence.
It proposes short 3x3x3 solutions from the input and near the
last wide turn of up to four ordinary solutions. It can split a projected
turn into two or insert an inner-layer turn using two moves. The projected
3x3x3 search shortened two long 4x4x4 solves (lines 152 and 168) and none
of the 50 long 5x5x5 solves.

### Choosing the last face on the 4x4x4

A hidden permutation of four same-colour centres can invalidate a solution
obtained by inverting a scramble. An outer turn at the solved end preserves
each face's set of centres. It is therefore possible to impose such a turn,
solve the resulting auxiliary position with the fast 4x4x4 search and map
the turn back using the actual solved orientation. The implementation tries
all 18 choices and combines a turn with another turn of the same face when
they commute past the intervening moves.

### Endings from the embedded 4x4x4

Removing the middle row and column of each 5x5x5 face preserves the
corners, all the wings and one orbit of centres. The fast 4x4x4 solver
proposes endings for this shell near the last wide turn. A wide turn can
be played from either side of its axis with a change of hold on the
4x4x4. Its two spellings move the omitted pieces differently on the 5x5x5.
The search keeps the spellings that return the shell to the correct frame
and checks the middle edges and remaining centres on the full cube.
Without the shell search, the long 5x5x5 sample took 5.41 s per cube.

### Reproduction

Run from the repository root with a release binary.

```sh
for size in 4 5
do
    if [ "$size" = 4 ]; then file=bench/scrambles-1000.txt; length=37; ending=--last-face
    else file=bench/scrambles555-200.txt; length=60; ending=--shell4
    fi
    sed -n '101,130p' "$file" |
        target/release/anycube bench-lift /dev/stdin --size "$size" --holds 6 "$ending"
    sed -n '151,170p' "$file" |
        RAYON_NUM_THREADS=3 target/release/anycube bench-lift /dev/stdin --size "$size" --holds 6 "$ending"
    awk 'NR >= 101 && NR <= 130 { NF = 14; print }' "$file" |
        target/release/anycube bench-lift /dev/stdin --size "$size" --holds 1
    awk -v length_limit="$length" 'NR >= 171 && NR <= 180 { NF = length_limit; print }' "$file" |
        RAYON_NUM_THREADS=4 target/release/anycube bench-lift /dev/stdin --size "$size" --holds 1 "$ending"
    target/release/anycube bench-widths "$file" --size "$size" --length "$length"
done
```

## Search speed and full solutions for large cubes (2026-11-18)

The control was commit `a612527`. Both release binaries solved the same
long scrambles on an Apple M1 Pro with the lookup tables already built.
Rayon used its default thread count. Time is the mean elapsed search time
per cube after the CLI's initial table load. It includes work performed
lazily during a solve. Each cohort was split in half. Within the first half the control
ran before the candidate, and within the second half their order was
reversed. Sizes 4 to 7 repeated each half in the opposite order. Full
mode ran after the normal pair on the same half for sizes 8 and up.

Each normal solution matched the control move for move. Both normal and
full words were replayed with the control binary's separate sticker
verifier. The release averages and these samples use different sets of
positions. The data file includes the extra word checks and the SHA-256
digest of each move sequence as UTF-8 text without a trailing newline.
[Per-cube measurements](search-optimization.tsv)

| Size | Cubes | Normal OBTM | Previous seconds | New seconds | Time saved | Full OBTM | Full seconds | Shorter in full |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 4 | 16 | 40.69 | 1.77 | 1.44 | +18.4% | — | — | — |
| 5 | 16 | 67.19 | 0.83 | 0.68 | +18.4% | — | — | — |
| 6 | 6 | 107.50 | 6.06 | 5.95 | +1.8% | — | — | — |
| 7 | 6 | 152.17 | 3.49 | 2.94 | +15.8% | — | — | — |
| 8 | 6 | 226.17 | 4.37 | 3.56 | +18.6% | 225.33 | 3.85 | 2/6 |
| 9 | 6 | 299.83 | 3.07 | 2.84 | +7.3% | 299.33 | 6.42 | 3/6 |
| 10 | 4 | 395.75 | 5.49 | 4.97 | +9.5% | 391.75 | 11.04 | 4/4 |
| 12 | 2 | 592.50 | 9.76 | 9.13 | +6.5% | 591.50 | 23.25 | 1/2 |
| 15 | 4 | 960.75 | 19.98 | 19.02 | +4.8% | 955.75 | 32.55 | 3/4 |
| 20 | 2 | 1771.00 | 39.86 | 37.98 | +4.7% | 1771.00 | 50.13 | 0/2 |
| 21 | 4 | 1934.25 | 44.12 | 41.89 | +5.1% | 1933.75 | 48.19 | 2/4 |

On the 28 mixed-width positions at sizes 8 and up, full mode shortened 15
solutions and saved 48 moves in total. The largest individual saving was
18 moves on size 15, line 12 of its input file. A separate
development pair on size 21 (lines 3 and 4) went from 1,949 and 1,929
moves to 1,922 and 1,902. Choosing the wing reductions alone had given
1,947 and 1,929 on that pair.

A second length check on 46 positions also reproduced the control words.
It used lines 71–90 on sizes 4 and 5 and lines 71–73 on sizes 6 and 7.
Six earlier `--lift` improvements on sizes 4 and 5 passed another replay
with six holds and the size-specific ending enabled. The 35–37 and
55–60 targets for long mixed-width scrambles remain open. An exploratory
exact replacement of short corner-projected move sequences shortened
zero of the 44 initial solutions at sizes 4 through 7.

The 4x4x4 search intersects the possible last moves for all twelve wing
pairs before playing them. The 5x5x5 reuses centre prefixes in all six
frames and only considers symmetries that attain the least x-centre
pattern. The finishing 3x3x3 search skips depths it has exhausted. The
6x6x6 and 7x7x7 apply centre-mask moves in batches. On larger cubes the
beam search counts scores with two 256-entry histograms, and rings with
the same move action share their tables.

Full mode compares the last two wing orbits as a pair. It first solves the
ordinary reduction, then considers up to eight other distinct positions of
the outer 5x5x5. From size nine it also tries a second centre staging. Two
half turns of different widths can act identically on the columns being
formed and differently on the next column. The solver compares those
choices on the next seven-layer projection and keeps the shorter complete
solution. The ordinary answer participates in this comparison.

### Positions scrambled at a single layer width

When all six faces consist of uniform rectangular blocks with the same
layer boundaries, the solver can reconstruct a 2x2x2 or 3x3x3 from the
stickers. A single turn on that cube lifts to a single turn of the
detected physical width. The solver replays each candidate on the
original stickers. Every physical move induces one face turn on the
corners, so reaching the optimal 2x2x2 distance proves optimality on the
large cube. On odd sizes the corners and true middle edges give a second
lower bound from the exact 3x3x3 search. When the proof exhausts its node
budget, the lifted solution bounds the ordinary search.

Each row below used a seeded sequence of face turns, mapped to the
indicated width. Rows with the same scramble length share the underlying
face sequences. The fixed-width tests exercise this class of positions.
The table above uses scrambles with mixed widths.

| Size | Layers per turn | Scramble length | Cubes | Previous OBTM | New OBTM | Previous seconds | New seconds |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 8 | 4 | 60 | 3 | 37.33 | 9.00 | 0.642 | 0.002 |
| 9 | 3 | 8 | 3 | 46.67 | 8.00 | 0.539 | 0.015 |
| 15 | 5 | 60 | 2 | 232.00 | 18.50 | 2.220 | 3.652 |
| 20 | 10 | 60 | 3 | 181.67 | 9.00 | 1.836 | <0.001 |
| 21 | 7 | 8 | 3 | 273.67 | 8.00 | 3.313 | 0.011 |

The 15x15x15 sample spent more time searching its harder 3x3x3 projections.
Search times in this table use the CLI's millisecond timer. Each binary
loads its shared large-cube tables before that timer starts.

### Reproduction

The TSV gives the input filename and one-based line for every mixed-width
position. For example, use these commands on the four 21x21x21 positions.
The project uses `bench` for size 4, `bench5`, `bench6` and `bench7` for
the next three sizes, and `benchn --size N` for the remaining sizes.

```sh
work=$(mktemp -d)
sed -n '11,14p' bench/scrambles212121-20.txt > "$work/cubes"
target/release/anycube benchn "$work/cubes" --size 21 --out "$work/normal.tsv"
target/release/anycube benchn "$work/cubes" --size 21 --mode full --out "$work/full.tsv"
cut -f5 "$work/full.tsv" > "$work/words"
paste "$work/cubes" "$work/words" > "$work/replay"
target/release/anycube verify --size 21 "$work/replay"
```

To reproduce the 20x20x20 fixed-width row, generate the three face
sequences and play every turn ten layers wide.

```sh
target/release/anycube scrambles --size 3 --seed 18711 --length 60 --count 3 |
  sed -E 's/([URFDLB])/10\1w/g' > "$work/blocks"
target/release/anycube benchn "$work/blocks" --size 20
```

A 30x30x30 with a half-width turn followed by an outer turn also passed
the sticker check. Above size 28 the general solver uses its existing
reduction that brings each projected cube’s centres home before its wings.

## Default mode on fresh random-move scrambles, 2x2x2 through 21x21x21

The comparison on 2026-11-18 starts at `38fe0d4`, the last version of this
branch before this experiment. The new solver is `31962f1`. Both
release-mode binaries ran on the same 10-core Apple M1 Pro with 16 GB of
RAM and the existing lookup tables. Every row uses the default settings.

The development and validation sets were generated separately before
tuning. Each size used a different seed. The held-out set has 212 cubes.
The CLI received the same random-move scrambles in both runs. The search
used only the resulting cube state. The scrambler can choose every
supported turn width. Its seed was `6175103 + 997 * N` for validation
and `912371 + 997 * N` for development.

Moves are in OBTM. Times are mean wall seconds per cube. Each set was
split in two. Both binaries processed each half, with their order
reversed for the second half. The timed interval excludes solver loading.
It includes any work triggered by the first solve in a batch. The `row`
command reports per-cube times to the nearest hundredth of a second for
sizes 2 and 3.

| Cube | Cases | Moves before | Moves after | s before | s after | Shortened |
|---|---:|---:|---:|---:|---:|---:|
| 2x2x2 | 20 | 8.80 | 8.80 | <0.005 | <0.005 | 0 |
| 3x3x3 | 20 | 17.75 | 17.75 | 0.202 | 0.211 | 0 |
| 4x4x4 | 48 | 40.54 | 40.40 | 1.240 | 1.841 | 6 |
| 5x5x5 | 48 | 67.46 | 67.00 | 0.588 | 1.093 | 17 |
| 6x6x6 | 10 | 110.50 | 110.50 | 5.250 | 4.963 | 0 |
| 7x7x7 | 10 | 151.40 | 151.30 | 2.552 | 2.942 | 1 |
| 8x8x8 | 6 | 230.00 | 229.17 | 4.470 | 4.515 | 3 |
| 9x9x9 | 6 | 301.17 | 300.50 | 2.588 | 3.551 | 2 |
| 10x10x10 | 6 | 388.33 | 387.00 | 5.963 | 6.341 | 5 |
| 11x11x11 | 4 | 487.25 | 485.75 | 6.715 | 8.200 | 3 |
| 12x12x12 | 4 | 589.75 | 589.00 | 9.743 | 10.123 | 2 |
| 13x13x13 | 4 | 701.75 | 700.00 | 9.859 | 11.078 | 2 |
| 14x14x14 | 4 | 828.00 | 827.75 | 13.755 | 14.397 | 1 |
| 15x15x15 | 4 | 953.75 | 952.75 | 17.699 | 18.603 | 2 |
| 16x16x16 | 3 | 1093.67 | 1093.00 | 17.750 | 17.310 | 2 |
| 17x17x17 | 3 | 1237.67 | 1237.00 | 23.762 | 24.374 | 2 |
| 18x18x18 | 3 | 1417.00 | 1416.67 | 28.793 | 29.306 | 1 |
| 19x19x19 | 3 | 1563.33 | 1563.33 | 37.695 | 38.502 | 0 |
| 20x20x20 | 3 | 1774.33 | 1773.33 | 35.767 | 35.304 | 3 |
| 21x21x21 | 3 | 1944.33 | 1943.33 | 39.902 | 41.085 | 1 |

53 cases had a shorter solution and 159 tied. All 424 solutions passed
sticker replay. The 4x4x4 and 5x5x5 saved 7 and 22 moves respectively
over their 48 cases. The shortest new solutions had 38 and 65 moves.
The target averages remain 35–37 and 55–60 moves.

On the 4x4x4, the mean moves in reduction and the 3x3x3 finish went
from 23.62 + 16.92 to 23.81 + 16.58. On the 5x5x5 they went
from 50.38 + 17.08 to 50.23 + 16.77. The extra search took
an average of 0.60 seconds on the 4x4x4 and 0.51 seconds on the 5x5x5.
Sizes 16 through 21 each have three cases.

The 4x4x4 keeps the ordinary solution as an incumbent. An actual opening
wide turn supplies further reductions. Its cost is combined with any
commuting moves at the start of each reduction. On longer solves, another
search revisits the deepest reached centre-staging level, prioritizing
two-move prefixes by centre distance and separated wings. Distinct
reduced 3x3x3 positions share a finishing search.

The 5x5x5 searches backwards from the set of reduced positions, tracking
both centre orbits and the oriented middle edge paired with each wing.
A short forward search from cuts in a reduction can meet that reverse
search using all 36 turns. The final 3x3x3 determines whether a new
reduction actually shortens the complete word. A separate, smaller
search in the reflected cube can supply a different whole reduction.

From 8x8x8, the ordinary ending stays in contention with six penultimate
wing reductions ranked using the next orbit. The score charges the moves
already spent and one move per four remaining misplaced wings, rounded
up. Each challenger must beat the length of the complete ending. The
6x6x6 and 7x7x7 also compare reflected finishes on their projected 5x5x5.

The speed changes include in-place sticker turns, shared quick searches
for identical 3x3x3 states, and reusing phase-one depths already searched.
On large cubes the distances for opposite centre colours are read from
the table for agreeing colours by complementing one orbit. A test compares
all those entries with an independently generated table.

In a separate turn-only measurement, applying 140,000 mixed-width turns
on a 21x21x21 took 220.2 ms before and 32.1 ms after (6.86×).
These are medians of eight alternating runs. Both implementations
produced the same stickers. The table above times complete solves.

The CLI also solved the first validation 4x4x4 and 5x5x5 directly from
their sticker colours in 39 and 68 moves, matching their benchmark rows.
Full mode solved the first 8x8x8 in 224 moves and the first 9x9x9 in 302.
The previous version replayed all four solutions successfully.

### Development controls

On the 24 development cubes per size, an unrestricted backwards search
from the single winning reduction saved no moves on the 4x4x4 and one
move on the 5x5x5. Trying state-dependent excursions with R and L quarter
turns during the 4x4x4 reduction also saved no moves. Scouting up to two
additional reduction depths saved one move over the 24 cubes at about
6.2 seconds per cube in that trial. These trials guided the choice of
the default search. The timing comparison in the table uses the final
binaries on the separate validation set.

The per-cube file contains the generator seed, one-based case number,
scramble length, move counts and both timings. [Paired measurements](default-random-paired.tsv)

To regenerate a set, substitute the desired size, count and length from
the per-cube file. For example, the 5x5x5 validation set can be replayed
with each release binary from its repository root.

```sh
anycube scrambles --size 5 --length 150 --count 48 --seed 6180088 > /tmp/anycube-validation-5.txt
anycube bench5 /tmp/anycube-validation-5.txt --out /tmp/anycube-validation-5.tsv
```

## Faster defaults on the same 212 random scrambles

The previous default at `331cb64` included extra searches for short solutions. The timed default at `3a080fc` uses the search budgets and reduction choices from before that experiment (`38fe0d4`). Full mode retains the alternative 4x4x4 openings, 5x5x5 reduction endings, reflected finishes on 6x6x6 and 7x7x7, and joint selection of the last two wing reductions on larger cubes.

On 2026-11-18 both release binaries solved the same random-move scrambles on the 10-core Apple M1 Pro with 16 GB of RAM. Each size ran in two batches. The execution order reversed between them, with one solve running at a time. The figures are mean moves in OBTM and mean wall seconds per cube. Timing starts after the solver loads and includes work first triggered during a solve. The 2x2x2 and 3x3x3 batch command rounds individual times to 0.01 s. Sizes 16 through 21 have three samples each.

| Cube | Cases | Previous moves | Current moves | Previous seconds | Current seconds |
|---|---:|---:|---:|---:|---:|
| 2x2x2 | 20 | 8.80 | 8.80 | <0.005 | <0.005 |
| 3x3x3 | 20 | 17.75 | 17.75 | 0.201 | 0.210 |
| 4x4x4 | 48 | 40.40 | 40.54 | 1.825 | 1.184 |
| 5x5x5 | 48 | 67.00 | 67.46 | 1.063 | 0.581 |
| 6x6x6 | 10 | 110.50 | 110.50 | 5.145 | 4.701 |
| 7x7x7 | 10 | 151.30 | 151.40 | 2.774 | 2.529 |
| 8x8x8 | 6 | 229.17 | 230.00 | 4.550 | 2.645 |
| 9x9x9 | 6 | 300.50 | 301.17 | 3.552 | 2.315 |
| 10x10x10 | 6 | 387.00 | 388.33 | 6.389 | 5.113 |
| 11x11x11 | 4 | 485.75 | 487.25 | 8.127 | 6.506 |
| 12x12x12 | 4 | 589.00 | 589.75 | 9.502 | 8.656 |
| 13x13x13 | 4 | 700.00 | 701.75 | 10.829 | 9.412 |
| 14x14x14 | 4 | 827.75 | 828.00 | 14.168 | 13.011 |
| 15x15x15 | 4 | 952.75 | 953.75 | 18.854 | 17.056 |
| 16x16x16 | 3 | 1093.00 | 1093.67 | 17.469 | 16.307 |
| 17x17x17 | 3 | 1237.00 | 1237.67 | 25.105 | 23.157 |
| 18x18x18 | 3 | 1416.67 | 1417.00 | 28.930 | 27.287 |
| 19x19x19 | 3 | 1563.33 | 1563.33 | 38.543 | 36.999 |
| 20x20x20 | 3 | 1773.33 | 1774.33 | 35.977 | 35.416 |
| 21x21x21 | 3 | 1943.33 | 1944.33 | 40.671 | 38.725 |

The current default reproduced the original fast solver’s exact sequence on every one of the 212 positions. All solutions passed sticker replay. The previous default saved seven moves over all 48 cases on the 4x4x4 and 22 over all 48 on the 5x5x5.

The current default took 35.1% less time on the 4x4x4 and 45.3% less on the 5x5x5 than the previous default. It finished sooner on 48 of 48 and 47 of 48 scrambles respectively. The per-cube measurements include the seed, scramble length and move counts for all three versions. The original fast timing comes from the earlier experiment. The final two columns record the repeat on sizes 4 and 5 after the full-mode change. [Per-cube timings and lengths](default-speed-paired.tsv)

### Search work avoided

The 3x3x3 finish uses the existing eight-move bitmap when it recovers an exact distance, and skips subgroup depths below the closest root. Once the beam detects a possible finish in one move on its final level, it can enumerate all eligible finishes directly in the original tie order.

The first 6x6x6 centre step uses an index of the last three moves. The inner-centre mask selects the possible endings. The differing oblique centres rule out paths that cannot repair those slots. The remaining paths are replayed in the same order as the forward search. On larger cubes, centre matching writes directly into the score array and stops counting mismatching centres once a position exceeds the ending search’s limit.

Compared with the restored search before these changes, the first 6x6x6 centre step used 783 → 310 ms of processor time per cube on 12 additional random 300-move scrambles (seed 786301). Matching the first row of centres took 0.79 → 0.69 s on 8 staged 8x8x8 cubes and 3.30 → 3.14 s on 8 staged 11x11x11 cubes. These are wall times with four cases each at seeds 81207 and 82130. All comparisons use alternating batches. These timings cover only the specified step.

At the restored search settings, the solver before and after the speed-only changes took 1.348 → 1.302 s on 4x4x4 and 0.557 → 0.611 s on 5x5x5. This separate comparison used 24 development scrambles per size and reproduced all 48 solution sequences.

The 5x5x5 medians at fixed search settings were 0.520 and 0.522 s. The first timed cube took 0.560 and 1.912 s respectively and is included in the means.

An exact reverse index of the last five 5x5x5 reduction moves gave the same solutions on 24 development scrambles and took 0.604 s per solve against 0.557 s without the index. It was dropped.

Direct sticker input gave 41 moves on the first 4x4x4 and 68 on the first 5x5x5 in default mode. Full mode on the first position of sizes 4 through 9 gave 39, 68, 108, 147, 224, 302 moves respectively. The original solver applied each answer to the scrambled stickers and checked the result.

### Keeping short solutions in full mode

Full mode at `ebabc0a` also compares the compact and wide 4x4x4 and 5x5x5 searches. Their reduction pools can contain different endpoints. On 4x4x4 case 1, the wide search gave 41 moves and the combined search recovered 39. On case 2, the wide search supplied the 38-move winner while the compact search found 40. On 5x5x5 case 14, keeping the compact result improved 66 to 65 moves. All six selected full-mode checks returned the shorter of the two verified results.

The full-mode change was followed by another paired default run on all 48 cases per size. Previous and final times were 1.854 → 1.177 s on the 4x4x4 and 1.060 → 0.592 s on the 5x5x5. All 96 final default solutions matched the table move for move. A further default solve at each of sizes 6, 7, 8, 9 and 21 also matched the timed version and passed independent sticker replay.

The scramble generator uses `6175103 + 997 * N` for each size. The same CLI commands regenerate the set for each version. For example, these commands time the 48 default 5x5x5 solves.

```sh
anycube scrambles --size 5 --length 150 --count 48 --seed 6180088 > /tmp/anycube-speed-5.txt
anycube bench5 /tmp/anycube-speed-5.txt --out /tmp/anycube-speed-5.tsv
```

## Wing insertions for shorter 4x4x4 and 5x5x5 solutions

An exact local rewrite examined every interval of two through ten moves in the 48 default solutions for each size. A breadth-first index of all sticker permutations within four turns, combined with up to two final turns, tested all replacements shorter than the interval and at most six turns long. It shortened 0/48 words at each size. This test distinguished all sticker positions and kept the coordinate frame fixed.

A second experiment allowed a cycle of three physical pieces to be inserted anywhere in a word. It used the 36 outer and wide turns, plus each inner-slice turn written as a wide turn and an opposite-power face turn. The candidate library consisted of commutators of a single turn with a conjugated turn. Moves on a common axis were collected and cancelled. The library kept a shortest representative found for each permutation that cycled just three wings or just three centres. At each cut the required permutation was conjugated by the remaining suffix. Multiplying the complete labelled-sticker permutations verified the resulting word in every trial. This adapts the insertion method described for the 3x3x3. [Sebastiano Tronto’s Fewest Moves Tutorial, chapter 2](https://fmcsolves.cubing.net/fmc_tutorial_ENG.pdf)

Each of the 48 ordinary words per size received 24 randomly sampled wing cycles and 24 centre cycles from the corresponding library. The standalone column counts the moves of the chosen representative in isolation. The insertion column gives the net cost above the original word at the best cut found.

| Cube | Three pieces cycled | Distinct library entries | Trials | Mean standalone moves | Mean net inserted moves |
|---|---|---:|---:|---:|---:|
| 4x4x4 | Wings | 1,920 | 1,152 | 10.46 | 7.61 |
| 4x4x4 | Centres | 2,112 | 1,152 | 13.30 | 10.20 |
| 5x5x5 | Wings | 1,920 | 1,152 | 10.44 | 7.47 |
| 5x5x5 | Centres | 2,160 | 1,152 | 13.27 | 9.84 |

A possible full-mode search would accept a reduction with a single three-wing cycle. It could pair those wings virtually for the 3x3x3 finish, carry their actual positions, then insert the correction at its cheapest cut. The insertion measurements reused ordinary complete words. The length of the relaxed search and its complete sticker-verified answer are what would determine the benefit on random cubes.

A second sampling scheme included all 4,048 directed wing triples reachable by conjugating these cycles with ordinary turns. It selected 24 uniformly per benchmark word. The 4x4x4 library supplied an insertion in 1,148 of 1,152 trials, at a mean net cost of 7.79 moves among hits. The 5x5x5 supplied 1,152 of 1,152, at 7.52 moves. For comparison the same test on centre cycles covered 1,152 of 1,152 on the 4x4x4 and 645 of 1,152 on the 5x5x5. The odd cube has two separate centre orbits.

A search for partial solutions by editing existing answers examined windows up to 14 turns long. It tried every shorter replacement of up to four turns with the same corner action, including both widths, and tested the actual sticker colours. On the 48 cases per size it found zero shorter solved words and zero shorter words leaving only one wing cycle. A search that accepts the defect while constructing the reduction is the next test. At the measured correction costs, the upper targets of 37 and 60 moves would require partial solutions averaging about 29 and 52 moves respectively.

On the same 48 solutions the final wing-reduction phase averaged 11.125 moves on the 4x4x4 and 13.688 on the 5x5x5. The other phases, including the 3x3x3 finish, averaged 29.417 and 53.771 moves. Even deleting the whole final wing phase and adding back the measured average insertion cost gives 37.21 and 61.29 moves at those other phase lengths. This calculation motivates choosing the three wings earlier in the reduction and selecting candidates together with their 3x3x3 finish. It is a comparison with the observed means, and gives no lower bound for a different search.

The [standalone example](../examples/wing_insertions.rs) reads one scramble and partial solution per line, separated by a tab, and prints the scramble with the shortest completed word it finds. It walks the actual and desired sticker colours backwards along the partial solution and checks the whole answer on the forward sticker model. It repaired all 24 planted partial solutions per size on a separate CLI replay with the original binary. Those inputs deliberately left one wing cycle.

```sh
cargo run --release --example wing_insertions -- 4 < partial-solutions.tsv > completed.tsv
anycube verify --size 4 completed.tsv
```

For example, the 4x4x4 scramble `D' U' F' Uw' U R' U R Uw U' R2` and the partial sequence `R U' R F D U` leave three wings. The example returns the verified 11-turn solution `R2 U Uw' R' U' R U' Uw F U D`.
