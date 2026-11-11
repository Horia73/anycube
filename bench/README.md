# Benchmarks

Mean moves and mean seconds per cube on the fixed scrambles in this
directory, on an Apple M1 Pro. A turn of a face with any number of layers
counts as one move.

## Against other solvers

Moves, lower is better:

| Solver | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| **anycube v0.1.0** | **40.7** | **67.8** | **108.9** | **152.8** | **235.7** | **314.7** | **406.1** |
| [dwalton76](https://github.com/dwalton76/rubiks-cube-NxNxN-solver) | 51.7 | 79.8 | 136.1 | 194.4 | 308.4 | 368.0 | 523.4 |
| [Chen Shuang](https://github.com/cs0x7f) | 44.5 | 70.6 | | | | | |
| [gullicube](https://github.com/rokicki/gullicube) | 97.3 | 166.3 | 273.7 | 396.3 | 566.0 | 703.0 | 890.3 |

Seconds, lower is better:

| Solver | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| **anycube v0.1.0** | 2.8 | **0.58** | **8.4** | **3.8** | 12.9 | **3.7** | 16.6 |
| dwalton76 | 1.6 | 4.6 | 43 | 9.4 | 15.8 | 11.0 | 24.8 |
| Chen Shuang | **0.14** | 1.1 | | | | | |
| gullicube | 9.8 | 8.1 | 8.5 | 4.2 | **8.9** | 6.0 | **10.5** |

Chen Shuang's solvers are [TPR](https://github.com/cs0x7f/TPR-4x4x4-Solver)
for the 4x4x4 and [cube555](https://github.com/cs0x7f/cube555) for the 5x5x5.
Every solver starts at the first scramble of the same file. dwalton76's
solver was run on fewer cubes than ours (100, 40, 10, 10, 5, 5 and 5 for
sizes 4 to 10) and not above 10x10x10. Nobody else has reproduced these
numbers yet; a report of different results is welcome.

gullicube was run on the first 3 cubes of each size, with its own time budget
of about ten seconds. It counts one move for every layer turned; here its
solutions are counted like the others, turns on one axis that follow each
other merged into the fewest turns of a face. Counted its own way it is the
shorter one from 8x8x8 up: 344 turns of a layer against our 455 on the 8x8x8,
528 against 930 on the 10x10x10.

anycube on every size from 2 to 21, with the slowest cube and the memory
of each: the [README](../README.md#results).

## Reproducing

```bash
# one size: cubes, mean moves, mean seconds, slowest cube
./target/release/anycube row bench/scrambles101010-20.txt --size 10

# every size, recorded under a release name, then the tables written
scripts/release.py measure v0.1.0
scripts/release.py readme
```

The other solvers' results are in `tpr-results.tsv`, `cube555-results.tsv`,
`dwalton*-results.tsv` and `gullicube*-results.tsv` (scramble and solution,
written in wide turns), cube by cube. `RESULTS.md` is the dated log of
every measurement, of what was tried and of what did not pay.
