//! 7x7x7, first part: every centre onto a face of its own axis, and both
//! wing orbits into even permutations.
//!
//! The six centre orbits are the inner x and inner t centres (the centres of
//! the 5x5x5 formed by layers 1, 3, 4, 5 and 7), the outer x centres, the
//! outer t centres and two mirror orbits of obliques. In the row next to an
//! edge of a face an oblique, an outer t centre and the other oblique lie
//! side by side: a *triple*. Turns of one or two layers move a triple as one
//! piece, the way the t centre of a 5x5x5 moves; only 3-layer turns take
//! triples apart. A triple is *formed* for an axis when its three centres
//! are all of that axis or none is. Five steps, each keeping what the ones
//! before achieved:
//!
//! 1. R/L inner x and inner t centres onto R and L: phase A of the 5x5x5,
//!    on the inner cube (its wide turn is our 3-layer turn);
//! 2. R/L triples formed. Only the 3-layer turns that keep step 1 (any turn
//!    of R and L, half turns of the rest) change triples; turns of one and
//!    two layers set them up;
//! 3. R/L outer x centres and triples onto R and L with 1- and 2-layer
//!    turns: phase A of the 5x5x5 again, on the outer cube;
//! 4. U/D inner x and inner t centres onto U and D, inner wings even, U/D
//!    triples formed, in one search guided by the exact table of phase B
//!    and exact tables for each two of the three orbits of a triple;
//! 5. U/D outer x centres and triples onto U and D, outer wings even:
//!    phase B of the 5x5x5 on the outer cube.
//!
//! Steps 2 and 4 are beam searches (`search`): their lower bounds are 5 to
//! 6 and about 10 where the shortest solutions have 11 and 14 moves.

use crate::cube::Cube;
use crate::cuben::Geometry;
use crate::orbits::{centre_orbits, CentreOrbit};
use crate::stage5::Stage5;
use crate::search::{beam_again_longer, beam_roots, fetch, Problem};
use crate::tables::{load_or_gen, table_dir, BitPerm, Bytes, Ranks};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

const GOAL_RL: u32 = 0xFF_0000;
const GOAL_UD: u32 = 0x00_00FF;
const N_UD: usize = 12870;
const FAR: u8 = 255;
const UNSEEN: u8 = 15;

/// Orbit indices, in the order `centre_orbits` lists them for a 7x7x7.
pub const X_OUT: usize = 0;
pub const OBL_A: usize = 1;
pub const T_OUT: usize = 2;
pub const OBL_B: usize = 3;
pub const X_IN: usize = 4;
pub const T_IN: usize = 5;

/// Layers of the inner and of the outer 5x5x5, and of the 4x4x4 that shows
/// the inner or the outer wings.
pub const INNER5: [usize; 5] = [0, 2, 3, 4, 6];
pub const OUTER5: [usize; 5] = [0, 1, 3, 5, 6];
pub const INNER_WINGS: [usize; 4] = [0, 2, 4, 6];
pub const OUTER_WINGS: [usize; 4] = [0, 1, 5, 6];

/// Our move for a move of the inner 5x5x5: its wide turn is our 3-layer turn.
#[inline(always)]
pub fn from_inner(m: u8) -> u8 {
    if m < 18 {
        m
    } else {
        m + 18
    }
}

/// Move numbers: `face * 3 + power` for faces U R F D L B, plus 18 per extra layer.
pub fn moves_where(keep: impl Fn(usize, usize, usize) -> bool) -> Vec<u8> {
    (0..54u8).filter(|&m| keep(m as usize / 3 % 6, m as usize / 18 + 1, m as usize % 3)).collect()
}

/// 3-layer quarter turns move one inner slice a quarter turn: an odd permutation of the inner wings.
#[inline(always)]
fn flips_inner(m: u8) -> bool {
    m >= 36 && m % 3 != 1
}

#[inline(always)]
fn nib(bytes: &[AtomicU8], idx: usize) -> u8 {
    (bytes[idx >> 1].load(Relaxed) >> ((idx & 1) * 4)) & 15
}

/// Sets an unseen entry to `v`; returns true if this call did it.
#[inline(always)]
fn nib_set(bytes: &[AtomicU8], idx: usize, v: u8) -> bool {
    let shift = (idx & 1) * 4;
    let cell = &bytes[idx >> 1];
    let mut cur = cell.load(Relaxed);
    loop {
        if (cur >> shift) & 15 != UNSEEN {
            return false;
        }
        match cell.compare_exchange_weak(cur, (cur & !(15 << shift)) | v << shift, Relaxed, Relaxed) {
            Ok(_) => return true,
            Err(now) => cur = now,
        }
    }
}

#[inline(always)]
fn nibble(bytes: &[u8], idx: usize) -> u8 {
    (bytes[idx >> 1] >> ((idx & 1) * 4)) & 15
}

fn into_bytes(v: Vec<AtomicU8>) -> Vec<u8> {
    let mut v = std::mem::ManuallyDrop::new(v);
    // AtomicU8 has the same layout as u8.
    unsafe { Vec::from_raw_parts(v.as_mut_ptr() as *mut u8, v.len(), v.capacity()) }
}

pub struct Stage7 {
    pub g: Geometry,
    pub orbits: Vec<CentreOrbit>,
    pub perms: Vec<BitPerm>,
    pub ranks: Ranks,
    /// Step 2: its moves, and for each two of the three orbits of a triple
    /// (the obliques, first oblique and t, t and second oblique) a lower
    /// bound on the moves that make them agree, by the mask of places where
    /// they differ.
    m2: Vec<u8>,
    diff2: [Bytes; 3],
    /// Steps 1 and 2 as one search (`MERGED`): every move, and the bounds of
    /// step 2 with every move allowed.
    m1: Vec<u8>,
    diff1: [Bytes; 3],
    /// Step 4: its moves; where they take the U/D pattern (one of 12,870) of
    /// the inner x, inner t, first oblique, outer t, second oblique and
    /// outer x orbits; and the exact moves to make each two orbits of a
    /// triple agree.
    m4: Vec<u8>,
    mv4: [Vec<u16>; 6],
    pair4: [Bytes; 3],
    /// The six orders in which the axes can be staged, as frames: frame
    /// `2 * k` has the cube turned so that axis `k` is its R/L axis, frame
    /// `2 * k + 1` is that frame turned a quarter about R (its other two
    /// axes swapped). With each, our move for each move of the frame.
    frames: Vec<(Vec<usize>, Vec<u8>)>,
}

