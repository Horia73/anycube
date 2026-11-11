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
use crate::edges5::{is_reduced, Found, Limits, Start};
use crate::solver5::{Params5, Solver5};
use crate::search::{beam_roots, Problem};
use crate::stage7::{from_inner, moves_where, Stage7, Wide, INNER5, OBL_A, OBL_B, OUTER5, T_IN, T_OUT, X_IN, X_OUT};
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

}

/// The cube seen from another side (a 5x5x5).
struct Frame {
    turns: Vec<usize>,
    /// Move of the original cube for each move of this frame, and back.
    to_orig: Vec<u8>,
    from_orig: Vec<u8>,
}

/// A 5x5x5 part of the way through its edge phases: the candidate it
/// started from, its moves so far in the moves of the cube as given, the
/// frame it is searched in, and the length after each finished phase.
struct Partial {
    cand: usize,
    moves: Vec<u8>,
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
                Partial { cand: base.cand, moves, frame: base.frame, ends }
            })
            .collect()
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
            .flat_map(|cand| (0..3).map(move |axis| Partial { cand, moves: Vec::new(), frame: 2 * axis, ends: Vec::new() }))
            .collect();
        let found = e5.phase_c(&self.starts_of(&from), p.labellings, &limits(p.c_keep, 10, 0.4));
        let after_c = self.extend(&from, found);
        // Phase P, with either of the two other axes turning freely.
        let from: Vec<Partial> = after_c
            .iter()
            .flat_map(|x| {
                (0..2).map(move |quarter| Partial { cand: x.cand, moves: x.moves.clone(), frame: x.frame + quarter, ends: x.ends.clone() })
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
    /// Stagings carried from step to step while staging, and the ways each
    /// is continued by a step.
    pub width: usize,
    pub fan: usize,
    /// States kept per level in the beam searches (two staging steps and
    /// the matching), and how many times each is run with its ties broken
    /// differently.
    pub beam: usize,
    pub match_beam: usize,
    /// How many lengths past the shortest the matching goes on for: the
    /// edge phases that follow choose among all of them.
    pub match_more: usize,
    pub restarts: usize,
    /// While staging: solutions a beam search hands over to be judged with
    /// the step after it, how many lengths past its shortest it goes on
    /// for, and whether the order of the axes is chosen.
    pub many: usize,
    pub more: usize,
    pub frames: bool,
    /// Whether each beam search step is one search from everything carried
    /// into it, or one search for each.
    pub joint: bool,
    /// Whether the first two staging steps are one search, and what the
    /// inner centres count for in its score.
    pub merged: bool,
    pub inner_weight: u32,
    pub strict: bool,
    /// Whether the last two staging steps are one search, and what the
    /// outer centres count for in its score.
    pub rest: bool,
    pub outer_weight: u32,
    /// Whether the first three staging steps are one search.
    pub first: bool,
    /// Stagings taken on to the matching step, and matchings kept of each.
    pub tries: usize,
    pub matchings: usize,
    /// The edge phases of the inner cube, and how many of its reductions go on.
    pub inner: Params5,
    pub reductions: usize,
    /// The outer cube.
    pub outer: Params5,
}

impl Params7 {
    /// About ten seconds per cube.
    pub fn fast() -> Params7 {
        Params7 {
            width: 8,
            fan: 8,
            beam: 20_000,
            match_beam: 20_000,
            match_more: 1,
            restarts: 1,
            many: 400,
            more: 1,
            frames: true,
            joint: true,
            merged: true,
            inner_weight: 2,
            strict: false,
            rest: true,
            outer_weight: 1,
            first: false,
            tries: 8,
            matchings: 4,
            inner: Params5 { c_keep: 500, p_keep: 500, e_keep: 100, time_ms: 0, ..Params5::normal() },
            reductions: 16,
            // No clock: a phase cut short can leave the next one nothing to work with.
            outer: Params5 { time_ms: 0, ..Params5::normal() },
        }
    }

    /// About half a minute per cube.
    pub fn normal() -> Params7 {
        Params7 { beam: 100_000, match_beam: 100_000, ..Params7::fast() }
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
                frames.push(Frame { turns, to_orig, from_orig });
            }
        }
        Solver7 { s7, s5: Solver5::load(), matching, frames }
    }

    /// Step 2 from a staged cube: the shortest matchings (up to `cap`).
    pub fn match_blocks(&self, f: &[u8], width: usize, cap: usize, salt: u32) -> Vec<Vec<u8>> {
        self.match_all(&[(f, 0)], width, cap, salt, 0).into_iter().map(|found| found.1).collect()
    }

    /// Step 2 from several staged cubes in one search, each with the moves
    /// it is behind the shortest: up to `cap` matchings that are shortest
    /// with the staging before them and up to `cap` of each of the next
    /// `more` lengths, each with the number of its cube.
    pub fn match_all(&self, cubes: &[(&[u8], usize)], width: usize, cap: usize, salt: u32, more: usize) -> Vec<(usize, Vec<u8>)> {
        let roots: Vec<([u32; 5], usize)> = cubes
            .iter()
            .map(|(f, late)| {
                let mut root = [0u32; 5];
                for i in 0..5 {
                    root[i] = self.s7.primary(MATCHED[i], f);
                }
                (root, *late)
            })
            .collect();
        let p = Match { s7: &self.s7, m: &self.matching };
        // The bound is far below the true length, so: beam search, states
        // ranked by the half moves the three axes need.
        let score = |n: &[u32; 5]| p.axis(n, 0) as u32 + p.axis(n, 1) as u32 + p.axis(n, 2) as u32;
        (0..4)
            .map(|again| beam_roots(&p, &roots, score, width, cap, 40, salt + 1000 * again, &|_| true, more))
            .find(|found| !found.is_empty())
            .unwrap_or_default()
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
        // Beam searches whose ties are broken differently give stagings a
        // move or two apart: a few runs, and the shortest of all go on.
        let wide = |salt: u32| Wide { width: p.width, fan: p.fan, beam: p.beam, salt, many: p.many, more: p.more, frames: p.frames, joint: p.joint, merged: p.merged, inner_weight: p.inner_weight, strict: p.strict, rest: p.rest, outer_weight: p.outer_weight, first: p.first };
        let mut staged: Vec<_> = (0..p.restarts as u32).flat_map(|salt| self.s7.stage(&self.s5.stage, facelets, &wide(salt))).collect();
        staged.sort_by(|x, y| (x.moves.len(), &x.moves).cmp(&(y.moves.len(), &y.moves)));
        staged.dedup_by(|x, y| x.moves == y.moves);
        lap("staged", staged.len(), staged.first().map_or(0, |s| s.moves.len()));
        // Step 2: (moves so far, lengths of steps 1 and 2, the cube).
        let tried = &staged[..staged.len().min(p.tries)];
        let cubes: Vec<Vec<u8>> = tried.iter().map(|st| apply(facelets, &st.moves)).collect();
        let least = tried.iter().map(|st| st.moves.len()).min().unwrap_or(0);
        let found: Vec<(usize, Vec<u8>)> = match p.joint {
            // One search from all the stagings.
            true => {
                let roots: Vec<(&[u8], usize)> = cubes.iter().zip(tried).map(|(f, st)| (&f[..], st.moves.len() - least)).collect();
                (0..p.restarts as u32).flat_map(|salt| self.match_all(&roots, p.match_beam, p.matchings * tried.len(), salt, p.match_more)).collect()
            }
            // One search for each.
            false => (0..tried.len())
                .into_par_iter()
                .flat_map_iter(|i| {
                    (0..p.restarts as u32)
                        .flat_map(|salt| self.match_blocks(&cubes[i], p.match_beam, p.matchings, salt))
                        .map(|sol| (i, sol))
                        .collect::<Vec<_>>()
                })
                .collect(),
        };
        let mut matched: Vec<(Vec<u8>, [usize; 2], Vec<u8>)> = found
            .par_iter()
            .map(|(i, sol)| {
                let st = &tried[*i];
                let moves: Vec<u8> = st.moves.iter().chain(sol.iter()).copied().collect();
                (moves, [st.moves.len(), sol.len()], apply(&cubes[*i], sol))
            })
            .collect();
        matched.sort_by_key(|m| m.0.len());
        lap("matched", matched.len(), matched.first().map_or(0, |m| m.0.len()));
        // Step 3: the inner cube, from every matching at once.
        let cands: Vec<(Vec<u8>, usize)> = matched.iter().map(|m| (g.sub_facelets(&m.2, &INNER5), m.0.len())).collect();
        let chain = Chain::new(self, &cands)?;
        let mut reduced = chain.edges(&p.inner);
        reduced.sort_by_key(|x| chain.total(x));
        reduced.truncate(p.reductions);
        lap("inner", reduced.len(), reduced.first().map_or(0, |x| chain.total(x)));
        // (moves so far, lengths of steps 1 to 3, the cube).
        let inner: Vec<(Vec<u8>, [usize; 3], Vec<u8>)> = reduced
            .iter()
            .map(|x| {
                let (before, lens, f2) = &matched[x.cand];
                let ours: Vec<u8> = x.moves.iter().map(|&m| from_inner(m)).collect();
                let moves: Vec<u8> = before.iter().chain(ours.iter()).copied().collect();
                (moves, [lens[0], lens[1], ours.len()], apply(f2, &ours))
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
