//! 7x7x7, the whole solve.
//!
//! Layers 1, 3, 4, 5 and 7 of the cube form a 5x5x5, the *inner cube*
//! (corners, inner wings, middle edges, inner x and inner t centres); its
//! wide turn is our 3-layer turn, and our 2-layer turn is a face turn to it.
//! Layers 1, 2, 4, 6 and 7 form another, the *outer cube*, once every triple
//! (oblique, outer t centre, oblique) shows one colour, every inner wing sits
//! next to its middle edge and the inner centres are solved: its t centres
//! are the triples, its middle edges the edges of the reduced inner cube,
//! and its moves are our 1- and 2-layer turns.
//!
//! 1. The centres are staged (`stage7`).
//! 2. *Matching.* Turns of one and three layers move the eight centres
//!    around a corner of the inner 3x3 block of a face as two rigid blocks:
//!    an inner x centre with the outer x centre and the two obliques next to
//!    it, and an inner t centre with the outer t centre next to it. Half
//!    turns of two layers (and face turns to set them up) make every oblique
//!    and outer t centre agree with the inner centre of its block: the same
//!    colour everywhere on an axis, or the opposite colour everywhere.
//! 3. The edge phases of the 5x5x5 solver reduce the inner cube with turns
//!    of one and three layers. They solve its centres on the way, and the
//!    blocks come along: afterwards every triple shows one colour.
//! 4. The 5x5x5 solver finishes the outer cube: its edge phases (the centres
//!    are staged already) and the 3x3x3.
//!
//! Every step hands several results to the next one, which continues the
//! ones that are shortest in total.

use crate::c3::C3;
use crate::c5::{tables5, Cube5};
use crate::edges5::rider::{inner_move, Beam, Rider};
use crate::edges5::{is_reduced, Found, Limits, Start};
use crate::solver5::{Params5, Solver5};
use crate::search::{beam, Problem};
use crate::stage7::{from_inner, moves_where, Stage7, INNER5, OBL_A, OBL_B, OUTER5, T_IN, T_OUT, X_IN, X_OUT};
use rayon::prelude::*;
use std::time::{Duration, Instant};

const FAR: u8 = 255;

/// The orbits the matching step looks at, in the order of its state.
const MATCHED: [usize; 5] = [X_IN, T_IN, OBL_A, OBL_B, T_OUT];

/// Step 2: tables and moves.
pub struct Matching {
    /// Face turns and half turns of two and three layers: what keeps the centres staged.
    moves: Vec<u8>,
    rank8: [u8; 256],
    /// Per axis (0 U/D, 1 F/B, 2 R/L) and per choice of colours (the same as
    /// the inner centre, or the opposite): a lower bound, in half moves, on
    /// matching the outer t centres of that axis, by (inner t pattern, outer
    /// t pattern); and on matching its obliques, by (inner x pattern, first
    /// oblique pattern, second oblique pattern). A move's cost is shared out
    /// so that the three axes may be added: a face turn counts for its own
    /// axis, a wide half turn half for each of the other two.
    t: [[Vec<u8>; 2]; 3],
    x: [[Vec<u8>; 2]; 3],
}

/// Cheapest way to a goal from every state. Move `j` takes state `s` to
/// `next(s, j)` at cost `cost[j]`; the moves are closed under inversion.
fn cheapest(n: usize, goal: impl Fn(usize) -> bool, cost: &[u8], next: impl Fn(usize, usize) -> usize) -> Vec<u8> {
    let mut dist = vec![FAR; n];
    let mut level: Vec<usize> = (0..n).filter(|&s| goal(s)).collect();
    for &s in &level {
        dist[s] = 0;
    }
    // Levels in order of cost; a level grows while moves of cost 0 add to it.
    let mut later: Vec<Vec<usize>> = vec![Vec::new(), Vec::new()];
    let mut d = 0u8;
    loop {
        let mut i = 0;
        while i < level.len() {
            let s = level[i];
            i += 1;
            if dist[s] != d {
                continue;
            }
            for (j, &c) in cost.iter().enumerate() {
                let t = next(s, j);
                if dist[t] > d + c {
                    dist[t] = d + c;
                    match c {
                        0 => level.push(t),
                        _ => later[c as usize - 1].push(t),
                    }
                }
            }
        }
        if later.iter().all(Vec::is_empty) {
            break;
        }
        level = std::mem::take(&mut later[0]);
        later.swap(0, 1);
        d += 1;
    }
    dist
}