/// How wide the staging goes.
#[derive(Clone, Debug)]
pub struct Wide {
    /// Stagings carried from step to step, and how many continuations of
    /// each are kept by a step.
    pub width: usize,
    pub fan: usize,
    /// States per level in the beam searches, and what decides their ties.
    pub beam: usize,
    pub salt: u32,
    /// Solutions a beam search hands over to be judged with an estimate of
    /// the step after it (the `fan` best stay), and how many lengths past
    /// its shortest it goes on for.
    pub many: usize,
    pub more: usize,
    /// Whether the order of the axes is chosen (whole 7x7x7 cubes only).
    pub frames: bool,
    /// Whether a beam search step is one search from all the stagings
    /// carried (`search::beam_roots`) or one search for each.
    pub joint: bool,
    /// Whether steps 1 and 2 are one search (`MERGED`).
    pub merged: bool,
    /// What the inner centres count for in the score of that search, and
    /// whether the triples are scored with the bounds of step 2 (which
    /// suppose the inner centres stay put) instead of its own weaker ones.
    pub inner_weight: u32,
    pub strict: bool,
    /// Whether steps 4 and 5 are one search (`REST`), and what the outer
    /// centres count for in its score.
    pub rest: bool,
    pub outer_weight: u32,
    /// Whether steps 1 to 3 are one search (`FIRST`; `outer_weight` is
    /// what the outer centres count for in its score too).
    pub first: bool,
}

/// Steps 1 and 2 in one: R/L inner centres onto R and L and R/L triples
/// formed, with every move. Step 2 by itself may only use the turns that
/// keep the inner centres where step 1 put them, and takes as long as step 1.
const MERGED: usize = 5;
/// Steps 4 and 5 in one: everything U/D onto U and D, with the moves of step 4.
const REST: usize = 6;

/// Steps 1 to 3 in one: everything R/L onto R and L, with every move.
const FIRST: usize = 7;

/// Where the length of a step is kept in `Staged7::lens`.
fn slot(step: usize) -> usize {
    match step {
        MERGED | FIRST => 0,
        REST => 3,
        step => step,
    }
}

/// A cube part of the way through its staging: the moves so far (in the
/// moves of the cube as given), the cube as its frame sees it, the frame.
struct Cand {
    st: Staged7,
    f: Vec<u8>,
    frame: usize,
}

/// A staging: the moves and how many each step took.
#[derive(Clone)]
pub struct Staged7 {
    pub moves: Vec<u8>,
    pub lens: [usize; 5],
}

/// A lower bound on the moves that make two orbits of a triple agree on one
/// axis, from the set of places where one centre is of the axis and its
/// neighbour in the other orbit is not. Turns that move the two orbits alike
/// only move that set around. Where a turn moves them differently it brings
/// new neighbours together; here it may leave those places in any state,
/// which can only make the count too small.
fn gen_diff(p: &CentreOrbit, q: &CentreOrbit, perm: &BitPerm, moves: &[u8]) -> Vec<u8> {
    let acts: Vec<(u8, u32, [u8; 24])> = moves
        .iter()
        .map(|&m| {
            let (a, b) = (&p.src[m as usize], &q.src[m as usize]);
            let touched = (0..24).filter(|&i| a[i] != b[i]).fold(0u32, |x, i| x | 1 << i);
            (m, touched, *a)
        })
        .collect();
    let dist: Vec<AtomicU8> = (0..1usize << 24).into_par_iter().map(|_| AtomicU8::new(FAR)).collect();
    dist[0].store(0, Relaxed);
    let set = |x: usize, v: u8| dist[x].load(Relaxed) == FAR && dist[x].compare_exchange(FAR, v, Relaxed, Relaxed).is_ok();
    for depth in 0..30u8 {
        let n: u64 = (0..1u32 << 24)
            .into_par_iter()
            .with_min_len(1 << 12)
            .map(|d| {
                if dist[d as usize].load(Relaxed) != depth {
                    return 0;
                }
                let mut new = 0;
                for (m, touched, src) in &acts {
                    if *touched == 0 {
                        new += set(perm.apply(*m, d) as usize, depth + 1) as u64;
                        continue;
                    }
                    let base = (0..24).filter(|&i| (touched >> i) & 1 == 0).fold(0u32, |x, i| x | ((d >> src[i]) & 1) << i);
                    // Every subset of the places where new neighbours meet.
                    let mut sub = *touched;
                    loop {
                        new += set((base | sub) as usize, depth + 1) as u64;
                        if sub == 0 {
                            break;
                        }
                        sub = (sub - 1) & touched;
                    }
                }
                new
            })
            .sum();
        eprintln!("  7x7x7 triple bound: depth {:2}  {:>10} new", depth + 1, n);
        if n == 0 {
            break;
        }
    }
    into_bytes(dist)
}

