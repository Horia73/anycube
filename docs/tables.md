# Lookup tables

Generated on first use into `tables/` (set `ANYCUBE_TABLES` to move them). The installed
program keeps them with the user's application data instead
(`~/Library/Application Support/anycube/tables` on macOS,
`~/.local/share/anycube/tables` on Linux), unless a `tables/` directory is
where it is run.
The 4x4x4 set is 4.9 GB; its large files take about four minutes on 8 cores,
once.

| File | Size | Time on 8 cores | Contents |
|---|---|---|---|
| `centres-exact.bin` | 2.37 GB | 105 s | moves to stage all centres, every centre state |
| `slice-exact.bin` | 1.72 GB | 55 s | 3x3x3 distance to "no twist, no flip, slice edges in place" |
| `phase1-exact.bin` | 554 MB | 20 s | 3x3x3 distance to the domino subgroup |
| `pairing.bin` | 240 MB | 20 s | moves to pair the dedges in phase 3 |
| nine small files | 70 MB | under 1 s | everything else, so that start-up reads instead of rebuilding |

The 2x2x2 solver needs only `cube222.bin` (3.7 MB) and the 3x3x3 solver only
`cube333.bin` (10 MB), plus `slice-exact.bin` for `--mode full`.

The library also builds for `wasm32-unknown-unknown`, with the 2x2x2 and
3x3x3 solvers only. There is no file system there, so they build their
tables in memory (0.1 s and 0.7 s), and the 3x3x3 runs without
`phase2-333.bin`.

The 5x5x5 solver has its own tables, 2.1 GB, generated in about two minutes on
8 cores: `phase4-555-*.bin` (850 MB, phase P), `phase5-555*.bin` (600 MB,
phase E), `phase2-555.bin` (331 MB, centres), `phase3-555-sym.bin` (175 MB,
phase C, one entry per class under its 16 symmetries) and four small ones. It
also uses `phase1-exact.bin` for the 3x3x3 step, `stage555-near8.bin`
(320 MB, 30 s: the first centre phase near its goal) and `phase2-333.bin`
(57 MB, 3 s: corners and U/D edges of the 3x3x3 together, used by every size). `--joint` needs two more,
`centres555-x.bin` and `centres555-t.bin` (300 MB each, 100 s to generate).

The 6x6x6 and 7x7x7 solvers add about 0.7 GB (`axes-*.bin`, `stage666-*.bin`,
`stage777-*.bin`).

Sizes from 8 up need no tables of their own size: they use the ones above,
and twelve small ones for matching rows of centres (`ring-*.bin`, 24 MB
each), which every size shares. With everything generated the directory
holds about 8.5 GB.
