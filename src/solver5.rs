//! 5x5x5, the whole solve: the centres are staged, the edges are reduced in
//! three phases, and what is left is solved as a 3x3x3.
//!
//! Every step keeps many results, and the next one continues the ones that
//! are shortest in total. The edge phases also look at the cube from six
//! sides: any of the three axes can be the one the wings are split along,
//! and either of the other two can be the one whose faces turn freely while
//! the first four edges are built.

use crate::big1::Big1;
use crate::c3::C3;
use crate::c5::{tables5, Cube5};
use crate::edges5::{is_reduced, Edges5, Limits, Start};
use crate::stage5::Stage5;
use rayon::prelude::*;
use std::time::{Duration, Instant};

/// How wide the search goes.
#[derive(Clone, Debug)]
pub struct Params5 {
    /// Centre stagings kept, and how many moves past the shortest first
    /// phase they may come from.
    pub staged: usize,
    pub stage_extra: usize,
    /// Labellings (which wing of each pair goes to which half) tried per staging.
    pub labellings: usize,
    /// Results each edge phase aims for before the next one starts.
    pub c_keep: usize,
    pub p_keep: usize,
    pub e_keep: usize,
    /// Effort of the 3x3x3 solver on each reduced cube.
    pub probes: u64,
    /// Time the solve aims at, in milliseconds; 0 for no limit. Each step
    /// stops widening when its share is used up, but always finishes.
    pub time_ms: u64,
}

impl Params5 {
    /// Under a second per cube.
    pub fn fast() -> Params5 {
        Params5 { staged: 100, stage_extra: 0, labellings: 4, c_keep: 100, p_keep: 100, e_keep: 8, probes: 200, time_ms: 700 }
    }

    /// Under five seconds per cube.
    pub fn normal() -> Params5 {
        Params5 { staged: 400, stage_extra: 1, labellings: 8, c_keep: 500, p_keep: 500, e_keep: 40, probes: 3000, time_ms: 4000 }
    }

    /// As long as it takes.
    pub fn full() -> Params5 {
        Params5 {
            staged: 2000,
            stage_extra: 1,
            labellings: 16,
            c_keep: 5000,
            p_keep: 5000,
            e_keep: 400,
            probes: 20000,
            time_ms: 0,
        }
    }
}

impl Default for Params5 {
    fn default() -> Params5 {
        Params5::normal()
    }
}

/// A solution in the moves of the 5x5x5 (`face * 3 + power`, wide faces from 18).
pub struct Solution5 {
    pub moves: Vec<u8>,
    /// Moves per step: both centre phases, the three edge phases, the 3x3x3.
    pub lens: [usize; 6],
}

/// The cube seen from another side.
struct Frame {
    turns: Vec<usize>,
    /// Move of the original cube for each move of this frame, and back.
    to_orig: Vec<u8>,
    from_orig: Vec<u8>,
}

/// A solution so far, in the moves of the original cube.
struct Partial {
    moves: Vec<u8>,
    frame: usize,
    /// Length after each finished step.
    ends: Vec<usize>,
}

pub struct Solver5 {
    pub stage: Stage5,
    pub edges: Edges5,
    pub big: Big1,
    frames: Vec<Frame>,
}

impl Solver5 {
    pub fn load() -> Solver5 {
        let stage = Stage5::load();
        let edges = Edges5::load();
        let big = Big1::load(&edges.t3);
        let g = &tables5().geometry;
        // Three choices of the axis the wings are split along, each with the
        // cube as it is or turned a quarter about that axis.
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
        Solver5 { stage, edges, big, frames }
    }