/// Exact moves until two orbits show the same U/D pattern, for every two
/// patterns, four bits each. `p[r * moves + k]` is pattern `r` after move `k`.
fn gen_pair(p: &[u16], q: &[u16], moves: usize) -> Vec<u8> {
    let dist: Vec<AtomicU8> = (0..N_UD * N_UD / 2).into_par_iter().map(|_| AtomicU8::new(0xFF)).collect();
    for a in 0..N_UD {
        nib_set(&dist, a * N_UD + a, 0);
    }
    let mut total = N_UD as u64;
    for depth in 0..UNSEEN - 1 {
        let n: u64 = (0..N_UD)
            .into_par_iter()
            .map(|a| {
                let mut new = 0;
                for b in 0..N_UD {
                    if nib(&dist, a * N_UD + b) != depth {
                        continue;
                    }
                    for k in 0..moves {
                        let j = p[a * moves + k] as usize * N_UD + q[b * moves + k] as usize;
                        new += nib_set(&dist, j, depth + 1) as u64;
                    }
                }
                new
            })
            .sum();
        total += n;
        eprintln!("  7x7x7 step 4 triple table: depth {:2}  {:>10} new", depth + 1, n);
        if n == 0 {
            break;
        }
    }
    assert_eq!(total as usize, N_UD * N_UD, "every two patterns can be made to agree within 14 moves");
    into_bytes(dist)
}

impl Stage7 {
    pub fn load() -> Stage7 {
        let g = Geometry::new(7);
        let orbits = centre_orbits(&g);
        assert_eq!(orbits.len(), 6);
        // The three orbits of a triple move alike under turns of one and two layers.
        for m in 0..36 {
            assert!(orbits[OBL_A].src[m] == orbits[T_OUT].src[m] && orbits[OBL_B].src[m] == orbits[T_OUT].src[m]);
        }
        let perms: Vec<BitPerm> = orbits.iter().map(|o| BitPerm::from_src(&o.src)).collect();
        let ranks = Ranks::new();
        let dir = table_dir();
        let side = |f: usize| f != 1 && f != 4;

        // Step 2: everything that keeps the inner R/L centres on R and L.
        let m2 = moves_where(|f, layers, power| layers < 3 || !side(f) || power == 1);
        let diff = |name: &str, p: usize, q: usize| {
            load_or_gen(&dir.join(name), 1 << 24, || gen_diff(&orbits[p], &orbits[q], &perms[p], &m2))
        };
        let diff2 = [
            diff("stage777-2-ab.bin", OBL_A, OBL_B),
            diff("stage777-2-at.bin", OBL_A, T_OUT),
            diff("stage777-2-tb.bin", T_OUT, OBL_B),
        ];

        let m1: Vec<u8> = (0..54).collect();
        let diff = |name: &str, p: usize, q: usize| {
            load_or_gen(&dir.join(name), 1 << 24, || gen_diff(&orbits[p], &orbits[q], &perms[p], &m1))
        };
        let diff1 = [
            diff("stage777-1-ab.bin", OBL_A, OBL_B),
            diff("stage777-1-at.bin", OBL_A, T_OUT),
            diff("stage777-1-tb.bin", T_OUT, OBL_B),
        ];

        // Step 4: U F D B face turns, turns of R and L of two or three layers,
        // half turns of two or three layers of the rest.
        let m4 = moves_where(|f, layers, power| match layers {
            1 => side(f),
            _ => !side(f) || power == 1,
        });
        let table = |o: usize| -> Vec<u16> {
            let mut out = Vec::with_capacity(N_UD * m4.len());
            for r in 0..N_UD {
                for &m in &m4 {
                    let y = perms[o].apply(m, ranks.unrank16[r] as u32);
                    assert!(y < 1 << 16, "these moves keep the R/L centres in place");
                    out.push(ranks.rank16[y as usize]);
                }
            }
            out
        };
        let mv4 = [table(X_IN), table(T_IN), table(OBL_A), table(T_OUT), table(OBL_B), table(X_OUT)];
        let pair = |name: &str, p: usize, q: usize| {
            load_or_gen(&dir.join(name), N_UD * N_UD / 2, || gen_pair(&mv4[p], &mv4[q], m4.len()))
        };
        let pair4 = [pair("stage777-4-ab.bin", 2, 4), pair("stage777-4-at.bin", 2, 3), pair("stage777-4-tb.bin", 3, 4)];
        let frames = [vec![], vec![0], vec![1], vec![1, 0], vec![2], vec![2, 0]]
            .into_iter()
            .map(|turns: Vec<usize>| {
                let map = g.frame_moves(&turns).into_iter().map(|m| m as u8).collect();
                (turns, map)
            })
            .collect();
        Stage7 { g, orbits, perms, ranks, m2, diff2, m1, diff1, m4, mv4, pair4, frames }
    }

    /// Positions of the centres of one axis colour pair (0 U/D, 1 R/L, 2 F/B) in an orbit.
    pub fn mask(&self, orbit: usize, facelets: &[u8], axis: u8) -> u32 {
        let col = self.orbits[orbit].colours(facelets);
        (0..24).fold(0, |m, i| m | ((col[i] % 3 == axis) as u32) << i)
    }

    /// Positions where an orbit shows the first colour of an axis (U, R or F).
    pub fn primary(&self, orbit: usize, facelets: &[u8]) -> u32 {
        let col = self.orbits[orbit].colours(facelets);
        (0..24).fold(0, |m, i| m | ((col[i] < 3) as u32) << i)
    }

    /// Parity of the permutation of the wings in the given layers.
    pub fn wings_odd(&self, facelets: &[u8], layers: [usize; 4]) -> bool {
        let sub = self.g.to_string(&self.g.sub_facelets(facelets, &layers));
        Cube::from_facelets(&sub).expect("four layers of a 7x7x7 form a 4x4x4").wing_parity() == 1
    }

    pub fn is_staged(&self, facelets: &[u8]) -> bool {
        (0..6).all(|o| self.mask(o, facelets, 1) == GOAL_RL && self.mask(o, facelets, 0) == GOAL_UD)
            && !self.wings_odd(facelets, INNER_WINGS)
            && !self.wings_odd(facelets, OUTER_WINGS)
    }

