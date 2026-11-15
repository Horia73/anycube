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

### 5x5x5, a search for a shorter structure (`claude/five-shorter-dc1676`, night)

Detail and numbers: the last section of `bench/RESULTS.md`.

- Counted like the 4x4x4 in `PLAN.md`, every phase but the centre staging
  is at what its own moves allow (the sum is about 65, measured 67.75). A
  mean of 60 would take a search with all 36 moves that is close to the
  shortest through the first forty moves.
- `known5` makes cubes a known number of moves from a reduced cube. The
  solver does not see that such a cube is close (18 moves away it finds a
  reduction of 43), but that does not carry over to random cubes: a beam
  search with all the pairing moves finds the short way on those cubes and
  nothing shorter than phases P and E on random ones.
- Did not work: the last edge phase with 20 moves instead of 14 (the same
  length from its states); choosing reductions by the first-phase distance
  of their 3x3x3 (0.2 moves per unit); the centres in one exact search
  bounded by mixed tables of both orbits (they are weaker than the orbit
  tables); beam searches for the pairing and for the centres; phase B and
  phase C as one search (they share no moves: same length, a thousand
  times the work); the reduction ending in a cube that the second phase
  of the 3x3x3 solves alone (the end 6.5 moves shorter, the way to it
  10 longer, four cubes).
- More effort in the present phases buys tenths: full mode is 0.6 shorter
  (12 cubes) for three to seven times the positions searched.

### 5x5x5, what more time buys (`five-tail`, 2026-11-14)

Detail: the last section of `bench/RESULTS.md`.

- Counted without the sequences that are one another rewritten, the
  bounds are 35, 52, 75 and 99 moves for sizes 4 to 7 (not 29.5 and 47.8),
  and a random cube is about that far. The solvers are at 1.16, 1.29,
  1.44 and 1.54 times the bound; the best learned search published is at
  1.26 and 1.68 for sizes 4 and 5 in its own metric.
- More time buys 0.28 moves for every doubling, whichever widths it is
  spent on, with no knee from 0.7 s to 3.9 s. The default keeps three times
  as many results of phase C (a tenth of a move for 30 ms) and nothing
  that costs more.
- The larger cubes gain nothing from wider phases C: their 5x5x5 steps
  get one staging to choose from.
- The centres staged in one step would not pay even if it were fast:
  the shortest stagings are too few to choose from.
- The first 25 moves (to the end of phase C) are 2 to 3 moves longer than
  what exists (`known5 --base split`), the largest loss measured and the
  only one in a stretch with all moves.

### 5x5x5, the centres by a learned estimate (2026-11-15, an experiment)

Detail: the last section of `bench/RESULTS.md`.

- **The shortest staging of the centres is 16 moves on most cubes, not 14
  or 15.** An exact search stopped at 15 moves (`centres5 --max 15`, two
  to seven minutes a cube) finds a staging on one of the first eight
  cubes and proves the other seven have none. The default mode is 0.6 of
  a move from the optimum there (16.63 against 15.88 to 16.13) and has
  the optimum on four of the eight. The count that gave 14 takes every
  move sequence for a different cube; near the goal most are not.
- The same count gives the 19.3 moves to the end of phase C from which a
  loss of 2 to 3 moves was read. Checked in the subsection after this
  one: there the count is 0.8 short, and the loss is 3 moves.
- A beam search with all 36 moves on a small network's estimate of the
  moves left stages the centres in 18.05, 17.48, 16.93, 16.65 and 16.33
  moves at widths from 10,000 to a million, each three times the one
  before (two phases: 16.75 in 0.24 s). A million takes 118 s a cube and
  is within a quarter of a move of the optimum where that is known; the
  two phases with a hundred times their positions give 16.28 in 22 s. In
  the solver the stagings of the widest beam are worth 0.68 moves; the
  full mode gets as much in 4 s. Not kept in any mode.
- Too small, or the wrong idea: neither. The network was not too small
  (0.3 to 14.4 million weights: the same error for each K, 3 points more
  cubes kept, no shorter beam at the same width, a twenty-seventh of the
  speed), nor the data (a quarter of it gives the same). The width
  mattered up to a million (0.3 moves for every three times) and three
  million gave the same lengths on the three cubes run: the cube ends
  the curve, not the machine.
