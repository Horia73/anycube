# Benchmarks

Mean moves and mean seconds per cube on the fixed scrambles in this
directory, on an Apple M1 Pro. A turn of a face with any number of layers
counts as one move.

## Against other solvers

Moves, lower is better:

| Solver | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| **cubenxnxn v0.1.0** | **40.7** | **67.8** | **108.9** | **152.8** | **235.7** | **314.7** | **406.1** |
| [TPR](https://github.com/cs0x7f/TPR-4x4x4-Solver) | 44.5 | | | | | | |
| [cube555](https://github.com/cs0x7f/cube555) | | 70.6 | | | | | |
| [dwalton76](https://github.com/dwalton76/rubiks-cube-NxNxN-solver) | 51.7 | 79.8 | 136.1 | 194.4 | 308.4 | 368.0 | 523.4 |

Seconds, lower is better:

| Solver | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| **cubenxnxn v0.1.0** | 2.8 | **0.58** | **8.4** | **3.8** | **12.9** | **3.7** | **16.6** |
| TPR | **0.14** | | | | | | |
| cube555 | | 1.1 | | | | | |
| dwalton76 | 1.6 | 4.6 | 43 | 9.4 | 15.8 | 11.0 | 24.8 |

Every solver starts at the first scramble of the same file. dwalton76's
solver was run on fewer cubes than ours (100, 40, 10, 10, 5, 5 and 5 for
sizes 4 to 10) and not above 10x10x10. Nobody else has reproduced these
numbers yet; a report of different results is welcome.

## The newest release in full

<!-- release:begin -->
Release **v0.1.0** (commit `b2f038a`, 2026-11-11), default mode:

| Cube | Moves | Seconds | Slowest cube | Memory | Cubes |
|---|---:|---:|---:|---:|---:|
| 2x2x2 | 8.79 | 0.00 | 0.00 | 6 MB | 100 |
| 3x3x3 | 19.03 | 0.22 | 0.42 | 77 MB | 100 |
| 4x4x4 | 40.71 | 2.8 | 3.8 | 3.0 GB | 100 |
| 5x5x5 | 67.75 | 0.58 | 2.5 | 2.9 GB | 200 |
| 6x6x6 | 108.85 | 8.4 | 10.8 | 3.7 GB | 20 |
| 7x7x7 | 152.75 | 3.8 | 4.7 | 4.0 GB | 20 |
| 8x8x8 | 235.65 | 12.9 | 23.4 | 4.4 GB | 20 |
| 9x9x9 | 314.65 | 3.7 | 12.7 | 4.1 GB | 20 |
| 10x10x10 | 406.05 | 16.6 | 32.7 | 5.3 GB | 20 |
| 11x11x11 | 510.40 | 7.5 | 13.1 | 3.7 GB | 5 |
| 12x12x12 | 619.80 | 21.7 | 28.0 | 4.4 GB | 5 |
| 13x13x13 | 757.80 | 15.8 | 24.2 | 4.0 GB | 5 |
| 14x14x14 | 888.40 | 30.4 | 36.9 | 4.3 GB | 5 |
| 15x15x15 | 1031.80 | 21.4 | 29.0 | 4.3 GB | 5 |
| 16x16x16 | 1185.60 | 40.3 | 45.6 | 4.1 GB | 5 |
| 17x17x17 | 1392.00 | 37.0 | 75.5 | 4.1 GB | 5 |
| 18x18x18 | 1548.00 | 45.8 | 59.8 | 4.1 GB | 5 |
| 19x19x19 | 1763.80 | 49.5 | 72.7 | 3.9 GB | 5 |
| 20x20x20 | 1938.60 | 56.5 | 87.8 | 4.2 GB | 5 |
| 21x21x21 | 2254.00 | 84.2 | 135 | 3.8 GB | 5 |
<!-- release:end -->

Seconds are per cube with the lookup tables already made. Memory is the most
the whole run held at once, tables included.

## Reproducing

```bash
# one size: cubes, mean moves, mean seconds, slowest cube
./target/release/cubenxnxn row bench/scrambles101010-20.txt --size 10

# every size, recorded under a release name, then the tables written
scripts/release.py measure v0.1.0
scripts/release.py readme
```

The other solvers' results are in `tpr-results.tsv`, `cube555-results.tsv`
and `dwalton*-results.tsv`, cube by cube. `RESULTS.md` is the dated log of
every measurement, of what was tried and of what did not pay.