impl Matching {
    pub fn new(s7: &Stage7) -> Matching {
        let moves = moves_where(|_, layers, power| layers == 1 || power == 1);
        let mut rank8 = [255u8; 256];
        let unrank8: Vec<u32> = (0..256u32).filter(|x| x.count_ones() == 4).collect();
        for (i, &x) in unrank8.iter().enumerate() {
            rank8[x as usize] = i as u8;
        }
        // The cells of a block: the inner centre at position `i` of its orbit
        // and the outer centres beside it. `outward` moves a row or column
        // index of an inner centre one step away from the middle.
        let n = 7;
        let cell = |o: usize, i: usize| {
            let f = s7.orbits[o].facelet[i];
            (f / (n * n), f / n % n, f % n)
        };
        let find = |o: usize, face: usize, r: usize, c: usize| s7.orbits[o].facelet.iter().position(|&f| f == face * n * n + r * n + c);
        let outward = |k: usize| match k {
            2 => 1,
            4 => 5,
            k => k,
        };
        let (mut first, mut second) = ([0usize; 24], [0usize; 24]);
        for i in 0..24 {
            let (face, r, c) = cell(T_IN, i);
            assert_eq!(find(T_OUT, face, outward(r), outward(c)), Some(i), "t centres of a block have the same number");
            let (face, r, c) = cell(X_IN, i);
            assert_eq!(find(X_OUT, face, outward(r), outward(c)), Some(i), "x centres of a block have the same number");
            // One of the two obliques of the block is of each orbit.
            let beside = [(outward(r), c), (r, outward(c))];
            let of = |o: usize| beside.iter().find_map(|&(r, c)| find(o, face, r, c)).expect("an oblique of each orbit in a block");
            first[i] = of(OBL_A);
            second[i] = of(OBL_B);
            assert!(first[i] / 8 == i / 8 && second[i] / 8 == i / 8);
        }
        let build = |axis: usize| -> ([Vec<u8>; 2], [Vec<u8>; 2]) {
            let shift = 8 * axis as u32;
            // Pattern of each orbit after each move.
            let mv: Vec<Vec<u8>> = MATCHED
                .iter()
                .map(|&o| {
                    (0..70)
                        .flat_map(|r| {
                            let (perm, unrank8, moves) = (&s7.perms[o], &unrank8, &moves);
                            moves.iter().map(move |&m| {
                                let y = perm.apply(m, unrank8[r] << shift) >> shift;
                                assert!(y < 256, "these moves keep the centres staged");
                                rank8[y as usize]
                            })
                        })
                        .collect()
                })
                .collect();
            let w = moves.len();
            let step = |k: usize, r: usize, j: usize| mv[k][r * w + j] as usize;
            // Positions are numbered U D F B R L, eight to an axis.
            let own = |m: u8| [0, 2, 1][m as usize / 3 % 3] == axis;
            let cost: Vec<u8> = moves
                .iter()
                .map(|&m| match (m >= 18, own(m)) {
                    (false, true) => 2,
                    (true, false) => 1,
                    _ => 0,
                })
                .collect();
            let t_table = |swap: u32| {
                cheapest(
                    4900,
                    |s| unrank8[s / 70] == unrank8[s % 70] ^ (swap * 255),
                    &cost,
                    |s, j| step(1, s / 70, j) * 70 + step(4, s % 70, j),
                )
            };
            let x_table = |swap: u32| {
                cheapest(
                    343_000,
                    |s| {
                        let (x, a, b) = (unrank8[s / 4900], unrank8[s / 70 % 70], unrank8[s % 70]);
                        (0..8).all(|i| {
                            let want = (x >> i & 1) ^ swap;
                            (a >> (first[8 * axis + i] % 8)) & 1 == want && (b >> (second[8 * axis + i] % 8)) & 1 == want
                        })
                    },
                    &cost,
                    |s, j| (step(0, s / 4900, j) * 70 + step(2, s / 70 % 70, j)) * 70 + step(3, s % 70, j),
                )
            };
            ([t_table(0), t_table(1)], [x_table(0), x_table(1)])
        };
        let (t0, x0) = build(0);
        let (t1, x1) = build(1);
        let (t2, x2) = build(2);
        for tab in [&t0, &t1, &t2].iter().flat_map(|t| t.iter()).chain([&x0, &x1, &x2].iter().flat_map(|t| t.iter())) {
            assert!(tab.iter().all(|&d| d != FAR), "every staged pattern can be matched");
        }
        Matching { moves, rank8, t: [t0, t1, t2], x: [x0, x1, x2] }
    }
}