    /// The best stagings found, shortest first. `width` candidates are
    /// carried from step to step. Each is continued in up to `fan` ways, and
    /// the continuations kept are the ones shortest with an estimate of the
    /// step that follows. `beam` is the width of the beam searches and
    /// `salt` decides their ties (see `search::beam`).
    ///
    /// The axes are staged in whichever order comes out shortest: the first
    /// step starts along each of the three, and the second axis is chosen
    /// when the first is done, both by the same rule as everything else.
    pub fn stage(&self, s5: &Stage5, facelets: &[u8], w: &Wide) -> Vec<Staged7> {
        let start = |frame: usize| Cand { st: Staged7 { moves: Vec::new(), lens: [0; 5] }, f: self.g.reframe(facelets, &self.frames[frame].0), frame };
        let cands = if w.frames { vec![start(0), start(2), start(4)] } else { vec![start(0)] };
        let mut steps: Vec<usize> = match (w.first, w.merged) {
            (true, _) => vec![FIRST],
            (_, true) => vec![MERGED, 2],
            _ => vec![0, 1, 2],
        };
        steps.extend(if w.rest { vec![REST] } else { vec![3, 4] });
        self.run(s5, cands, w, &steps, true)
    }

    /// Some of the steps only (0 to 2 do the R/L centres, 3 and 4 the
    /// rest), from a cube on which the ones before them are done. For the
    /// larger cubes, which stage one axis on all their layers before the next.
    #[allow(clippy::too_many_arguments)]
    pub fn stage_steps(
        &self,
        s5: &Stage5,
        facelets: &[u8],
        width: usize,
        fan: usize,
        beam: usize,
        salt: u32,
        steps: std::ops::Range<usize>,
    ) -> Vec<Staged7> {
        let w = Wide { width, fan, beam, salt, many: fan, more: 0, frames: false, joint: false, merged: false, inner_weight: 2, strict: false, rest: false, outer_weight: 1, first: false };
        let start = Cand { st: Staged7 { moves: Vec::new(), lens: [0; 5] }, f: facelets.to_vec(), frame: 0 };
        let whole = steps.end == 5;
        self.run(s5, vec![start], &w, &steps.collect::<Vec<_>>(), whole)
    }

    /// `whole`: the steps given end with the last one.
    fn run(&self, s5: &Stage5, mut cands: Vec<Cand>, w: &Wide, steps: &[usize], whole: bool) -> Vec<Staged7> {
        let clock = std::time::Instant::now();
        let trace = std::env::var_os("CUBE7_TRACE").is_some();
        for (at, &step) in steps.iter().enumerate() {
            let after = steps.get(at + 1).copied();
            // Solutions of the step, each with the number of the staging it continues.
            let sols: Vec<(usize, Vec<u8>)> = if w.joint && (step == 1 || step == 3 || step >= MERGED) {
                self.beam_step(s5, step, &cands, w)
            } else {
                cands.par_iter().enumerate().flat_map_iter(|(i, c)| self.step(s5, step, &c.f, w).into_iter().map(move |sol| (i, sol))).collect()
            };
            // (staging so far, the cube, its frame, estimate of the step after this one, the staging continued).
            let mut next: Vec<(Staged7, Vec<u8>, usize, usize, usize)> = sols
                .par_iter()
                .flat_map_iter(|(i, sol)| {
                    let c = &cands[*i];
                    let to_ours = &self.frames[c.frame].1;
                    let mut f2 = c.f.clone();
                    self.g.apply_seq(&mut f2, &sol.iter().map(|&m| m as usize).collect::<Vec<_>>());
                    let mut st = c.st.clone();
                    st.lens[slot(step)] = sol.len();
                    st.moves.extend(sol.iter().map(|&m| to_ours[m as usize]));
                    let ahead = |f: &[u8]| after.map_or(0, |next| self.ahead(s5, next, f));
                    let mut out = Vec::with_capacity(2);
                    // With the first axis done, either of the other two may come next.
                    if w.frames && (step == 2 || step == FIRST) {
                        let turned = self.g.reframe(&f2, &[0]);
                        let est = ahead(&turned);
                        out.push((st.clone(), turned, c.frame + 1, est, *i));
                    }
                    let est = ahead(&f2);
                    out.push((st, f2, c.frame, est, *i));
                    out
                })
                .collect();
            // Of the continuations of one staging the `fan` best stay, then the `width` best of all.
            next.sort_by(|x, y| (x.4, x.0.moves.len() + x.3, &x.1, x.2).cmp(&(y.4, y.0.moves.len() + y.3, &y.1, y.2)));
            let mut run = (usize::MAX, 0);
            next.retain(|x| {
                if x.4 != run.0 {
                    run = (x.4, 0);
                }
                run.1 += 1;
                run.1 <= w.fan
            });
            next.sort_by(|x, y| (x.0.moves.len() + x.3, &x.1, x.2).cmp(&(y.0.moves.len() + y.3, &y.1, y.2)));
            next.dedup_by(|x, y| x.1 == y.1 && x.2 == y.2);
            // Where the second axis is chosen, each staging is there twice.
            next.truncate(if w.frames && (step == 2 || step == FIRST) { 2 * w.width } else { w.width });
            if trace {
                let lens: Vec<String> = next.iter().map(|x| format!("{}+{}/{}", x.0.moves.len(), x.3, x.2)).collect();
                eprintln!("    staging step {} at {:.0} ms: {}", step + 1, clock.elapsed().as_secs_f64() * 1e3, lens.join(" "));
            }
            cands = next.into_iter().map(|x| Cand { st: x.0, f: x.1, frame: x.2 }).collect();
        }
        debug_assert!(!whole || cands.iter().all(|c| self.is_staged(&c.f)));
        cands.into_iter().map(|c| c.st).collect()
    }

