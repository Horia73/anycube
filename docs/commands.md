# Commands

| Command | What it does |
|---|---|
| `solve --size N <moves>` | solve a scramble given as moves |
| `solve --state <facelets>` | solve a cube given as it looks, 6·N·N stickers in URFDLB order; the size is read from their number |
| `facelets --size N <moves>` | print the facelets of a scrambled cube |
| `scrambles --size N --count C --length L` | random-move scrambles, one per line |
| `verify --size N <file>` | check solutions; each line is `scramble<TAB>solution` |
| `row <file> --size N` | solve a scramble file of any size: mean moves and seconds on one line |
| `bench`, `bench5`, `bench6`, `bench7 <file>` | the same for 4x4x4 to 7x7x7 with the lengths of every step |
| `benchn <file> --size N` | and for sizes from 8 up |

The row of one size in the results table, and the whole table for a release:

```bash
./target/release/anycube row bench/scrambles101010-20.txt --size 10
```

```bash
scripts/release.py measure v0.2.0
```

A minor release takes the place of the newest row instead of adding one:

```bash
scripts/release.py measure v0.1.1 --minor
```

```bash
scripts/release.py readme
```

To make new scrambles or look at a cube:

```bash
./target/release/anycube scrambles --size 12 --count 5 --length 600
```

```bash
./target/release/anycube facelets --size 3 "U2 R' F2 L2 U2 R F L2 B' U' L' B2 R2 D2 R2 B D R D' F2 L U2 F' L2 D'"
```

The flags behind `--mode` are in [search-flags.md](search-flags.md).