- The length of a random walk is a poor label here: 24 of the 36 moves
  leave staged centres staged, half the cubes K moves away are nearer than
  K, and one in ten is within 11 moves after a walk of 16.
- Exact distances up to 11 moves are cheap to label with (a search bounded
  by the tables, milliseconds a cube) and make the beam 0.75 shorter at a
  width of 100,000. Against a random cube the estimate is sure up to 9
  moves, keeps a cube at 10 or 11 seven to nine times in ten and one at
  12 half the time: from 12 on the search is blind.

### 5x5x5, the distance to the end of phase C counted level by level (2026-11-15, an experiment)

Detail: the last section of `bench/RESULTS.md`.

- **By the levels a random cube is about 20 moves from the end of
  phase C; the solver finds 23.2.** The 3 moves read from the count are there, 0.6 of them on
  the centres. `levels5` counts the cubes at each number of moves from a
  goal, whole levels first and a few more from random descents.
- Why the count was two moves out for the centres and is 0.8 out here:
  the levels of the centres grow 23.2 times a move at every length
  measured (to 10 moves) where the sequences grow 28.12, so one sequence
  in six keeps ending on a cube already reached. With the wings and
  middle edges to tell cubes apart the levels grow 27.2 times at seven
  moves and rising: what is lost is lost in the first moves.
- The levels put the centres at 15.6 moves where the exact searches found
  15.9 to 16.1, and the end of phase C at 19.8: at least that, 20.1 to
  20.3 if as far short as on the centres. No exact search reaches 20 moves.
- The 2,048 labellings of a cube's wings are separate chances of a short
  phase C (`labellings5`, from the exact table): worth 1,900 or more where
  few cubes have a labelling that near, 1,670 where one in five has.
- More stagings do not get there: ten times as many shorten the way to
  the end of phase C by 0.4 and a hundred times by 0.7 (23.2, 22.8,
  22.5; the solution 67.35, 66.98, 66.33, the last at 38 s a cube). The loss is in staging the
  centres first and moving the wings after with the moves that keep them
  staged (16.75 and 6.45), where by the levels a way that does both at
  once is about 20.
- On cubes a known walk from the end of phase C (60 of each) the solver
  finds the way that exists two times in three up to 11 moves, is 2.3
  moves longer at 14 and 3 at 17.
- A search with all 36 moves to the end of phase C is where those 3
  moves are. Tried the same day (next): it does not reach them.

### 5x5x5, a search with all 36 moves to the end of phase C (2026-11-15, an experiment)

Detail: the last section of `bench/RESULTS.md`.

- **It fails.** A beam search from the cube straight to the end of
  phase C (`split5-beam`, exact over the last three moves) finds the way
  on cubes that are up to 12 to 16 moves from it and not on random
  cubes, which are at 20: one cube in six at a beam of a million with
  the first estimate (25 moves, 438 s), 8 in 20 at 28 to 30 moves with
  the best network. The solver finds 23.2 on all in under a second.
- What a beam needs is an estimate that tells the cubes one move nearer
  from the rest, at every length. Read off cubes a known walk from the
  goal (`--see`): a table of four numbers does so for 43% of the cubes
  at 14 moves and 17% at 20; a network that reads the whole cube for
  85% and 45%; the same on forty times the cubes (480 million, each
  seen once) for 92% and 52%. In the search that is two moves of reach
  each time. More cubes help, a wider network hardly, and a wider one
  on the same cubes learns them by heart.
- Do not label the cubes by anything but the length of the walk: it is a
  fair label as far as 19 moves (a cube 17 moves away is far more often
  the end of a walk of 17 than of 22). What flattened past 12 moves was
  the table's four numbers.
- Did not work: starting from every labelling of the wings (the beam
  fills with wrong ones; keep cubes, choose the labelling at the end);
  whole levels to four moves at the end instead of three (the same
  cubes, ten times the time); training the network on what its own
  searches found, one round (the network's error is not that it takes
  far cubes for near ones, but that it takes the cubes on the short way
  for 1.3 moves farther than they are); the network for the first three
  or six moves and the solver for the rest (`split5-hybrid`: 67.95 and
  69.65 against 67.35; of six moves chosen 3.7 are progress, of six
  random ones 2.0).
- The 4x4x4 gave the same picture on 2026-11-11 (a network on 192
  million cubes: reductions of 27 to 30 against 23). By these figures a
  larger machine would move the reach by a move or two where four are
  missing; not tried.

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