    /// A beam search step (the second or the fourth) as one search from all
    /// the stagings carried, each as many moves behind as it is longer than
    /// the shortest: solutions with the number of the staging they continue.
    fn beam_step(&self, s5: &Stage5, step: usize, cands: &[Cand], w: &Wide) -> Vec<(usize, Vec<u8>)> {
        let least = cands.iter().map(|c| c.st.moves.len()).min().unwrap_or(0);
        let late = |c: &Cand| c.st.moves.len() - least;
        for again in 0..4 {
            let salt = w.salt + 1000 * again;
            let found = if step == MERGED {
                let roots: Vec<([u32; 5], usize)> = cands.iter().map(|c| (self.node12(&c.f), late(c))).collect();
                beam_roots(&Step12 { s: self, s5 }, &roots, |n| self.score12(s5, n, w), w.beam, w.many, 60, salt, &|_| true, w.more)
            } else if step == FIRST {
                let roots: Vec<([u32; 6], usize)> = cands.iter().map(|c| (self.node123(&c.f), late(c))).collect();
                beam_roots(&Step123 { s: self, s5 }, &roots, |n| self.score123(s5, n, w), w.beam, w.many, 70, salt, &|_| true, w.more)
            } else if step == REST {
                let roots: Vec<(Node45, usize)> = cands.iter().map(|c| (self.node45(&c.f), late(c))).collect();
                beam_roots(&Step45 { s: self, s5 }, &roots, |n| self.score45(s5, n, w.outer_weight), w.beam, w.many, 60, salt, &|_| true, 0)
            } else if step == 1 {
                let roots: Vec<([u32; 3], usize)> =
                    cands.iter().map(|c| ([self.mask(OBL_A, &c.f, 1), self.mask(T_OUT, &c.f, 1), self.mask(OBL_B, &c.f, 1)], late(c))).collect();
                beam_roots(&Step2 { s: self }, &roots, |n| self.score2(n), w.beam, w.many, 40, salt, &|_| true, w.more)
            } else {
                let roots: Vec<(Node4, usize)> = cands.iter().map(|c| (self.node4(&c.f), late(c))).collect();
                beam_roots(&Step4 { s: self, s5, free: false }, &roots, |n| self.score4(s5, n, false), w.beam, w.many, 40, salt, &|_| true, w.more)
            };
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    fn node45(&self, f: &[u8]) -> Node45 {
        Node45 { n: self.node4(f), xo: self.ranks.rank16[self.mask(X_OUT, f, 0) as usize], odd: self.wings_odd(f, OUTER_WINGS) }
    }

    /// How the search for steps 4 and 5 in one ranks states: as step 4
    /// does, plus what step 5 would take if the triples were formed,
    /// `weight` times. Once step 4's part is done that is exactly what is left.
    #[inline(always)]
    fn score45(&self, s5: &Stage5, n: &Node45, weight: u32) -> u32 {
        self.score4(s5, &n.n, false) + weight * s5.b_dist(n.xo as usize, n.n.o as usize, n.odd) as u32
    }

    fn node123(&self, f: &[u8]) -> [u32; 6] {
        [X_IN, T_IN, OBL_A, T_OUT, OBL_B, X_OUT].map(|o| self.mask(o, f, 1))
    }

    /// How the search for steps 1 to 3 in one ranks states: as the one for
    /// steps 1 and 2, plus what step 3 would take if the triples were formed.
    #[inline(always)]
    fn score123(&self, s5: &Stage5, n: &[u32; 6], w: &Wide) -> u32 {
        let d = &self.diff1;
        let (x, y) = (n[2] ^ n[3], n[3] ^ n[4]);
        let rows = d[1][x as usize] as u32 + d[2][y as usize] as u32;
        let (inner, outer) = (s5.a_close(n[0], n[1]) as u32, s5.a_close(n[5], n[3]) as u32);
        (w.inner_weight * inner + rows + w.outer_weight * outer) * 64 + x.count_ones() + y.count_ones()
    }

    fn node12(&self, f: &[u8]) -> [u32; 5] {
        [X_IN, T_IN, OBL_A, T_OUT, OBL_B].map(|o| self.mask(o, f, 1))
    }

    /// How the search for steps 1 and 2 in one ranks states: as step 2
    /// does, with what the inner centres still need added `weight` times.
    #[inline(always)]
    fn score12(&self, s5: &Stage5, n: &[u32; 5], w: &Wide) -> u32 {
        let d = if w.strict { &self.diff2 } else { &self.diff1 };
        let (x, y) = (n[2] ^ n[3], n[3] ^ n[4]);
        (w.inner_weight * s5.a_close(n[0], n[1]) as u32 + d[1][x as usize] as u32 + d[2][y as usize] as u32) * 64 + x.count_ones() + y.count_ones()
    }

    /// How the beam search of step 2 ranks states: by the two bounds for the
    /// outer t centres against each oblique orbit, then by how many obliques
    /// do not agree with the outer t centre beside them.
    #[inline(always)]
    fn score2(&self, n: &[u32; 3]) -> u32 {
        let d = &self.diff2;
        let (x, y) = (n[0] ^ n[1], n[1] ^ n[2]);
        (d[1][x as usize] as u32 + d[2][y as usize] as u32) * 64 + x.count_ones() + y.count_ones()
    }

    /// What the inner centres need in step 4: exactly, with the parity of
    /// the inner wings to be made even or (`free`) left as it comes.
    #[inline(always)]
    fn inner4(&self, s5: &Stage5, n: &Node4, free: bool) -> u8 {
        let d = s5.b_dist(n.x as usize, n.t as usize, n.odd);
        match free {
            true => d.min(s5.b_dist(n.x as usize, n.t as usize, !n.odd)),
            false => d,
        }
    }

    /// For the larger cubes, whose steps are 7x7x7 cubes made of layers:
    /// the rows of several such cubes formed in one search
    /// (`search::beam_roots`). Each cube comes with the moves it is behind
    /// the shortest; the result is up to `cap` solutions that are shortest
    /// in total, each with the number of its cube.
    ///
    /// `pass` 0: the R/L triples formed, from cubes whose inner R/L centres
    /// are on R and L (step 2, or with `merged` the search of steps 1 and 2
    /// in one, which may move the inner centres away and bring them back).
    /// `pass` 1: the U/D inner centres on U and D and the U/D triples formed
    /// (step 4), the parity of the inner wings left as it comes.
    #[allow(clippy::too_many_arguments)]
    pub fn rows(&self, s5: &Stage5, pass: usize, cubes: &[(Vec<u8>, usize)], beam: usize, cap: usize, salt: u32, merged: bool) -> Vec<(usize, Vec<u8>)> {
        let all = &|_: &[u8]| true;
        match (pass, merged) {
            (0, true) => {
                let w = Wide { width: 1, fan: cap, beam, salt, many: cap, more: 0, frames: false, joint: true, merged, inner_weight: 2, strict: false, rest: false, outer_weight: 1, first: false };
                let roots: Vec<([u32; 5], usize)> = cubes.iter().map(|(f, late)| (self.node12(f), *late)).collect();
                beam_roots(&Step12 { s: self, s5 }, &roots, |n| self.score12(s5, n, &w), beam, cap, 60, salt, all, 0)
            }
            (0, false) => {
                let roots: Vec<([u32; 3], usize)> =
                    cubes.iter().map(|(f, late)| ([self.mask(OBL_A, f, 1), self.mask(T_OUT, f, 1), self.mask(OBL_B, f, 1)], *late)).collect();
                beam_roots(&Step2 { s: self }, &roots, |n| self.score2(n), beam, cap, 40, salt, all, 0)
            }
            _ => {
                let roots: Vec<(Node4, usize)> = cubes.iter().map(|(f, late)| (self.node4(f), *late)).collect();
                beam_roots(&Step4 { s: self, s5, free: true }, &roots, |n| self.score4(s5, n, true), beam, cap, 40, salt, all, 0)
            }
        }
    }

    /// How the beam search of step 4 ranks states. The two halves of the job
    /// share few moves, so the bound is weak: what the inner centres need
    /// (counted twice) and what the triples need.
    #[inline(always)]
    fn score4(&self, s5: &Stage5, n: &Node4, free: bool) -> u32 {
        let (a, o, b) = (n.a as usize, n.o as usize, n.b as usize);
        let t = &self.pair4;
        2 * self.inner4(s5, n, free) as u32
            + nibble(&t[0], a * N_UD + b) as u32
            + nibble(&t[1], a * N_UD + o) as u32
            + nibble(&t[2], o * N_UD + b) as u32
    }

    /// An estimate of the length of a step from a cube on which the steps
    /// before it are done: lower bounds for steps 3 and 4, the exact length
    /// for step 5, nothing for step 2 (a narrow beam search as estimate
    /// cost more time than it saved moves).
    fn ahead(&self, s5: &Stage5, step: usize, f: &[u8]) -> usize {
        match step {
            2 => s5.a_bound(self.mask(X_OUT, f, 1), self.mask(T_OUT, f, 1)) as usize,
            3 => Step4 { s: self, s5, free: false }.bound(&self.node4(f)),
            REST => Step45 { s: self, s5 }.bound(&self.node45(f)),
            4 => {
                let rank = |o: usize| self.ranks.rank16[self.mask(o, f, 0) as usize] as usize;
                s5.b_dist(rank(X_OUT), rank(T_OUT), self.wings_odd(f, OUTER_WINGS)) as usize
            }
            _ => 0,
        }
    }

    fn node4(&self, f: &[u8]) -> Node4 {
        let rank = |o: usize| self.ranks.rank16[self.mask(o, f, 0) as usize];
        Node4 { x: rank(X_IN), t: rank(T_IN), odd: self.wings_odd(f, INNER_WINGS), a: rank(OBL_A), o: rank(T_OUT), b: rank(OBL_B) }
    }

    /// Step 2 by beam search.
    fn step2(&self, root: &[u32; 3], width: usize, cap: usize, salt: u32, more: usize) -> Vec<Vec<u8>> {
        beam_again_longer(&Step2 { s: self }, root, |n| self.score2(n), width, cap, 40, salt, more)
    }

    /// Solutions of one step from a cube on which the earlier steps are
    /// done: up to `fan` of an exact search, taken evenly from all it found
    /// so that they differ from the first move on, and up to `many` per
    /// length of a beam search.
    fn step(&self, s5: &Stage5, step: usize, f: &[u8], w: &Wide) -> Vec<Vec<u8>> {
        let cap = w.fan;
        // The searches of phase A also give the solutions one move longer when more than a few are wanted.
        let extra = (cap > 8) as usize;
        let thin = |mut sols: Vec<Vec<u8>>| {
            if sols.len() > cap {
                let n = sols.len();
                sols = (0..cap).map(|i| std::mem::take(&mut sols[i * n / cap])).collect();
            }
            sols
        };
        match step {
            FIRST => {
                let p = Step123 { s: self, s5 };
                beam_again_longer(&p, &self.node123(f), |n| self.score123(s5, n, w), w.beam, w.many, 70, w.salt, w.more)
            }
            REST => {
                let p = Step45 { s: self, s5 };
                beam_again_longer(&p, &self.node45(f), |n| self.score45(s5, n, w.outer_weight), w.beam, w.many, 60, w.salt, 0)
            }
            MERGED => {
                let p = Step12 { s: self, s5 };
                beam_again_longer(&p, &self.node12(f), |n| self.score12(s5, n, w), w.beam, w.many, 60, w.salt, w.more)
            }
            0 => thin(
                s5.phase_a(self.mask(X_IN, f, 1), self.mask(T_IN, f, 1), extra, cap, None)
                    .into_iter()
                    .map(|sol| sol.into_iter().map(from_inner).collect())
                    .collect(),
            ),
            1 => self.step2(&[self.mask(OBL_A, f, 1), self.mask(T_OUT, f, 1), self.mask(OBL_B, f, 1)], w.beam, w.many, w.salt, w.more),
            2 => thin(s5.phase_a(self.mask(X_OUT, f, 1), self.mask(T_OUT, f, 1), extra, cap, None)),
            3 => {
                let p = Step4 { s: self, s5, free: false };
                beam_again_longer(&p, &self.node4(f), |n| self.score4(s5, n, false), w.beam, w.many, 40, w.salt, w.more)
            }
            _ => vec![s5.phase_b(self.mask(X_OUT, f, 0), self.mask(T_OUT, f, 0), self.wings_odd(f, OUTER_WINGS))],
        }
    }
}

/// Step 2: the R/L masks of the first oblique, outer t and second oblique orbits.
struct Step2<'a> {
    s: &'a Stage7,
}

impl Problem for Step2<'_> {
    type Node = [u32; 3];

