# Findings of 2026-11-11, all sessions together

Six sessions worked on this repository on 2026-11-11, each on its own
branch. This file puts what they found in one place. The detail, with every
table and every failed attempt, is in `bench/RESULTS.md` of each branch;
the Romanian plan is `PLAN.md`.

Written by the session of the general solver (branch `shorter-solutions`)
from the commits and result files of the other branches as they stood at
15:40. The sessions of the 5x5x5 and of the 7x7x7 then read their sections
and sent corrections and figures, which are in; the sections of the
6x6x6, the 4x4x4 and the publication are from their branches alone.

## Where each size stands

Mean number of moves (outer block turns) on the fixed scramble sets in
`bench/`. "Before" is the state at the start of the day.

| Size | dwalton76 | Before | Now, the single version | Cubes | Seconds per cube |
|---|---|---|---|---|---|
| 4x4x4 default | 51.71 | 40.60 | 40.67 | first 100 | 2.8 |
| 5x5x5 default | 79.78 | 68.82 | 67.77 | 200 | 0.55 |
| 6x6x6 default | 136.10 | 109.20 | 108.45 | first 20 | 8.6 |
| 7x7x7 default | 194.40 | 154.56 | 152.75 | first 20 | 3.6 |
| 8x8x8 | 308.40 | 301.90 | 235.80 | 20 | 13.0 |
| 9x9x9 | 368.00 | 368.55 | 314.65 | 20 | 4.0 |
| 10x10x10 | 523.40 | 531.00 | 402.45 | 20 | 16.6 |

"Now" is one run of every size, one after another, on the single version
(10b980f: `shorter-solutions` and `together` at the same commit) at 16:17 to
16:39 with nothing else running; every solution applied on the sticker
model, no cube of 8 to 10 the long way. The 4x4x4 is unchanged (40.60 was
all 1,000 cubes; 40.67 is the first 100, where the morning's run gave the
same). Measured apart by the session of the 6x6x6 on the same code: 6x6x6
default 108.35 in 9.4 s and fast 110.65 in 2.4 s (20 cubes). Not measured
on the single version: the fast modes of 5x5x5 and 7x7x7, every full mode.

Every size from 2 to 21 was then measured the same way (release v0.1.0):
the table is in `README.md`, the numbers in `bench/releases.tsv`. Moves per
piece are 0.69 to 0.73 for sizes 4 to 7, 0.80 at 8 and rise slowly to 0.94
at 21; even cubes take about one 6x6x6 solve longer than the odd size next
to them, and from 13 up the odd ones are 1 to 3% longer per piece.

Two runs of the same code do not give the same solutions on the even
cubes: one 10x10x10 cube gave 397, 399 and 403 moves in three runs, and the
mean of the 20 moved from 402.45 to 406.05, while 7x7x7 and 9x9x9 repeated
to the hundredth. So it is in the 6x6x6 steps; the searches that count
their results across threads (`src/solver6.rs`, `src/edges5.rs`) are the
first to look at.

Targets to judge by: the solvers of sizes 4 to 7 make about 0.71 moves per
piece (a cube of size n has 6n² - 12n + 8 pieces), which would be about 210,
274 and 346 moves for sizes 8, 9 and 10; they are at 1.4 to 1.7 times the
number of positions counted in moves, the general solver at 2.1 to 2.4.

## What each session found

### General solver, sizes 8 and up (`shorter-solutions`)

- The centres are not brought home by themselves any more. Each row of
  centres is made to agree in colour with the row under it (`src/rings.rs`)
  and the edge steps, which solve the innermost centres of their small cube
  anyway, bring the rest home. That is the idea of the 6x6x6 and 7x7x7
  solvers, made general.
- A staging step only forms the rows of a ring; the ring goes onto its
  faces once per axis, not once per step (10x10x10: 220 moves of staging
  became 165).
- Rows of any length are matched from their middle outwards with the
  columns done so far kept alike, so that each step is a search of one
  fixed size (10x10x10: 482.75 became 427.95; no cube takes the long way).
- Did not work: staging all orbits at once on a sum of per-orbit
  distances; a row of six centres in one search; moves built like
  commutators (always succeed, 105 to 139 moves for one row).

### 5x5x5 (`faster-555`)

- Phase A of the centres was 62% of the processor time when run without a
  clock (with the clock it was cut off at 15% of the budget, so there the
  loss was in length), and its second length was worth 1.85 moves. A third
  length is worth 0.65 more and a fourth 0.3 (but about a minute per cube
  if searched whole). They are made affordable by a table one move deeper
  in the same memory (reduced by 16 symmetries) and by searching the
  longer lengths in part within a budget.