/// The matching step as a search problem. A state is where each of the five
/// orbits shows the first colour of its axis (U, R or F).
struct Match<'a> {
    s7: &'a Stage7,
    m: &'a Matching,
}

impl Match<'_> {
    /// Half moves still needed on one axis.
    #[inline(always)]
    fn axis(&self, n: &[u32; 5], axis: usize) -> u8 {
        let pat = |k: usize| self.m.rank8[((n[k] >> (8 * axis)) & 255) as usize] as usize;
        let (t, x) = (pat(1) * 70 + pat(4), (pat(0) * 70 + pat(2)) * 70 + pat(3));
        let (tt, xt) = (&self.m.t[axis], &self.m.x[axis]);
        tt[0][t].max(xt[0][x]).min(tt[1][t].max(xt[1][x]))
    }
}

impl Problem for Match<'_> {
    type Node = [u32; 5];

    fn moves(&self) -> &[u8] {
        &self.m.moves
    }

    #[inline(always)]
    fn next(&self, n: &[u32; 5], k: usize) -> [u32; 5] {
        let m = self.m.moves[k];
        let mut out = [0u32; 5];
        for i in 0..5 {
            out[i] = self.s7.perms[MATCHED[i]].apply(m, n[i]);
        }
        out
    }

    #[inline(always)]
    fn bound(&self, n: &[u32; 5]) -> usize {
        (self.axis(n, 0) as usize + self.axis(n, 1) as usize + self.axis(n, 2) as usize).div_ceil(2)
    }

    /// Every axis at 0 means every block matches.
    fn done(&self, _: &[u32; 5]) -> bool {
        true
    }

    /// Only a 2-layer turn changes what lies beside an inner centre.
    fn may_end(&self, k: usize) -> bool {
        (18..36).contains(&self.m.moves[k])
    }
}

/// The triples of one axis while the centres are staged: where the first
/// obliques, the outer t centres and the second obliques show the first
/// colour of the axis, each one of 70 patterns.
pub struct Triples {
    /// `step[axis][orbit][pattern * 54 + move]`: the pattern after a move that keeps the centres staged.
    step: [[Vec<u8>; 3]; 3],
    /// Per axis, by the three patterns: moves until every triple of the axis
    /// shows one colour and the two colours lie in *stripes*, one on the
    /// triples along the rows of both faces and the other on the triples
    /// along the columns. Stripes stay stripes under every half turn.
    stripes: [Vec<u8>; 3],
    /// The same until every triple of the axis shows one colour.
    uniform: [Vec<u8>; 3],
}

impl Triples {
    pub fn new(s7: &Stage7) -> Triples {
        let moves = moves_where(|_, layers, power| layers == 1 || power == 1);
        let mut rank8 = [255u8; 256];
        let unrank8: Vec<u32> = (0..256u32).filter(|x| x.count_ones() == 4).collect();
        for (i, &x) in unrank8.iter().enumerate() {
            rank8[x as usize] = i as u8;
        }
        let build = |axis: usize| -> ([Vec<u8>; 3], Vec<u8>, Vec<u8>) {
            let shift = 8 * axis as u32;
            let step = [OBL_A, T_OUT, OBL_B].map(|o| {
                let mut t = vec![255u8; 70 * 54];
                for r in 0..70 {
                    for &m in &moves {
                        let y = s7.perms[o].apply(m, unrank8[r] << shift) >> shift;
                        assert!(y < 256, "these moves keep the centres staged");
                        t[r * 54 + m as usize] = rank8[y as usize];
                    }
                }
                t
            });
            let next = |s: usize, j: usize| {
                let m = moves[j] as usize;
                (step[0][s / 4900 * 54 + m] as usize * 70 + step[1][s / 70 % 70 * 54 + m] as usize) * 70 + step[2][s % 70 * 54 + m] as usize
            };
            // The triples of this axis that lie along a row of their face.
            let rows = (0..8usize)
                .filter(|&i| matches!(s7.orbits[T_OUT].facelet[8 * axis + i] / 7 % 7, 1 | 5))
                .fold(0u32, |x, i| x | 1 << i);
            assert_eq!(rows.count_ones(), 4);
            let alike = |s: usize| s / 4900 == s % 70 && s / 70 % 70 == s % 70;
            let striped = |s: usize| alike(s) && (unrank8[s % 70] == rows || unrank8[s % 70] == rows ^ 255);
            // Positions are numbered U D F B R L, eight to an axis.
            let own = |m: u8| [0, 2, 1][m as usize / 3 % 3] == axis;
            for s in (0..343_000).filter(|&s| striped(s)) {
                for (j, &m) in moves.iter().enumerate() {
                    assert!(striped(next(s, j)) || (own(m) && m < 18 && m % 3 != 1), "stripes survive every half turn");
                }
            }
            let cost = vec![1u8; moves.len()];
            let stripes = cheapest(343_000, striped, &cost, next);
            let uniform = cheapest(343_000, alike, &cost, next);
            assert!(stripes.iter().all(|&d| d != FAR), "stripes can be reached from every staged pattern");
            (step, stripes, uniform)
        };
        let (s0, a0, u0) = build(0);
        let (s1, a1, u1) = build(1);
        let (s2, a2, u2) = build(2);
        if std::env::var_os("CUBE7_TRACE").is_some() {
            let mean = |t: &[u8]| t.iter().map(|&d| d as f64).sum::<f64>() / t.len() as f64;
            eprintln!(
                "  triples: stripes mean {:.2} max {}, one colour mean {:.2} max {}",
                mean(&a0),
                a0.iter().max().unwrap(),
                mean(&u0),
                u0.iter().max().unwrap()
            );
        }
        Triples { step: [s0, s1, s2], stripes: [a0, a1, a2], uniform: [u0, u1, u2] }
    }

