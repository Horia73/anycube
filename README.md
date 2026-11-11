# cubenxnxn

A Rubik's cube solver in Rust for cubes of **any size**, built to find
**short solutions in seconds**. It is measured on every size from 2x2x2 to
21x21x21, the largest cube that is mass-produced.

- **2x2x2 and 3x3x3**: optimal solutions (always for 2x2x2, with `--mode full`
  for 3x3x3), or a two-phase solution in a fraction of a second.
- **4x4x4 to 7x7x7**: a solver written for each size, tuned for length.
- **8x8x8 and up**: one general solver that does not know the size. It treats
  groups of layers as smaller cubes and reuses the solvers above.
- Every solution is applied to a full model of the cube and checked before it
  is printed.

The project is young and under active work: each release gets its rows in
the table below, so the progress can be followed.

## Results

Mean moves and mean seconds per cube, for every size from 2x2x2 to 21x21x21.

<!-- releases:begin -->
| Release | | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 21 |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **v0.1.0** | moves | 8.8 | 19.0 | 40.7 | 67.8 | 108.9 | 152.8 | 235.7 | 314.7 | 406.1 | 510.4 | 619.8 | 757.8 | 888.4 | 1031.8 | 1185.6 | 1392.0 | 1548.0 | 1763.8 | 1938.6 | 2254.0 |
| | seconds | 0.00 | 0.22 | 2.8 | 0.58 | 8.4 | 3.8 | 12.9 | 3.7 | 16.6 | 7.5 | 21.7 | 15.8 | 30.4 | 21.4 | 40.3 | 37.0 | 45.8 | 49.5 | 56.5 | 84.2 |
<!-- releases:end -->

Default mode, fixed scrambles, Apple M1 Pro. A turn of a face with any number
of layers counts as one move. The numbers in full, and other solvers on the
same scrambles: [`bench/README.md`](bench/README.md).

## Install

Rust is all it needs ([rustup.rs](https://rustup.rs)):

```bash
cargo build --release
```

The first solve of a size builds its lookup tables, once, into `tables/`:
a moment for 2x2x2 and 3x3x3, about four minutes and 4.9 GB for 4x4x4, about
8.5 GB for every size together.

## Usage

```bash
# 4x4x4 is the default size
./target/release/cubenxnxn solve "Rw U2 F' Uw L2 D Bw' R"

# any other size
./target/release/cubenxnxn solve --size 2 "R U2 F' R2 U F R' U' F2 R"
./target/release/cubenxnxn solve --size 3 "R U R' F2 D L B' U2 R2 D' F L2 U B2 R' D2 F' L U2 B"
./target/release/cubenxnxn solve --size 7 "3Rw U2 F' Uw L2 D 3Bw' R"

# a scramble from a file: the first 21x21x21 of the benchmark
./target/release/cubenxnxn solve --size 21 "$(sed -n 1p bench/scrambles212121-5.txt)"

# a cube as it looks: 6*N*N letters, faces in the order U R F D L B
./target/release/cubenxnxn solve --state <facelets>

# shorter or quicker, on sizes 3 to 7
./target/release/cubenxnxn solve --size 5 "Rw U2 F' Uw L2 D Bw' R" --mode fast
./target/release/cubenxnxn solve --size 3 "R U R' F2 D L B' U2" --mode full
```

It prints the solution and, on a second line, what it took:

```text
R2 F' U R F2 R2 F U2 R'
9 moves, optimal, 7 ms
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

All code here is written for this project. The ideas it stands on are other
people's:

- **Herbert Kociemba**: the [two-phase algorithm](https://kociemba.org/cube.htm),
  used for the 3x3x3 and at the end of every larger cube.
- **Chen Shuang**: [TPR](https://github.com/cs0x7f/TPR-4x4x4-Solver) and
  [cube555](https://github.com/cs0x7f/cube555), where the 4x4x4 and 5x5x5
  solvers started.
- **dwalton76**: the
  [NxNxN solver](https://github.com/dwalton76/rubiks-cube-NxNxN-solver), which
  showed that one program can solve every size.

## Licence

MIT, see [LICENSE](LICENSE).