- Edge phases search the starts with least slack first and stop when they
  have what was asked: about 25 million positions per cube in P and E
  became 8.7 million at the same widths and lengths (68.90 against 68.80
  on 20 cubes).
- After that the widths of P and E no longer matter (`p_keep` 700, 2000,
  6000: 68.25, 68.20, 68.25; `e_keep` 100, 200, 600: 68.30, 68.33,
  68.20). The cheap ones do: more stagings, labellings and results of
  phase C 0.1 to 0.15 each, more 3x3x3 searches 0.3.
- An exact table for the second phase of the 3x3x3 (57 MB): the same
  solutions in two thirds of the time, for every size.
- Did not work: tables of whole states near the goal for phases P and E.
  Their bounds are already exact that close; the work is in the first
  three or four moves of each start, 9 to 11 moves from the goal, and a
  table that helps would have to see that far.
- Copies among the starts of the 5x5x5 phases were counted and left in: 4
  to 12% (990 to 927 in C, 20,324 to 19,492 in P, 10,008 to 8,834 in E on
  one cube), far fewer than on the 6x6x6.
- For the other sizes: a phase now returns about what it is asked for, not
  up to eight times as much, so solvers that pass their own widths get
  fewer reductions for the same numbers; a cube that arrives with staged
  centres skips the long phase-A lengths.
- For the larger cubes, not measured: in their 5x5x5 steps the centres
  arrive staged, phase A has length 0 and phase C has one start (and those
  one move away) where a 5x5x5 of its own chooses among about 1,200
  stagings. The edges of a 5x5x5 of its own take 32.7 to 32.9 moves and
  about 35 there, so at most some 2 moves per orbit of wings are to be
  had, by more results of phase C, more labellings and more axes at the
  start; the widths of P and E will not bring them.

### 6x6x6 (`six-faster`)

- Half of a beam search's time was one read of a large table per state.
  The children of eight states are now made first and their table entries
  asked for ahead (`Problem::prefetch`): the last staging step takes a
  third of the processor time, the inner reduction two thirds.
- A beam that fails ran all its levels four times over; the reduction now
  gives up after four levels on one score (`Problem::patience`). The rule
  must stay the reduction's own: applied to every beam it sent 3 of 20
  8x8x8 cubes the long way.
- A third to a half of the candidates carried from step to step were the
  same cube reached by other moves. Only different cubes are carried now.
- Did not pay: narrow beams for many candidates, more solutions per start
  in phase E, more results of the first step. The cube held six ways gave
  nothing at equal effort; it stays as an option (`--holds`).
- On the single version the edge phases of the 5x5x5 return about as many
  results as they are asked for, so the 6x6x6 asks for three times as many
  (`p_keep` 12000, `e_keep` 2400): 108.35 where the old numbers gave
  108.85. The full mode is scaled the same way and not measured.
- For the larger cubes: on an even cube the innermost ring and the four
  middle layers are a 6x6x6, and its inner steps replace the matching of
  that ring and the 4x4x4 on the innermost wings (8x8x8 246.50 to 243.15).
  The staging of the 7x7x7 may leave a pair of colours on the faces of
  another axis, and these steps take each pair on its own axis: the frame is
  turned first (`Work::reaxis`). Without it 5 of the first 8 8x8x8 cubes
  went the long way.

### 7x7x7 (`faster-777`)

Processor time throughout (the machine was at a load of 30 to 290); the
detail is the last section of `bench/RESULTS.md` on that branch.

- Whole solves, first 20 cubes: 153.15 moves (staging 50.30, matching
  16.40, inner edges 34.45, outer edges 34.90, 3x3x3 17.10) at about 25 s
  of processor time per cube, against 154.3 to 154.8 at about 53 s. Two
  earlier runs of the new code gave 153.45 and 153.05.
- Staging alone, ten cubes, beams of 100,000: the five steps 52.90; steps
  1 and 2 in one search with the three axes as starts 51.50; steps 4 and 5
  in one as well 50.70.
- Of the 2.9 moves gained in the staging about 1.3 are left in the total:
  the stagings handed to the matching now resemble each other, and the
  matching takes 16.9 to 17.1 where it took 15.55 from eight stagings
  that differed early.