    fn moves(&self) -> &[u8] {
        &self.s.m2
    }

    #[inline(always)]
    fn next(&self, n: &[u32; 3], k: usize) -> [u32; 3] {
        let (m, p) = (self.s.m2[k], &self.s.perms);
        [p[OBL_A].apply(m, n[0]), p[T_OUT].apply(m, n[1]), p[OBL_B].apply(m, n[2])]
    }

    #[inline(always)]
    fn bound(&self, n: &[u32; 3]) -> usize {
        let d = &self.s.diff2;
        d[0][(n[0] ^ n[2]) as usize].max(d[1][(n[0] ^ n[1]) as usize]).max(d[2][(n[1] ^ n[2]) as usize]) as usize
    }

    fn done(&self, n: &[u32; 3]) -> bool {
        n[0] == n[1] && n[1] == n[2]
    }

    #[inline(always)]
    fn prefetch(&self, n: &[u32; 3]) {
        let d = &self.s.diff2;
        fetch(&d[0], (n[0] ^ n[2]) as usize);
        fetch(&d[1], (n[0] ^ n[1]) as usize);
        fetch(&d[2], (n[1] ^ n[2]) as usize);
    }

}

/// Steps 4 and 5 in one: the state of step 4, the U/D pattern of the outer x
/// centres and the parity of the outer wings.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Node45 {
    n: Node4,
    xo: u16,
    odd: bool,
}