    /// The three patterns of an axis on a staged cube.
    fn patterns(&self, s7: &Stage7, f: &[u8], axis: usize) -> [u8; 3] {
        [OBL_A, T_OUT, OBL_B].map(|o| s7.ranks.rank8[((s7.primary(o, f) >> (8 * axis)) & 255) as usize])
    }
}

/// The triples as a rider of an edge phase of the inner cube. Each phase
/// looks after the axis whose faces it turns by quarters, in the order that
/// leaves stripes alone afterwards: phase C (0) stripes the axis that is R/L
/// in its frame, phase P (1) the one that is F/B, and phase E (2) brings the
/// triples of the last axis to one colour each.
struct TripleRider<'a> {
    s: &'a Solver7,
    phase: usize,
}

impl Rider for TripleRider<'_> {
    type State = [u8; 3];

    #[inline(always)]
    fn step(&self, ctx: usize, s: &[u8; 3], m: u8) -> [u8; 3] {
        let frame = &self.s.frames[ctx];
        let step = &self.s.triples.step[frame.axes[self.phase]];
        let ours = frame.to7[m as usize] as usize;
        [0, 1, 2].map(|k| step[k][s[k] as usize * 54 + ours])
    }

    #[inline(always)]
    fn bound(&self, ctx: usize, s: &[u8; 3]) -> usize {
        let axis = self.s.frames[ctx].axes[self.phase];
        let i = (s[0] as usize * 70 + s[1] as usize) * 70 + s[2] as usize;
        let t = &self.s.triples;
        (if self.phase == 2 { &t.uniform[axis] } else { &t.stripes[axis] })[i] as usize
    }
}

/// The cube seen from another side (a 5x5x5).
struct Frame {
    turns: Vec<usize>,
    /// Move of the original cube for each move of this frame, and back.
    to_orig: Vec<u8>,
    from_orig: Vec<u8>,
    /// The axes of the cube as given (0 U/D, 1 F/B, 2 R/L) that are R/L,
    /// F/B and U/D in this frame.
    axes: [usize; 3],
    /// Move of the 7x7x7 as given for each of its moves made in this frame.
    to7: Vec<u8>,
}

/// A 5x5x5 part of the way through its edge phases: the candidate it
/// started from, its moves so far in the moves of the cube as given, the
/// frame it is searched in, and the length after each finished phase.
struct Partial {
    cand: usize,
    moves: Vec<u8>,
    /// For the inner cube: the same moves as moves of the 7x7x7.
    ours: Vec<u8>,
    frame: usize,
    ends: Vec<usize>,
}

/// The edge phases of the 5x5x5 solver run over many 5x5x5 cubes at once,
/// each with the moves already spent on it, so that every phase keeps what
/// is shortest in total.
struct Chain<'a> {
    s: &'a Solver7,
    /// Each candidate in each frame.
    roots: Vec<Vec<Cube5>>,
    spent: Vec<usize>,
    t0: Instant,
}

impl<'a> Chain<'a> {
    /// `cands`: 150 facelets with staged centres, and the moves spent so far.
    fn new(s: &'a Solver7, cands: &[(Vec<u8>, usize)]) -> Result<Chain<'a>, String> {
        let g = &tables5().geometry;
        let roots = cands
            .iter()
            .map(|(f, _)| s.frames.iter().map(|fr| Cube5::from_facelets(&g.reframe(f, &fr.turns))).collect::<Result<Vec<_>, _>>())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Chain { s, roots, spent: cands.iter().map(|c| c.1).collect(), t0: Instant::now() })
    }

