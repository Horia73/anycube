# Benchmarks

How cubenxnxn compares with the other solvers we could run, on the same
scramble files, on the same machine (Apple M1 Pro). Moves are counted in
OBTM: any turn of a face, with any number of layers, is one move.

cubenxnxn is release v0.1.0 in its default mode; the numbers are the ones in
the [README](../README.md#results).

## Moves

Mean solution length; lower is better.

| Solver | 4x4x4 | 5x5x5 | 6x6x6 | 7x7x7 | 8x8x8 | 9x9x9 | 10x10x10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| **cubenxnxn v0.1.0** | **40.71** | **67.75** | **108.85** | **152.75** | **235.65** | **314.65** | **406.05** |
| [TPR](https://github.com/cs0x7f/TPR-4x4x4-Solver) | 44.50 | | | | | | |
| [cube555](https://github.com/cs0x7f/cube555) | | 70.58 | | | | | |
| [dwalton76](https://github.com/dwalton76/rubiks-cube-NxNxN-solver) | 51.71 | 79.78 | 136.10 | 194.40 | 308.40 | 368.00 | 523.40 |
| cubenxnxn against the shortest other | -8.5% | -4.0% | -20.0% | -21.4% | -23.6% | -14.5% | -22.4% |

## Seconds

Mean time per cube, lookup tables already made; lower is better.

| Solver | 4x4x4 | 5x5x5 | 6x6x6 | 7x7x7 | 8x8x8 | 9x9x9 | 10x10x10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| **cubenxnxn v0.1.0** | 2.8 | **0.58** | **8.4** | **3.8** | **12.9** | **3.7** | **16.6** |
| TPR | **0.14** | | | | | | |
| cube555 | | 1.1 | | | | | |
| dwalton76 | 1.6 | 4.6 | 43 | 9.4 | 15.8 | 11.0 | 24.8 |

On the 4x4x4 TPR is twenty times quicker and dwalton76 is quicker too;
cubenxnxn spends its time looking for a shorter solution.

## Cubes

How many cubes each number is a mean of. Every solver starts at the first
scramble of the same file, so a smaller set is the beginning of a larger one.

| Solver | 4x4x4 | 5x5x5 | 6x6x6 | 7x7x7 | 8x8x8 | 9x9x9 | 10x10x10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| cubenxnxn v0.1.0 | 100 | 200 | 20 | 20 | 20 | 20 | 20 |
| TPR | 100 | | | | | | |
| cube555 | | 200 | | | | | |
| dwalton76 | 100 | 40 | 10 | 10 | 5 | 5 | 5 |

| Size | Scramble file |
|---|---|
| 4x4x4 | `scrambles-1000.txt` |
| 5x5x5 | `scrambles555-200.txt` |
| 6x6x6, 7x7x7 | `scrambles666-100.txt`, `scrambles777-100.txt` |
| 8x8x8 to 10x10x10 | `scrambles888-20.txt`, `scrambles999-20.txt`, `scrambles101010-20.txt` |

## What to keep in mind

- **The sets for the large cubes are small.** dwalton76's solver was run on
  5 or 10 cubes of each size from 6 up; the means on so few cubes can move by
  a few moves.
- **The times were not all taken on the same day.** The other solvers were
  timed while this one was being written, cubenxnxn at its release; all on
  the same machine.
- **The other solvers were not run above 10x10x10**, so sizes 11 to 21 have
  only our own numbers, in the README.
- **Nobody else has reproduced these numbers yet.** Everything needed is in
  this directory; a report of different results is welcome.

## Reproducing

One size, the row of the README's table (cubes, mean moves, mean seconds,
slowest cube):

```bash
./target/release/cubenxnxn row bench/scrambles101010-20.txt --size 10
```

All sizes of a release:

```bash
scripts/release.py measure v0.1.0
```

Checking a file of solutions, one `scramble<TAB>solution` per line, on the
sticker model:

```bash
./target/release/cubenxnxn verify --size 10 <file>
```

## Files

| File | Contents |
|---|---|
| `releases.tsv` | cubenxnxn by release and size: cubes, moves, seconds, slowest cube, memory |
| `scrambles*.txt` | the fixed scrambles, one per line |
| `tpr-results.tsv` | TPR on the 1,000 4x4x4 scrambles, cube by cube |
| `cube555-results.tsv` | cube555 on the 200 5x5x5 scrambles |
| `dwalton*-results.tsv` | dwalton76's solver by size: cube, moves, seconds, solution |
| the other `.tsv` and `.txt` files | runs of single modes and experiments, described where they were made in `RESULTS.md` |
| `RESULTS.md` | the dated log: every measurement, what was tried and what did not pay |
