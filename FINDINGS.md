# Findings of 2026-11-11, all sessions together

Six sessions worked on this repository on 2026-11-11, each on its own
branch. This file puts what they found in one place. The detail, with every
table and every failed attempt, is in `bench/RESULTS.md` of each branch;
the Romanian plan is `PLAN.md`.

Written by the session of the general solver (branch `shorter-solutions`)
from the commits and result files of the other branches as they stood at
15:40. Cross-session messages between the sessions were not delivered (the
receiving sessions hold them), so nothing here was confirmed by the other
sessions; where a branch had not written its results up, that is said.

## Where each size stands

Mean number of moves (outer block turns) on the fixed scramble sets in
`bench/`. "Before" is the state at the start of the day.

| Size | dwalton76 | Before | Best measured now | Branch | Note |
|---|---|---|---|---|---|
| 4x4x4 | 51.71 | 40.60 | 40.60 | `shorter-solutions` | nothing shorter found; see below |
| 5x5x5 fast | 79.78 | 70.16 | 68.84 (200 cubes) | `faster-555` | a third of the processor time |
| 5x5x5 default | 79.78 | 68.82 | 67.90 (first 30; 68.6 before on those) | `faster-555` | whole set not run yet |
| 6x6x6 fast | 136.10 | 112.00 | 111.65 (20 cubes) | `six-faster` | half the processor time |
| 6x6x6 default | 136.10 | 109.20 | 108.59 (first 17; 109.35 before on those) | `six-faster` | run not finished |
| 7x7x7 default | 194.40 | 154.56 | not written up | `faster-777` | staging 52.9 to 50.7 moves on ten cubes |
| 8x8x8 | 308.40 | 301.90 | 243.15 (20 cubes) | `six-faster` on top of `shorter-solutions` | |
| 9x9x9 | 368.00 | 368.55 | 326.90 (20 cubes) | `shorter-solutions` | |
| 10x10x10 | 523.40 | 531.00 | 427.95 (20 cubes) | `shorter-solutions` | |

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

- Phase A of the centres was 62% of the work and its second length was
  worth 1.85 moves. A third length is worth 0.65 more; it is made
  affordable by a table one move deeper in the same memory (reduced by 16
  symmetries) and by searching the longer lengths in part within a budget.
- Edge phases search the starts with least slack first and stop when they
  have what was asked: a third of the positions for the same lengths.
- An exact table for the second phase of the 3x3x3 (57 MB): the same
  solutions in two thirds of the time, for every size.
- Did not work: tables of whole states near the goal for phases P and E.
- For the other sizes: a phase now returns about what it is asked for, not
  up to eight times as much, so solvers that pass their own widths get
  fewer reductions for the same numbers; a cube that arrives with staged
  centres skips the long phase-A lengths.

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
- Did not pay: narrow beams for many candidates, the cube held six ways,
  more solutions per start in phase E, more results of the first step.
- For the larger cubes: on an even cube the innermost ring and the four
  middle layers are a 6x6x6, and its inner steps replace the matching of
  that ring and the 4x4x4 on the innermost wings (8x8x8 246.50 to 243.15).

### 7x7x7 (`faster-777`, from its commits and comments; no results written up)

- Several cubes are carried through the staging and each beam-search step
  is one search from all of them (`search::beam_roots`).
- Which axis is staged first is chosen inside the search (three starts,
  then either of the other two): worth more than a move.
- Steps 1 and 2 of the staging in one search, and steps 4 and 5: 52.9
  moves became 50.7 on ten cubes.
- The same is applied to the staging of any size (`src/staging.rs`).

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

- `together` (in the worktree of the 6x6x6 session) had `shorter-solutions`
  (f8394fa) and `faster-555` merged at 15:36. Still to go in: `faster-777`,
  `six-faster`, `known-reductions`, `public-cleanup`.
- Tried in a scratch worktree: on top of f8394fa the code of `faster-555`
  applies cleanly; `faster-777` then conflicts in `src/stage5.rs`, and
  `six-faster` in `src/edges5.rs` and `src/stage5.rs`. `src/nnn.rs` and
  `src/search.rs` are changed by `six-faster`, `faster-777` and
  `faster-555` (the first two heavily); `src/rings.rs` only by
  `shorter-solutions`.
- New tables since the morning: `tables/ring-*.bin` (general solver, 24 MB
  each, about 1 GB for sizes 8 to 10, made on first use).
- After the branches are together: run every size once more on a quiet
  machine. None of today's times and several of today's lengths (5x5x5
  default, 6x6x6 default, 7x7x7, 8x8x8 with everything) are final.