struct Step45<'a> {
    s: &'a Stage7,
    s5: &'a Stage5,
}

/// Quarter turns of two or three layers move an outer slice a quarter turn: an odd permutation of the outer wings.
#[inline(always)]
fn flips_outer(m: u8) -> bool {
    m >= 18 && m % 3 != 1
}

impl Problem for Step45<'_> {
    type Node = Node45;

    fn moves(&self) -> &[u8] {
        &self.s.m4
    }

    #[inline(always)]
    fn next(&self, n: &Node45, k: usize) -> Node45 {
        let w = self.s.m4.len();
        Node45 {
            n: Step4 { s: self.s, s5: self.s5, free: false }.next(&n.n, k),
            xo: self.s.mv4[5][n.xo as usize * w + k],
            odd: n.odd ^ flips_outer(self.s.m4[k]),
        }
    }

    #[inline(always)]
    fn bound(&self, n: &Node45) -> usize {
        let inner = Step4 { s: self.s, s5: self.s5, free: false }.bound(&n.n);
        inner.max(self.s5.b_dist(n.xo as usize, n.n.o as usize, n.odd) as usize)
    }

    /// The tables are exact, so a bound of 0 is the goal.
    fn done(&self, _: &Node45) -> bool {
        true
    }

    #[inline(always)]
    fn prefetch(&self, n: &Node45) {
        Step4 { s: self.s, s5: self.s5, free: false }.prefetch(&n.n);
        self.s5.b_fetch(n.xo as usize, n.n.o as usize, n.odd);
    }
}

/// Steps 1 to 3 in one: the R/L masks of the inner x and inner t centres,
/// of the three orbits of the triples and of the outer x centres.
struct Step123<'a> {
    s: &'a Stage7,
    s5: &'a Stage5,
}

impl Problem for Step123<'_> {
    type Node = [u32; 6];

    fn moves(&self) -> &[u8] {
        &self.s.m1
    }

    #[inline(always)]
    fn next(&self, n: &[u32; 6], k: usize) -> [u32; 6] {
        let (m, p) = (k as u8, &self.s.perms);
        [
            p[X_IN].apply(m, n[0]),
            p[T_IN].apply(m, n[1]),
            p[OBL_A].apply(m, n[2]),
            p[T_OUT].apply(m, n[3]),
            p[OBL_B].apply(m, n[4]),
            p[X_OUT].apply(m, n[5]),
        ]
    }

    #[inline(always)]
    fn bound(&self, n: &[u32; 6]) -> usize {
        let d = &self.s.diff1;
        let triples = d[0][(n[2] ^ n[4]) as usize].max(d[1][(n[2] ^ n[3]) as usize]).max(d[2][(n[3] ^ n[4]) as usize]);
        triples.max(self.s5.a_close(n[0], n[1])).max(self.s5.a_close(n[5], n[3])) as usize
    }

    fn done(&self, n: &[u32; 6]) -> bool {
        n[0] == GOAL_RL && n[1] == GOAL_RL && n[2] == GOAL_RL && n[3] == GOAL_RL && n[4] == GOAL_RL && n[5] == GOAL_RL
    }

    #[inline(always)]
    fn prefetch(&self, n: &[u32; 6]) {
        let d = &self.s.diff1;
        self.s5.a_fetch(n[0], n[1]);
        self.s5.a_fetch(n[5], n[3]);
        fetch(&d[1], (n[2] ^ n[3]) as usize);
        fetch(&d[2], (n[3] ^ n[4]) as usize);
    }
}