    fn start_in(&self, x: &Partial) -> Start {
        let f = &self.s.frames[x.frame];
        let mut cube = self.roots[x.cand][x.frame];
        let mut last = 255;
        for &m in &x.moves {
            last = f.from_orig[m as usize];
            cube.apply(last);
        }
        Start { cube, cost: self.spent[x.cand] + x.moves.len(), last }
    }

    fn starts_of(&self, from: &[Partial]) -> Vec<Start> {
        from.par_iter().map(|x| self.start_in(x)).collect()
    }

    /// Continues each start's partial solution with what a phase found.
    fn extend(&self, from: &[Partial], found: Vec<Found>) -> Vec<Partial> {
        found
            .into_iter()
            .map(|f| {
                let base = &from[f.start];
                let to_orig = &self.s.frames[base.frame].to_orig;
                let moves: Vec<u8> = base.moves.iter().copied().chain(f.moves.iter().map(|&m| to_orig[m as usize])).collect();
                let mut ends = base.ends.clone();
                ends.push(moves.len());
                Partial { cand: base.cand, moves, ours: Vec::new(), frame: base.frame, ends }
            })
            .collect()
    }

    /// The same for a phase with the triples as rider, whose moves are
    /// moves of the 7x7x7 in the frame of the start.
    fn extend_ridden(&self, from: &[Partial], found: Vec<Found>) -> Vec<Partial> {
        found
            .into_iter()
            .map(|f| {
                let base = &from[f.start];
                let to7 = &self.s.frames[base.frame].to7;
                let (mut moves, mut ours) = (base.moves.clone(), base.ours.clone());
                for &m in &f.moves {
                    ours.push(to7[m as usize]);
                    moves.push(inner_move(to7[m as usize]));
                }
                let mut ends = base.ends.clone();
                ends.push(moves.len());
                Partial { cand: base.cand, moves, ours, frame: base.frame, ends }
            })
            .collect()
    }

    /// The three edge phases of the inner cube with the triples as rider.
    /// `cubes` are the 7x7x7 cubes the candidates were cut from. The results
    /// are reduced inner cubes on which every triple shows one colour.
    fn edges_ridden(&self, cubes: &[Vec<u8>], p: &Params7) -> Vec<Partial> {
        let (s, e5) = (self.s, &self.s.s5.edges);
        let beam = |cap: usize| Beam { width: p.inner_beam, cap, more: p.more, mix: p.mix };
        // Frame and triple patterns of each start, for the axis its phase looks after.
        let ride = |from: &[Partial], phase: usize| -> Vec<(usize, [u8; 3])> {
            from.par_iter()
                .map(|x| {
                    let mut f = cubes[x.cand].clone();
                    s.s7.g.apply_seq(&mut f, &x.ours.iter().map(|&m| m as usize).collect::<Vec<_>>());
                    (x.frame, s.triples.patterns(&s.s7, &f, s.frames[x.frame].axes[phase]))
                })
                .collect()
        };
        let trace = std::env::var_os("CUBE7_TRACE").is_some();
        let lap = |what: &str, found: &[Partial]| {
            if trace {
                let best = found.iter().map(|x| self.total(x)).min().unwrap_or(0);
                eprintln!("    {} {:6} results, shortest {:3}, at {:6.0} ms", what, found.len(), best, self.t0.elapsed().as_secs_f64() * 1e3);
            }
        };
        let from: Vec<Partial> = (0..self.roots.len())
            .flat_map(|cand| {
                (0..3).map(move |axis| Partial { cand, moves: Vec::new(), ours: Vec::new(), frame: 2 * axis, ends: Vec::new() })
            })
            .collect();
        let rider = TripleRider { s, phase: 0 };
        let found = e5.phase_c_ridden(&self.starts_of(&from), &ride(&from, 0), &rider, p.inner.labellings, &beam(p.inner.c_keep));
        let after_c = self.extend_ridden(&from, found);
        lap("phase C", &after_c);
        let from: Vec<Partial> = after_c
            .iter()
            .flat_map(|x| {
                (0..2).map(move |quarter| Partial {
                    cand: x.cand,
                    moves: x.moves.clone(),
                    ours: x.ours.clone(),
                    frame: x.frame + quarter,
                    ends: x.ends.clone(),
                })
            })
            .collect();
        let rider = TripleRider { s, phase: 1 };
        let found = e5.phase_p_ridden(&self.starts_of(&from), &ride(&from, 1), &rider, &beam(p.inner.p_keep));
        let after_p = self.extend_ridden(&from, found);
        lap("phase P", &after_p);
        let rider = TripleRider { s, phase: 2 };
        let found = e5.phase_e_ridden(&self.starts_of(&after_p), &ride(&after_p, 2), &rider, &beam(p.inner.e_keep));
        let reduced = self.extend_ridden(&after_p, found);
        lap("phase E", &reduced);
        reduced
    }

