# Search flags

`--mode fast`, `normal` or `full` is all most uses need; these are the flags a mode sets.

## 3x3x3

| Flag | fast | normal | full | Meaning |
|---|---|---|---|---|
| `--depth` | 12 | 14 | 14, then 15 | longest first phase of the two-phase search that is tried in full |
| `--variants` | 6 | 6 | 6 | how many of the six variants (three axes, the cube and its inverse) are tried at that length; the shorter lengths are tried in all six |
| `--time` | | | 120000 | milliseconds the search for the optimal solution may take; past them the two-phase solution is returned |

Each move of `--depth` takes about 13 times as long. On the 100 scrambles
of `bench/scrambles333-100.txt`:

| `--depth` | 12 | 13 | 14 | 15 |
|---|---:|---:|---:|---:|
| Moves | 18.84 | 18.41 | 18.13 | 17.87 |
| Seconds | 0.002 | 0.02 | 0.20 | 2.9 |

`full` asks for 15 only when 14 has found nothing under 19 moves, and then
only for a solution of 18 or fewer.

## 5x5x5

`--mode fast|normal|full` sets all of them; each can be overridden.

| Flag | fast | normal | full | Meaning |
|---|---|---|---|---|
| `--time` | 800 | 4000 | 0 | milliseconds the solve aims at; 0 is no limit |
| `--staged` | 400 | 1200 | 4000 | centre stagings kept |
| `--extra` | 2 | 3 | 3 | lengths of the first centre phase searched past its shortest |
| `--a-budget` | 200000 | 1000000 | 10000000 | positions each of those lengths may take from the second on; a length that does not fit is searched in part; 0 is no limit |
| `--labellings` | 8 | 16 | 16 | ways of splitting the wings tried per staging |
| `--c-keep` | 2000 | 6000 | 20000 | results phase C aims for |
| `--p-keep` | 300 | 700 | 2000 | results phase P aims for |
| `--e-keep` | 50 | 100 | 400 | reduced cubes aimed for |
| `--pool`, `--depth` | 30, 11 | 100, 11 | 400, 11 | reduced cubes whose 3x3x3 is searched past a quick solve, and the longest first phase of that search |
| `--deeper` | none | 12:30,13:8 | 12:100,13:24,14:4 | then a longer first phase for fewer of them: `length:count` steps; `0` for none |
| `--joint N` | off | off | off | also stage the centres in one step, exactly, N lengths past the shortest; minutes per cube |


## 4x4x4

| Flag | Meaning | Default |
|---|---|---|
| `--mode` | `fast`, `normal` or `full`; sets the flags below | normal |
| `--time` | soft time limit per cube, in ms | 4000 |
| `--n2` | joint-phase endpoints collected | 10000 |
| `--joint-nodes` | node budget of the joint phase, in millions | 1000 |
| `--k3` | endpoints tried in phase 3 | 10000 |
| `--n3` | reductions compared by total length | 2000 |
| `--extra3` | moves past the shortest reduction to keep collecting | 1 |
| `--first` | longest first phase of the 3x3x3 search tried for every reduction | 11 |
| `--deeper` | then a longer first phase for the reductions with the shortest totals: `length:count` steps (`all` for every one, `--deeper 0` for none) | 13:48 |
| `--split` | search phases 1 and 2 separately, as TPR does | off |
| `--n1`, `--k1` | with `--split`: phase-1 solutions enumerated and kept | 10000, 500 |
| `--exact` | node budget (millions) of the exact 3x3x3 search | 0 (off) |
| `--exact-cap` | longest 3x3x3 solution the exact search looks for | 15 |
| `--far-g` | also keep longer reductions this close to a domino subgroup | 0 (off) |

`--mode full` collects fifteen times as many endpoints and adds the exact
3x3x3 search (`src/opt3.rs`), which scans the reductions for 3x3x3 solutions
of up to 16 moves that the two-phase search missed.
