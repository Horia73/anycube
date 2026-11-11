<p align="center">
  <img src="docs/logo.svg" width="96" alt="">
</p>

<h1 align="center">cubenxnxn</h1>

<p align="center">
  <b>Any cube, solved short.</b><br>
  A Rubik's cube solver for every size from 2x2x2 up, written in Rust.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/release-v0.1.0-2f6fde" alt="release v0.1.0">
  <img src="https://img.shields.io/badge/cubes-2x2x2_to_21x21x21-1faa59" alt="cubes 2x2x2 to 21x21x21">
  <img src="https://img.shields.io/badge/written_in-Rust-f58220" alt="written in Rust">
  <img src="https://img.shields.io/badge/licence-MIT-d92b3a" alt="licence MIT">
</p>

<p align="center">
  <a href="#results">Results</a> ·
  <a href="#install">Install</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#how-it-works">How it works</a> ·
  <a href="bench/README.md">Benchmarks</a> ·
  <a href="#thanks">Thanks</a>
</p>

---

cubenxnxn finds **short solutions in seconds** for cubes of **any size**. It
is measured on every size from 2x2x2 to 21x21x21, the largest cube that is
mass-produced.

- **2x2x2 and 3x3x3**: optimal solutions (always for 2x2x2, with `--mode full`
  for 3x3x3), or a two-phase solution in a fraction of a second.
- **4x4x4 to 7x7x7**: a solver written for each size, tuned for length.
- **8x8x8 and up**: one general solver that does not know the size. It treats
  groups of layers as smaller cubes and reuses the solvers above.
- Every solution is applied to a full model of the cube and checked before it
  is printed.

The project is young and under active work: each release gets its rows in
the tables below, so the progress can be followed.

## Results

Mean moves and mean seconds per cube. A turn of a face with any number of
layers counts as one move.

<!-- releases:begin -->
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

Every release, the newest first: a row of mean moves and a row of mean seconds per cube.

| Release | | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 21 |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **v0.1.0** | moves | 8.8 | 19.0 | 40.7 | 67.8 | 108.9 | 152.8 | 235.7 | 314.7 | 406.1 | 510.4 | 619.8 | 757.8 | 888.4 | 1031.8 | 1185.6 | 1392.0 | 1548.0 | 1763.8 | 1938.6 | 2254.0 |
| | seconds | 0.00 | 0.22 | 2.8 | 0.58 | 8.4 | 3.8 | 12.9 | 3.7 | 16.6 | 7.5 | 21.7 | 15.8 | 30.4 | 21.4 | 40.3 | 37.0 | 45.8 | 49.5 | 56.5 | 84.2 |
<!-- releases:end -->

Measured on an Apple M1 Pro (10 cores, 16 GB) on the fixed scrambles in
`bench/`, with the lookup tables already made. Memory is the most the whole
run held at once, tables included. Other solvers
on the same scrambles: [`bench/README.md`](bench/README.md).

## Install

It is built with Rust. If `cargo --version` answers, skip this; if not, this
is Rust's own installer:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then one line fetches cubenxnxn and builds it:

```bash
git clone https://github.com/Horia73/cubenxnxn.git && cd cubenxnxn && cargo build --release
```

The first solve of a size builds its lookup tables, once, into `tables/`:
a moment for 2x2x2 and 3x3x3, about four minutes and 4.9 GB for 4x4x4, about
8.5 GB for every size together.

## Usage

A cube is given as it looks: its stickers face by face in the order U R F D
L B, each face row by row, each sticker named by the face its colour belongs
to. That is the order Kociemba's and dwalton76's solvers read, and the size
is read from the number of stickers.

```bash
# 2x2x2
./target/release/cubenxnxn solve --state BFRRDRRDUFLDUBDLUBFFULBL

# 3x3x3
./target/release/cubenxnxn solve --state LFUBUFUUFUURLRRULFLRRBFURLLBBFRDDFFLDDBFLUDBDBDBDBLDRR

# 4x4x4
./target/release/cubenxnxn solve --state FUDLLDRRLLBULBDULBUFFDBDRLURRDULFRFBULDRUFDDRLBBBDRUBBRBFRRFFDDBRFFDLUULFFBLRULDUFBDRFFBULURDBLU

# 5x5x5
./target/release/cubenxnxn solve --state RFBBRLFBRLFLUFBFBDBRRDDLBDBRFFUBLRRFBRULDFLFFUDUDFFRBBRFDRLBFDFRUFDBUBBUBULURLLFDLDRLLLDURUBDURDFFFLUBRLULFUUUDRLFLLUFRDLRDBLDUUUBBDULDUFBRRRLBDUDRDDB

# 6x6x6
./target/release/cubenxnxn solve --state FRDFLFRLDRRRDBDUDRURUFLLBBULUFDFLUULURDUBRDBUUULUDDFLLLUDLRRLLFLRLDULFFBRUURRFRFFLUFDUBRBFBFURUFRFDBDBBBDRDFRURBLLFLBBDFLLDUFFFDBLRDBBDUFRDBUDBLRBBBLBDRBRBDRFLBRLURBRBULUBLRDLDRBDUUURFUDFDFLDLBDRFUDBDFLFFFFFRLUUUBLDB

# 7x7x7
./target/release/cubenxnxn solve --state LUFDFDUBLRLFFRBLUUDDFBUFUDUFBLFLRFRLRLDFUDFRDUFUFDBULUFRFLBUFRDFDFRRURDDLRLFBFDFUUULFUUDUFBDLBLLURRFBBLBLLRBBLDLUDFDDFLURRFBLFBRUUBRDBBBLLLLLBLLUDBBDBDBFRULURUFDRFDUBFDRFBDFFUULLRBBBBBDFFRLFRUFDFUDRRRRDDUBUURFBLBLFRBLLRLLFLFLDLBULULDDBBUUURFUDFUBRRRRFBRBLBRDRDRLDDRUDDBBRBBDRBDRUDLDBRDUUFDFRRUL
```