    /// The three edge phases; the results are reduced cubes.
    fn edges(&self, p: &Params5) -> Vec<Partial> {
        let e5 = &self.s.s5.edges;
        let until = |share: f64| (p.time_ms > 0).then(|| self.t0 + Duration::from_secs_f64(p.time_ms as f64 * share / 1e3));
        let limits = |keep: usize, per_node: usize, share: f64| Limits {
            keep,
            cap: keep * 8,
            per_node,
            deadline: until(share),
            floor: (keep / 8).clamp(4, 64),
        };
        // Phase C, along each axis.
        let from: Vec<Partial> = (0..self.roots.len())
            .flat_map(|cand| {
                (0..3).map(move |axis| Partial { cand, moves: Vec::new(), ours: Vec::new(), frame: 2 * axis, ends: Vec::new() })
            })
            .collect();
        let found = e5.phase_c(&self.starts_of(&from), p.labellings, &limits(p.c_keep, 10, 0.4));
        let after_c = self.extend(&from, found);
        // Phase P, with either of the two other axes turning freely.
        let from: Vec<Partial> = after_c
            .iter()
            .flat_map(|x| {
                (0..2).map(move |quarter| Partial {
                    cand: x.cand,
                    moves: x.moves.clone(),
                    ours: Vec::new(),
                    frame: x.frame + quarter,
                    ends: x.ends.clone(),
                })
            })
            .collect();
        let found = e5.phase_p(&self.starts_of(&from), &limits(p.p_keep, 10, 0.7));
        let after_p = self.extend(&from, found);
        // Phase E.
        let found = e5.phase_e(&self.starts_of(&after_p), &limits(p.e_keep, 2, 0.9));
        self.extend(&after_p, found)
    }

    fn total(&self, x: &Partial) -> usize {
        self.spent[x.cand] + x.moves.len()
    }

    /// Solves the 3x3x3 each reduced cube has become: a quick solve of every
    /// one, then the full effort on the most promising. Returns the partial
    /// that is shortest with its 3x3x3, and the 3x3x3 moves.
    fn finish<'b>(&self, reduced: &'b [Partial], p: &Params5) -> Option<(&'b Partial, Vec<u8>)> {
        let (t3, big) = (&self.s.s5.edges.t3, &self.s.s5.big);
        let mut quick: Vec<(usize, &Partial, C3, Vec<u8>)> = reduced
            .par_iter()
            .map(|x| {
                let cube = self.start_in(x).cube;
                debug_assert!(is_reduced(&cube));
                let c3 = C3 { cp: cube.c4.cp, co: cube.c4.co, ep: cube.mid.ep, eo: cube.mid.eo };
                let sol = t3.solve(&c3, 30, Some(big));
                (self.total(x) + sol.len(), x, c3, sol)
            })
            .collect();
        quick.sort_by_key(|r| r.0);
        quick.truncate(p.pool);
        let best = quick
            .into_par_iter()
            .map(|(len, x, c3, sol)| {
                let better = t3.solve(&c3, p.probes, Some(big));
                match better.len() < sol.len() {
                    true => (self.total(x) + better.len(), x, better),
                    false => (len, x, sol),
                }
            })
            .min_by_key(|r| r.0)?;
        let to_orig = &self.s.frames[best.1.frame].to_orig;
        Some((best.1, best.2.iter().map(|&m| to_orig[m as usize]).collect()))
    }
}

/// How wide the search goes.
#[derive(Clone, Debug)]
pub struct Params7 {
    /// Stagings carried from step to step while staging.
    pub width: usize,
    /// Stagings taken on to the matching step, and matchings kept of each.
    pub tries: usize,
    pub matchings: usize,
    /// Lengths past the shortest matching that are searched as well.
    pub match_extra: usize,
    /// States kept per level in the beam searches.
    pub beam: usize,
    /// True: no matching step; the edge phases of the inner cube bring the
    /// triples to one colour themselves (see `TripleRider`).
    pub ride: bool,
    /// The beam searches of the inner cube's edge phases: states kept per
    /// level, levels searched past the first with a solution, and the
    /// weights of the edges' and the triples' bounds in the ranking.
    pub inner_beam: usize,
    pub more: usize,
    pub mix: [u32; 2],
    /// The edge phases of the inner cube, and how many of its reductions go on.
    pub inner: Params5,
    pub reductions: usize,
    /// The outer cube.
    pub outer: Params5,
}