    /// Solves a cube given as 150 facelets (faces U R F D L B, row by row).
    pub fn solve(&self, facelets: &[u8], p: &Params5) -> Result<Solution5, String> {
        let g = &tables5().geometry;
        let t0 = Instant::now();
        let until = |share: f64| (p.time_ms > 0).then(|| t0 + Duration::from_secs_f64(p.time_ms as f64 * share / 1e3));
        let roots: Vec<Cube5> = self
            .frames
            .iter()
            .map(|f| Cube5::from_facelets(&g.reframe(facelets, &f.turns)))
            .collect::<Result<_, _>>()?;
        let start_in = |frame: usize, moves: &[u8]| -> Start {
            let f = &self.frames[frame];
            let mut cube = roots[frame];
            let mut last = 255;
            for &m in moves {
                last = f.from_orig[m as usize];
                cube.apply(last);
            }
            Start { cube, cost: moves.len(), last }
        };
        let limits = |keep: usize, per_node: usize, share: f64| Limits {
            keep,
            cap: keep * 8,
            per_node,
            deadline: until(share),
        };
        // Continues each start's partial solution with what a phase found.
        let extend = |from: &[Partial], found: Vec<crate::edges5::Found>| -> Vec<Partial> {
            found
                .into_iter()
                .map(|f| {
                    let base = &from[f.start];
                    let to_orig = &self.frames[base.frame].to_orig;
                    let moves: Vec<u8> =
                        base.moves.iter().copied().chain(f.moves.iter().map(|&m| to_orig[m as usize])).collect();
                    let mut ends = base.ends.clone();
                    ends.push(moves.len());
                    Partial { moves, frame: base.frame, ends }
                })
                .collect()
        };
        let starts_of = |from: &[Partial]| -> Vec<Start> { from.par_iter().map(|x| start_in(x.frame, &x.moves)).collect() };

        let trace = std::env::var_os("CUBE5_TRACE").is_some();
        let mut clock = Instant::now();
        let mut lap = |what: &str, n: usize, best: usize| {
            if trace {
                eprintln!("  {:8} {:6} results, shortest {:2}, {:5.0} ms", what, n, best, clock.elapsed().as_secs_f64() * 1e3);
            }
            clock = Instant::now();
        };
        let shortest = |x: &[Partial]| x.iter().map(|p| p.moves.len()).min().unwrap_or(0);
        // Centres.
        let staged = self.stage.stage(facelets, p.stage_extra, 100_000, p.staged, until(0.08))?;
        lap("centres", staged.len(), staged.first().map_or(0, |s| s.moves.len()));
        // Phase C, along each axis.
        let from: Vec<Partial> = staged
            .iter()
            .flat_map(|s| {
                (0..3).map(move |axis| Partial { moves: s.moves.clone(), frame: 2 * axis, ends: vec![s.len_a, s.moves.len()] })
            })
            .collect();
        let found = self.edges.phase_c(&starts_of(&from), p.labellings, &limits(p.c_keep, 10, 0.2));
        let after_c = extend(&from, found);
        lap("phase C", after_c.len(), shortest(&after_c));
        // Phase P, with either of the two other axes turning freely.
        let from: Vec<Partial> = after_c
            .iter()
            .flat_map(|x| {
                (0..2).map(move |quarter| Partial { moves: x.moves.clone(), frame: x.frame + quarter, ends: x.ends.clone() })
            })
            .collect();
        let found = self.edges.phase_p(&starts_of(&from), &limits(p.p_keep, 10, 0.6));
        let after_p = extend(&from, found);
        lap("phase P", after_p.len(), shortest(&after_p));
        // Phase E.
        let found = self.edges.phase_e(&starts_of(&after_p), &limits(p.e_keep, 2, 0.85));
        let reduced = extend(&after_p, found);
        lap("phase E", reduced.len(), shortest(&reduced));
        // The 3x3x3 that is left.
        let best = reduced
            .par_iter()
            .map(|x| {
                let cube = start_in(x.frame, &x.moves).cube;
                debug_assert!(is_reduced(&cube));
                let c3 = C3 { cp: cube.c4.cp, co: cube.c4.co, ep: cube.mid.ep, eo: cube.mid.eo };
                let sol = self.edges.t3.solve(&c3, p.probes, Some(&self.big));
                (x.moves.len() + sol.len(), x, sol)
            })
            .min_by_key(|r| r.0)
            .ok_or("no reduction found")?;
        let (_, x, sol3) = best;
        lap("3x3x3", 1, x.moves.len() + sol3.len());
        let to_orig = &self.frames[x.frame].to_orig;
        let moves: Vec<u8> = x.moves.iter().copied().chain(sol3.iter().map(|&m| to_orig[m as usize])).collect();
        let e = &x.ends;
        let lens = [e[0], e[1] - e[0], e[2] - e[1], e[3] - e[2], e[4] - e[3], sol3.len()];
        Ok(Solution5 { moves, lens })
    }
}
