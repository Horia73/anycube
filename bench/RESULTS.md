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