impl Params7 {
    pub fn normal() -> Params7 {
        Params7 {
            width: 4,
            tries: 4,
            matchings: 4,
            match_extra: 0,
            beam: 20_000,
            ride: true,
            inner_beam: 20_000,
            more: 1,
            mix: [1, 1],
            inner: Params5 { labellings: 4, c_keep: 60, p_keep: 60, e_keep: 30, time_ms: 0, ..Params5::normal() },
            reductions: 16,
            outer: Params5::normal(),
        }
    }
}

pub struct Solution7 {
    pub moves: Vec<u8>,
    /// Moves per step: staging, matching, the inner cube's edges, the outer
    /// cube's edges, the 3x3x3.
    pub lens: [usize; 5],
}

pub struct Solver7 {
    pub s7: Stage7,
    pub s5: Solver5,
    pub matching: Matching,
    pub triples: Triples,
    frames: Vec<Frame>,
}

impl Solver7 {
    pub fn load() -> Solver7 {
        let s7 = Stage7::load();
        let matching = Matching::new(&s7);
        let g = &tables5().geometry;
        // As in the 5x5x5 solver: three choices of the axis the wings are split
        // along, each with the cube as it is or turned a quarter about that axis.
        let mut frames = Vec::new();
        for axis in [vec![], vec![0, 1], vec![0, 1, 0, 1]] {
            for quarter in [false, true] {
                let mut turns: Vec<usize> = axis.clone();
                if quarter {
                    turns.push(0);
                }
                let to_orig: Vec<u8> = g.frame_moves(&turns).into_iter().map(|m| m as u8).collect();
                let mut from_orig = vec![0u8; to_orig.len()];
                for (m, &o) in to_orig.iter().enumerate() {
                    from_orig[o as usize] = m as u8;
                }
                // The faces that R, F and U of this frame are on the cube as given.
                let axes = [3, 6, 0].map(|m| [0, 2, 1][to_orig[m] as usize / 3 % 3]);
                let to7: Vec<u8> = s7.g.frame_moves(&turns).into_iter().map(|m| m as u8).collect();
                assert!((0..54).all(|m| inner_move(to7[m]) == to_orig[inner_move(m as u8) as usize]));
                frames.push(Frame { turns, to_orig, from_orig, axes, to7 });
            }
        }
        let triples = Triples::new(&s7);
        Solver7 { s7, s5: Solver5::load(), matching, triples, frames }
    }

    /// Step 2 from a staged cube: the shortest matchings (up to `cap`).
    pub fn match_blocks(&self, f: &[u8], extra: usize, width: usize, cap: usize) -> Vec<Vec<u8>> {
        let mut root = [0u32; 5];
        for i in 0..5 {
            root[i] = self.s7.primary(MATCHED[i], f);
        }
        let _ = extra;
        let p = Match { s7: &self.s7, m: &self.matching };
        // The bound is far below the true length, so: beam search, states
        // ranked by the half moves the three axes need.
        let score = |n: &[u32; 5]| p.axis(n, 0) as u32 + p.axis(n, 1) as u32 + p.axis(n, 2) as u32;
        beam(&p, &root, score, width, cap, 40)
    }

    /// True if the outer cube is a 5x5x5: every triple of one colour, the
    /// inner centres solved, every inner wing beside its middle edge.
    pub fn outer_ready(&self, f: &[u8]) -> bool {
        let o = &self.s7.orbits;
        let triples = (0..24).all(|i| f[o[OBL_A].facelet[i]] == f[o[T_OUT].facelet[i]] && f[o[OBL_B].facelet[i]] == f[o[T_OUT].facelet[i]]);
        let at = |face: usize, r: usize, c: usize| f[face * 49 + r * 7 + c];
        let inner = (0..6).all(|face| (2..5).all(|r| (2..5).all(|c| at(face, r, c) == face as u8)));
        let edges = (0..6).all(|face| {
            [0, 6].iter().all(|&k| {
                at(face, k, 2) == at(face, k, 3) && at(face, k, 4) == at(face, k, 3) && at(face, 2, k) == at(face, 3, k) && at(face, 4, k) == at(face, 3, k)
            })
        });
        triples && inner && edges
    }