/// Steps 1 and 2 in one: the R/L masks of the inner x and inner t centres
/// and of the three orbits of the triples.
struct Step12<'a> {
    s: &'a Stage7,
    s5: &'a Stage5,
}

impl Problem for Step12<'_> {
    type Node = [u32; 5];

    fn moves(&self) -> &[u8] {
        &self.s.m1
    }

    #[inline(always)]
    fn next(&self, n: &[u32; 5], k: usize) -> [u32; 5] {
        let (m, p) = (k as u8, &self.s.perms);
        [p[X_IN].apply(m, n[0]), p[T_IN].apply(m, n[1]), p[OBL_A].apply(m, n[2]), p[T_OUT].apply(m, n[3]), p[OBL_B].apply(m, n[4])]
    }

    #[inline(always)]
    fn bound(&self, n: &[u32; 5]) -> usize {
        let d = &self.s.diff1;
        let triples = d[0][(n[2] ^ n[4]) as usize].max(d[1][(n[2] ^ n[3]) as usize]).max(d[2][(n[3] ^ n[4]) as usize]);
        triples.max(self.s5.a_close(n[0], n[1])) as usize
    }

    fn done(&self, n: &[u32; 5]) -> bool {
        n[0] == GOAL_RL && n[1] == GOAL_RL && n[2] == n[3] && n[3] == n[4]
    }

    #[inline(always)]
    fn prefetch(&self, n: &[u32; 5]) {
        let d = &self.s.diff1;
        self.s5.a_fetch(n[0], n[1]);
        fetch(&d[0], (n[2] ^ n[4]) as usize);
        fetch(&d[1], (n[2] ^ n[3]) as usize);
        fetch(&d[2], (n[3] ^ n[4]) as usize);
    }
}

/// Step 4: the U/D patterns of the inner x and inner t centres, the parity
/// of the inner wings, and the U/D patterns of the three orbits of the triples.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Node4 {
    x: u16,
    t: u16,
    odd: bool,
    a: u16,
    o: u16,
    b: u16,
}

struct Step4<'a> {
    s: &'a Stage7,
    s5: &'a Stage5,
    /// The parity of the inner wings is left as it comes (larger cubes:
    /// the turns of their later steps change it again anyway).
    free: bool,
}

impl Problem for Step4<'_> {
    type Node = Node4;

    fn moves(&self) -> &[u8] {
        &self.s.m4
    }

    #[inline(always)]
    fn next(&self, n: &Node4, k: usize) -> Node4 {
        let (mv, w) = (&self.s.mv4, self.s.m4.len());
        Node4 {
            x: mv[0][n.x as usize * w + k],
            t: mv[1][n.t as usize * w + k],
            odd: n.odd ^ flips_inner(self.s.m4[k]),
            a: mv[2][n.a as usize * w + k],
            o: mv[3][n.o as usize * w + k],
            b: mv[4][n.b as usize * w + k],
        }
    }

    #[inline(always)]
    fn bound(&self, n: &Node4) -> usize {
        let (a, o, b) = (n.a as usize, n.o as usize, n.b as usize);
        let p = &self.s.pair4;
        self.s
            .inner4(self.s5, n, self.free)
            .max(nibble(&p[0], a * N_UD + b))
            .max(nibble(&p[1], a * N_UD + o))
            .max(nibble(&p[2], o * N_UD + b)) as usize
    }

    /// The tables are exact, so a bound of 0 is the goal.
    fn done(&self, _: &Node4) -> bool {
        true
    }

    #[inline(always)]
    fn prefetch(&self, n: &Node4) {
        let (a, o, b) = (n.a as usize, n.o as usize, n.b as usize);
        let p = &self.s.pair4;
        self.s5.b_fetch(n.x as usize, n.t as usize, n.odd);
        fetch(&p[0], (a * N_UD + b) >> 1);
        fetch(&p[1], (a * N_UD + o) >> 1);
        fetch(&p[2], (o * N_UD + b) >> 1);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_and_outer_layers_move_like_5x5x5_cubes() {
        let (g7, g5) = (Geometry::new(7), Geometry::new(5));
        let mut seed = 777u64;
        let mut rnd = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        // The inner cube under turns of one and three layers, and under
        // turns of two layers, which are face turns to it.
        let (mut f, mut inner) = (g7.solved(), g5.solved());
        for _ in 0..200 {
            let m = rnd(36) as u8;
            g7.apply(&mut f, from_inner(m) as usize);
            g5.apply(&mut inner, m as usize);
            assert_eq!(g7.sub_facelets(&f, &INNER5), inner);
        }
        for _ in 0..50 {
            let m = rnd(18);
            g7.apply(&mut f, m + 18);
            g5.apply(&mut inner, m);
            assert_eq!(g7.sub_facelets(&f, &INNER5), inner);
        }
        // The outer cube under turns of one and two layers.
        let (mut f, mut outer) = (g7.solved(), g5.solved());
        for _ in 0..200 {
            let m = rnd(36);
            g7.apply(&mut f, m);
            g5.apply(&mut outer, m);
            assert_eq!(g7.sub_facelets(&f, &OUTER5), outer);
        }
    }

    #[test]
    fn step_moves_keep_what_they_should() {
        let g = Geometry::new(7);
        let orbits = centre_orbits(&g);
        let side = |f: usize| f != 1 && f != 4;
        let on_rl = |o: usize, m: u8| (16..24).all(|i| orbits[o].src[m as usize][i] >= 16);
        // Step 2 keeps the inner centres of R and L on R and L.
        for m in moves_where(|f, layers, power| layers < 3 || !side(f) || power == 1) {
            assert!(on_rl(X_IN, m) && on_rl(T_IN, m));
        }
        // Step 4 keeps every centre of R and L on R and L.
        let m4 = moves_where(|f, layers, power| match layers {
            1 => side(f),
            _ => !side(f) || power == 1,
        });
        for m in m4 {
            assert!((0..6).all(|o| on_rl(o, m)));
        }
    }
}