Or as the moves that scrambled it, with the size of the cube:

```bash
# 3x3x3, 25 random moves
./target/release/cubenxnxn solve --size 3 "U2 R' F2 L2 U2 R F L2 B' U' L' B2 R2 D2 R2 B D R D' F2 L U2 F' L2 D'"

# the same cube in the fewest moves there are: 18, after about 25 seconds
./target/release/cubenxnxn solve --size 3 "U2 R' F2 L2 U2 R F L2 B' U' L' B2 R2 D2 R2 B D R D' F2 L U2 F' L2 D'" --mode full

# 5x5x5, 60 random moves, the quick way
./target/release/cubenxnxn solve --size 5 "Bw2 D2 L' F Dw R B U L2 F2 Dw L Dw2 Fw Lw' B' R' Dw Bw' D2 F Lw2 Dw R' B' Rw2 U2 Bw Uw Bw' R2 Dw Fw Dw L' Fw' Lw' Uw2 Bw Dw2 B' Dw2 Bw' L2 Dw' Bw' Lw2 Bw Lw2 U' L2 B2 Lw2 U2 R2 Uw Rw Fw' L2 Uw" --mode fast

# 7x7x7, 100 random moves
./target/release/cubenxnxn solve --size 7 "Rw2 3Uw 3Rw Uw' B2 3Uw2 3Fw2 R Fw' 3Uw' F2 D2 3Fw 3Dw2 Lw2 3Uw 3Bw' Lw2 3Uw' Rw' 3Uw2 3Lw' 3Bw' Uw' F2 Lw2 B' 3Rw Uw Fw' R' B' Lw2 U F' Dw 3Lw2 Uw2 Bw' Rw2 U R' F Uw2 B2 3Lw' Uw' Lw 3Uw' Bw2 U2 3Rw2 3Dw 3Bw' D2 3Bw Uw2 Bw2 Uw' 3Rw2 Bw U' Rw2 Fw2 3Rw Fw L2 3Fw U 3Bw L2 F Uw Rw2 Dw2 R2 3Bw Uw F 3Dw2 Fw2 3Rw Uw 3Bw Dw2 3Fw' 3Lw 3Bw Rw Uw B Rw Uw' F2 Lw' Dw B' 3Lw 3Bw U"

# the first 21x21x21 of the benchmark, 1,050 random moves
./target/release/cubenxnxn solve --size 21 "$(sed -n 1p bench/scrambles212121-5.txt)"

# the stickers of a scrambled cube, to pass to --state
./target/release/cubenxnxn facelets --size 3 "U2 R' F2 L2 U2 R F L2 B' U' L' B2 R2 D2 R2 B D R D' F2 L U2 F' L2 D'"
```

It prints the solution and, on a second line, what it took. The 4x4x4 state
above:

```text
Fw' L D' L U Uw' B2 Rw U' Uw2 R Fw' R2 B2 D2 F' D' Uw2 Fw2 L2 D F D2 Uw2 Rw2 L2 U2 L' F2 L' D F' R' F2 R' D2 R' U2 L R2 F
41 moves (phases [12, 0, 13, 16]), tables 24 ms, search 3646 ms
```

Moves are written as on any cube: `U R F D L B`, `'` for a turn the other
way, `2` for a half turn; `Rw` turns two layers and `3Rw` three.

## How it works

Every solver here turns the cube into a smaller problem in a few phases and
searches each phase with lookup tables. Each phase starts from many results
of the phase before, and the shortest total wins. A large cube is solved as
a sequence of small ones: groups of its layers behave like a 5x5x5, 6x6x6 or
7x7x7, so the same code runs every size.

## More

- [How each solver works](docs/how-it-works.md), phase by phase
- [Commands](docs/commands.md) and [search flags](docs/search-flags.md)
- [Lookup tables](docs/tables.md), file by file
- [Benchmarks](bench/README.md), and [`bench/RESULTS.md`](bench/RESULTS.md):
  the dated log of what was measured, what was tried and what was dropped
- [`FINDINGS.md`](FINDINGS.md): what was learned so far, size by size

## Thanks

All code here is written for this project, but the ideas it stands on are
other people's. Thank you to:

- **[Herbert Kociemba](https://kociemba.org)**, for the
  [two-phase algorithm](https://kociemba.org/cube.htm). It solves the 3x3x3
  here and finishes every larger cube.
- **[Chen Shuang](https://github.com/cs0x7f)**, for
  [TPR](https://github.com/cs0x7f/TPR-4x4x4-Solver) (itself based on Tsai's
  8-step method) and [cube555](https://github.com/cs0x7f/cube555). The 4x4x4
  and 5x5x5 solvers here follow the way his are built.
- **[dwalton76](https://github.com/dwalton76)**, for the
  [NxNxN solver](https://github.com/dwalton76/rubiks-cube-NxNxN-solver) he
  wrote for his LEGO robots that solve cubes. It is the inspiration for this
  project, and the general solver here began from how his works. We have a
  LEGO cube robot of our own, and driving it is the first thing this solver
  is for.

## Licence

MIT, see [LICENSE](LICENSE).