    /// Solves a cube given as 294 facelets (faces U R F D L B, row by row).
    pub fn solve(&self, facelets: &[u8], p: &Params7) -> Result<Solution7, String> {
        let g = &self.s7.g;
        let apply = |f: &[u8], moves: &[u8]| {
            let mut f = f.to_vec();
            g.apply_seq(&mut f, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
            f
        };
        let trace = std::env::var_os("CUBE7_TRACE").is_some();
        let clock = Instant::now();
        let lap = |what: &str, n: usize, best: usize| {
            if trace {
                eprintln!("  {:10} {:5} results, shortest {:3}, at {:6.0} ms", what, n, best, clock.elapsed().as_secs_f64() * 1e3);
            }
        };
        // Step 1.
        let staged = self.s7.stage(&self.s5.stage, facelets, p.width, p.beam);
        lap("staged", staged.len(), staged.first().map_or(0, |s| s.moves.len()));
        // Step 2: (moves so far, lengths of steps 1 and 2, the cube).
        let mut matched: Vec<(Vec<u8>, [usize; 2], Vec<u8>)> = if p.ride {
            staged[..staged.len().min(p.tries)].iter().map(|st| (st.moves.clone(), [st.moves.len(), 0], apply(facelets, &st.moves))).collect()
        } else {
            staged[..staged.len().min(p.tries)]
            .par_iter()
            .flat_map_iter(|st| {
                let f1 = apply(facelets, &st.moves);
                let mut sols = self.match_blocks(&f1, p.match_extra, p.beam, p.matchings);
                sols.truncate(p.matchings);
                sols.into_iter()
                    .map(|sol| {
                        let f2 = apply(&f1, &sol);
                        let moves: Vec<u8> = st.moves.iter().chain(sol.iter()).copied().collect();
                        (moves, [st.moves.len(), sol.len()], f2)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
        };
        matched.sort_by_key(|m| m.0.len());
        lap("matched", matched.len(), matched.first().map_or(0, |m| m.0.len()));
        // Step 3: the inner cube, from every matching at once.
        let cands: Vec<(Vec<u8>, usize)> = matched.iter().map(|m| (g.sub_facelets(&m.2, &INNER5), m.0.len())).collect();
        let chain = Chain::new(self, &cands)?;
        let mut reduced = if p.ride {
            let cubes: Vec<Vec<u8>> = matched.iter().map(|m| m.2.clone()).collect();
            chain.edges_ridden(&cubes, p)
        } else {
            chain.edges(&p.inner)
        };
        reduced.sort_by_key(|x| chain.total(x));
        reduced.truncate(p.reductions);
        lap("inner", reduced.len(), reduced.first().map_or(0, |x| chain.total(x)));
        // (moves so far, lengths of steps 1 to 3, the cube).
        let inner: Vec<(Vec<u8>, [usize; 3], Vec<u8>)> = reduced
            .iter()
            .map(|x| {
                let (before, lens, f2) = &matched[x.cand];
                let ours: Vec<u8> = if p.ride { x.ours.clone() } else { x.moves.iter().map(|&m| from_inner(m)).collect() };
                let f3 = apply(f2, &ours);
                let moves: Vec<u8> = before.iter().chain(ours.iter()).copied().collect();
                (moves, [lens[0], lens[1], ours.len()], f3)
            })
            .collect();
        if let Some(bad) = inner.iter().find(|x| !self.outer_ready(&x.2)) {
            return Err(format!("the outer cube is not a 5x5x5 after {} moves", bad.0.len()));
        }
        // Step 4: the outer cube, from every reduction at once.
        let cands: Vec<(Vec<u8>, usize)> = inner.iter().map(|x| (g.sub_facelets(&x.2, &OUTER5), x.0.len())).collect();
        let chain = Chain::new(self, &cands)?;
        let reduced = chain.edges(&p.outer);
        lap("outer", reduced.len(), reduced.iter().map(|x| chain.total(x)).min().unwrap_or(0));
        let (x, last) = chain.finish(&reduced, &p.outer).ok_or("no reduction of the outer cube found")?;
        let (before, lens, _) = &inner[x.cand];
        let moves: Vec<u8> = before.iter().chain(x.moves.iter()).chain(last.iter()).copied().collect();
        lap("solved", 1, moves.len());
        Ok(Solution7 { lens: [lens[0], lens[1], lens[2], x.moves.len(), last.len()], moves })
    }
}