- Faster with the same solutions (staging of five cubes): 149 s became
  84 s by asking for table entries ahead. One beam for all candidates:
  4.7 s per cube against 17.6 s with one each, but 53.2 moves against
  52.3; at 300,000 states for all, 52.5 in 8.4 s.
- Did not work: the order of the axes chosen by the length of one step
  (53.1 against 52.9); steps 1 to 3 in one search (no shorter, 1.7 times
  the time); other weights in the merged searches (52.8 to 57.4); 32
  candidates carried instead of 8; 256 stagings given to the matching.
- For the larger cubes (`src/staging.rs`, staging alone): 8x8x8 92.0
  moves became 81.2, 9x9x9 128.0 became 112.6, 10x10x10 168.3 became
  155.0 (three to five cubes each; no whole solves on the final code).
- The parity of the wings left free in every staging step shortens the
  staging by 1 to 4 moves and lengthens the edges by 11 on a 10x10x10; it
  is now seen to once, by the step of the outermost ring.
- Its matching scores each state by the nearer of "same" and "opposite"
  per axis, the case that kept the beams of the general solver undecided;
  the eight choices as eight starts of one beam is the next thing to try.

### 4x4x4 (`known-reductions`)

- A test bench of cubes with a reduction of known length (`known --walk
  K`): at 20 random moves from a reduced cube the solver finds 22.73
  against 23.10 on random cubes, so its reductions are about 2.7 moves
  longer than one that exists.
- The last phase loses nothing: the shortest reduction with all moves has
  the same length from the states it starts from. What is lost is lost in
  the first ten moves, in having to pass through staged centres.
- A free search (beam with all moves) finds the known reduction only up to
  14 moves away; the hand-made signals fade beyond that. A small network
  as the signal did not work at that scale either.
- Solving the inverse cube does not work on the 4x4x4 (0 of 60 solutions
  solve the real cube: centres of one colour are not told apart). The
  figures 40.37, 39.67 and 39.85 recorded for it were not real results.

### Publication (`public-cleanup`)

- MIT licence file, `.DS_Store` and `.claude` ignored, crate metadata, and
  a README that starts with what the project is and one real solve.

## What holds across sizes

- **Times measured today are not comparable.** Four to six benchmarks ran
  at once (load 30 to 300). Compare lengths without a clock (`--time 0`),
  processor time (`src/clock.rs` on `six-faster`) and positions visited.
- **A beam needs a score that sees what hangs together.** Exact tables over
  all the orbits that share moves work (four orbits, 70⁴ states per axis);
  sums of distances of single orbits or pairs stall. Found on the 6x6x6,
  the 7x7x7 and three times on the general solver.
- **Choose the target before the search.** A score that takes the nearer
  of two targets at each state keeps a beam undecided.
- **Candidates must be different cubes**, and a search from all candidates
  at once puts the work where it pays. (One exception measured: the inner
  reduction of the 6x6x6 came out longer from one shared beam of 100,000
  than from one beam per candidate.)
- **What is chosen late is chosen well:** every size gains from more
  different reductions reaching the 3x3x3 and from choosing by total.
- **Larger cubes need no work per size.** Everything from 8 up goes through
  the same small cubes made of layers; what improves a step improves every
  size. What they need is steps that handle a whole row or ring at once.

## State of the code

- There is one version: `shorter-solutions` (the main folder) and
  `together` are the same commit and hold every branch of the day. The
  branches of the sessions (`faster-555`, `six-faster`, `faster-777`,
  `known-reductions`, `public-cleanup`) are all inside it and are history.
- One session measures at a time, one run after another; the table at the
  top is that run.
- After it: `row` solves a scramble file of any size from 2 up in one mode
  and prints one line; `scripts/release.py` measures sizes 2 to 21 with it
  and writes the tables of releases in `README.md` from
  `bench/releases.tsv`.
- The tables of the row matching (`tables/ring-*.bin`, 24 MB each) are
  named by what they are made of, so rows, sizes and axes whose orbits are
  moved alike share one file. Before, every size made its own: 86 files for
  sizes 8 to 12, more than the disk holds up to 21.
- On even cubes the staging of `src/staging.rs` may leave the colours of
  an axis on the faces of another axis (every centre is still staged). The
  matching in `src/rings.rs` needs only four of each colour per axis and
  orbit, and the steps after it name the frame from the innermost centres,
  so this is allowed for.
- Open: the solutions are not the same from run to run (above); matching
  time on the large cubes; staging and edges are the long parts (254 and
  131 of 402 moves at 10x10x10); the fast and full modes on the single
  version.
