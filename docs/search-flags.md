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
| `--time` | 0 | 0 | 0 | milliseconds the solve aims at; 0 is no limit. With a limit the steps stop by the clock, and the same cube gives another solution on another run |
| `--staged` | 400 | 1200 | 4000 | centre stagings kept |
| `--extra` | 2 | 3 | 3 | lengths of the first centre phase searched past its shortest |
| `--a-budget` | 200000 | 1000000 | 10000000 | positions each of those lengths may take from the second on; a length that does not fit is searched in part; 0 is no limit |
| `--labellings` | 8 | 16 | 16 | ways of splitting the wings tried per staging |
| `--c-keep` | 2000 | 20000 | 20000 | results phase C aims for |
| `--p-keep` | 300 | 700 | 2000 | results phase P aims for |
| `--e-keep` | 50 | 100 | 400 | reduced cubes aimed for |
| `--pool`, `--depth` | 30, 11 | 100, 11 | 400, 11 | reduced cubes whose 3x3x3 is searched past a quick solve, and the longest first phase of that search |
| `--deeper` | none | 12:30,13:8 | 12:100,13:24,14:4 | then a longer first phase for fewer of them: `length:count` steps; `0` for none |
| `--joint N` | off | off | off | also stage the centres in one step, exactly, N lengths past the shortest; minutes per cube |


## 4x4x4

| Flag | Meaning | Default |
|---|---|---|
| `--mode` | `fast`, `normal` or `full`; sets the flags below | normal |
| `--time` | soft time limit per cube, in ms; with one, the same cube gives another solution on another run | 0 (none) |
| `--n2` | joint-phase endpoints collected | 10000 |
| `--joint-nodes` | positions the joint phase may visit once it has an endpoint, in millions: what ends it | 20 |
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

`--mode full` gives the joint phase twenty times the positions, collects
fifteen times as many endpoints and adds the exact 3x3x3 search
(`src/opt3.rs`), which scans the reductions for 3x3x3 solutions of up to 16
moves that the two-phase search missed.

No mode runs by a clock: the joint phase ends when its budget of positions
is spent, counted in a fixed order of its searches, so the same cube gives
the same solution on every run and every machine. On a slower machine a
solve takes longer and is no worse.

## Projected face turns on the 4x4x4 and 5x5x5

`solve --lift` tries to shorten the ordinary solution. It works from the
stickers, whether the input is a scramble or a facelet string. A 5x5x5
turn such as `R` or `Rw` moves the corners and middle edges the same way.
The search proposes sequences on that 3x3x3 and uses the wings and centres
to decide how many layers each turn needs. It also tries pairs of turns
such as `R' Rw` that move an inner layer while leaving the 3x3x3 fixed. On
a 4x4x4 it tries several choices of virtual middle edges. Each result is
replayed on the original stickers.

`--holds N` tries the ordinary solver in N orientations, from 1 (default)
to 6, before the projected search. The reported lengths show what each
step contributed. The reduction search flags above apply to each
orientation. Searching for a new ending adds time after the reductions.

On the 4x4x4, `--last-face` tries each of the 18 outer turns at the solved
end. The fast solver searches the resulting auxiliary cube. On return,
the final face turn is expressed in the orientation that solver reached
and combined with any turn of the same face on the final axis. This works
with indistinguishable centres because an outer turn only permutes the
four centres within a face.

On the 5x5x5, `--shell4` adds another source of possible endings. Removing
the middle row and column of each face gives a real 4x4x4. The search
solves that shell, tries both sides of the axis for its wide turns and
keeps the shorter endings that also solve the middle band. This option
loads the 4x4x4 tables as well as the 5x5x5 tables.

```sh
anycube solve --size 4 --lift --holds 6 --last-face "Rw U Fw2 D' Bw R2"
anycube solve --size 5 --lift --holds 6 --shell4 "Rw U Fw2 D' Bw R2"
anycube bench-lift bench/scrambles555-200.txt --size 5 --holds 6 --shell4 --limit 30 --out lift.tsv
```

The benchmark TSV records the zero-based row, ordinary length, best length
across orientations, length after the projected 3x3x3 search, final length,
milliseconds and the final moves.

If another search has an entire sequence of faces, directions and powers,
`widths` can test it directly. It propagates each piece's possible
positions from both ends and assigns one shared width to each move. It
reports a solution, an impossible word, or an exhausted search budget.
For this example, the supplied sequence is consistent with the scramble
once the middle move turns two layers.

```sh
state=$(anycube facelets --size 5 "F2 Rw U'")
anycube widths --size 5 --state "$state" --word "U R' F2"
```

`bench-widths` measures this last step in isolation. It takes the first L
turns of each input scramble and supplies the projected inverse with every
width erased. The corresponding sticker-only benchmark is `bench-lift`.
[Measurements for both experiments](../bench/RESULTS.md#projected-face-turns-on-4x4x4-and-5x5x5-2026-11-17)
