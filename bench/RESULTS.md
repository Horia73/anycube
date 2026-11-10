# Benchmark results

Machine: Apple M1 Pro (8 performance cores), 16 GB RAM. Date: 2026-11-09.
Set: `scrambles-1000.txt`, 1,000 scrambles of 100 random moves each (seed 444).
Metric: OBTM. Every solution was applied to the cube model and verified.

## Mean length per stage

| Solver | Phase 1 | Phase 2 | Phase 3 | 3x3x3 | Total | Min | Max |
|---|---|---|---|---|---|---|---|
| TPR (Java, `tpr-results.tsv`) | 6.727 | 5.847 | 12.362 | 19.638 | 44.574 | 40 | 48 |
| cube444 `--n3 1 --probes 500` (`m1-results.tsv`) | 6.666 | 6.021 | 12.323 | 19.828 | 44.838 | 40 | 48 |
| cube444 default (`n3-2000-results.tsv`) | 6.652 | 6.223 | 12.605 | 17.438 | 42.918 | 38 | 47 |

## Time per solve

| Solver | Mean | Median | Max | Cubes timed |
|---|---|---|---|---|
| TPR (Java, after JIT warm-up) | 146 ms | 121 ms | 2.4 s | 1,000 |
| cube444 `--n3 1 --probes 500` | 121 ms | 101 ms | 1.4 s | 1,000 |
| cube444 default (`timing-200.tsv`) | 950 ms | 748 ms | 6.6 s | 200 |

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
| cube444 default | 12.775 | 10.895 | 17.000 | 40.670 | 37 | 43 |

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
| cube444, previous default (8 s limit) | 12.775 | 10.895 | 17.000 | 40.670 | 37 | 43 |
| cube444, new default (4 s limit) | 12.802 | 10.809 | 16.986 | 40.597 | 37 | 44 |

Length histogram: 37: 1, 38: 13, 39: 89, 40: 311, 41: 467, 42: 112, 43: 4, 44: 3.

Against TPR on the same cube: 3.98 moves shorter on average, equal on 16
cubes, longer on none.

Time per solve: mean 2.47 s (was 3.67 s), median 2.32 s, 99th percentile
3.84 s, max 5.32 s. 6 cubes took more than the 4 s soft limit, 1 more than 5 s.
