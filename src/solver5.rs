//! 5x5x5, the whole solve: the centres are staged, the edges are reduced in
//! three phases, and what is left is solved as a 3x3x3.
//!
//! The centres are staged in two phases. Staging them in one step with all
//! moves is shorter but takes minutes; it runs only on request.
//!
//! Every step keeps many results, and the next one continues the ones that
//! are shortest in total. The edge phases also look at the cube from six
//! sides: any of the three axes can be the one the wings are split along,
//! and either of the other two can be the one whose faces turn freely while
//! the first four edges are built.

use crate::two3::Two3;
use crate::c3::C3;
use crate::c5::{tables5, Cube5};
use crate::centres5::{Budget, Centres5};
use crate::edges5::{is_reduced, Edges5, Limits, Start};
use crate::stage5::{Stage5, Staged};
use rayon::prelude::*;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The longest first phase of the quick 3x3x3 solve that every reduced cube gets.
pub(crate) const QUICK: usize = 10;

/// How wide the search goes.
#[derive(Clone, Debug)]
pub struct Params5 {
    /// Centre stagings kept, and how many moves past the shortest first
    /// phase they may come from.
    pub staged: usize,
    pub stage_extra: usize,
    /// Positions the first centre phase may visit at each length past its
    /// shortest and the next; 0 for no limit. A length that does not fit
    /// is searched in part.
    pub stage_budget: u64,
    /// Stage the centres in one step as well, searching this many lengths
    /// past the shortest staging. Exact but takes minutes; off in every mode.
    pub joint_extra: Option<usize>,
    /// Labellings (which wing of each pair goes to which half) tried per staging.
    pub labellings: usize,
    /// Results each edge phase aims for before the next one starts.
    pub c_keep: usize,
    pub p_keep: usize,
    pub e_keep: usize,
    /// Reduced cubes whose 3x3x3 gets the full effort, after a quick solve
    /// of all of them, and that effort: the longest first phase of the
    /// two-phase search that is tried. Then, step by step, a longer first
    /// phase for fewer of them: (length, how many).
    pub pool: usize,
    pub depth: usize,
    pub deeper: Vec<(usize, usize)>,
    /// Time the solve aims at, in milliseconds; 0 for no limit. Each step
    /// stops widening when its share is used up, but always finishes.
    pub time_ms: u64,
}

impl Params5 {
    /// Under a second per cube.
    pub fn fast() -> Params5 {
        Params5 {
            staged: 400,
            stage_extra: 2,
            stage_budget: 200_000,
            joint_extra: None,
            labellings: 8,
            c_keep: 2000,
            p_keep: 300,
            e_keep: 50,
            pool: 30,
            depth: 11,
            deeper: Vec::new(),
            time_ms: 800,
        }
    }

    /// Under five seconds per cube.
    pub fn normal() -> Params5 {
        Params5 {
            staged: 1200,
            stage_extra: 3,
            stage_budget: 1_000_000,
            joint_extra: None,
            labellings: 16,
            c_keep: 6000,
            p_keep: 700,
            e_keep: 100,
            pool: 100,
            depth: 11,
            deeper: vec![(12, 30), (13, 8)],
            time_ms: 4000,
        }
    }

    /// As long as it takes.
    pub fn full() -> Params5 {
        Params5 {
            staged: 4000,
            stage_extra: 3,
            stage_budget: 10_000_000,
            joint_extra: None,
            labellings: 16,
            c_keep: 20000,
            p_keep: 2000,
            e_keep: 400,
            pool: 400,
            depth: 11,
            deeper: vec![(12, 100), (13, 24), (14, 4)],
            time_ms: 0,
        }
    }
}

impl Params5 {
    /// The steps of the 3x3x3 after the quick solve: (length of first phase, how many cubes).
    pub fn steps(&self) -> Vec<(usize, usize)> {
        std::iter::once((self.depth, self.pool)).chain(self.deeper.iter().copied()).collect()
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
    /// Which of the cubes given this continues.
    root: usize,
}

pub struct Solver5 {
    pub stage: Stage5,
    centres: OnceLock<Centres5>,
    pub edges: Edges5,
    /// The 3x3x3 search that finishes a reduced cube.
    pub two: Two3,
    frames: Vec<Frame>,
}

impl Solver5 {
    pub fn load() -> Solver5 {
        let stage = Stage5::load();
        let edges = Edges5::load();
        let two = Two3::load(&edges.t3);
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
        Solver5 { stage, centres: OnceLock::new(), edges, two, frames }
    }

    /// Solves a cube given as 150 facelets (faces U R F D L B, row by row).
    pub fn solve(&self, facelets: &[u8], p: &Params5) -> Result<Solution5, String> {
        self.solve_any(&[(facelets.to_vec(), 0)], p).map(|x| x.1)
    }

    /// Solves whichever of several cubes gives the shortest total, each
    /// given as 150 facelets and the number of moves already spent on it:
    /// (which cube, its solution). For callers that reach a 5x5x5 in many
    /// ways, such as the larger cubes: every step here chooses among all of
    /// them by total length.
    pub fn solve_any(&self, cubes: &[(Vec<u8>, usize)], p: &Params5) -> Result<(usize, Solution5), String> {
        self.run(cubes, p, true)
    }

    /// The shortest reduction found: centres solved and edges whole, the
    /// 3x3x3 left as it comes. For callers that go on from there themselves.
    pub fn reduce(&self, facelets: &[u8], p: &Params5) -> Result<Vec<u8>, String> {
        self.run(&[(facelets.to_vec(), 0)], p, false).map(|x| x.1.moves)
    }

    fn run(&self, cubes: &[(Vec<u8>, usize)], p: &Params5, finish: bool) -> Result<(usize, Solution5), String> {
        let g = &tables5().geometry;
        let t0 = Instant::now();
        let until = |share: f64| (p.time_ms > 0).then(|| t0 + Duration::from_secs_f64(p.time_ms as f64 * share / 1e3));
        let roots: Vec<Vec<Cube5>> = cubes
            .iter()
            .map(|(facelets, _)| {
                self.frames.iter().map(|f| Cube5::from_facelets(&g.reframe(facelets, &f.turns))).collect::<Result<_, _>>()
            })
            .collect::<Result<_, String>>()?;
        let start_in = |root: usize, frame: usize, moves: &[u8]| -> Start {
            let f = &self.frames[frame];
            let mut cube = roots[root][frame];
            let mut last = 255;
            for &m in moves {
                last = f.from_orig[m as usize];
                cube.apply(last);
            }
            Start { cube, cost: cubes[root].1 + moves.len(), last }
        };
        let limits = |keep: usize, per_node: usize, share: f64| Limits {
            keep,
            cap: keep * 8,
            per_node,
            deadline: until(share),
            // Even when late, leave the next step something to choose from.
            floor: (keep / 8).clamp(4, 64),
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
                    Partial { moves, frame: base.frame, ends, root: base.root }
                })
                .collect()
        };
        let starts_of =
            |from: &[Partial]| -> Vec<Start> { from.par_iter().map(|x| start_in(x.root, x.frame, &x.moves)).collect() };

        let trace = std::env::var_os("CUBE5_TRACE").is_some();
        let mut clock = (Instant::now(), crate::clock::cpu_ms());
        let mut lap = |what: &str, n: usize, best: usize| {
            if trace {
                let (wall, cpu) = (clock.0.elapsed().as_secs_f64() * 1e3, crate::clock::cpu_ms() - clock.1);
                eprintln!("  {:8} {:6} results, shortest {:2}, {:5.0} ms, {:6.0} ms of processor", what, n, best, wall, cpu);
            }
            clock = (Instant::now(), crate::clock::cpu_ms());
        };
        let shortest = |x: &[Partial]| x.iter().map(|p| p.moves.len()).min().unwrap_or(0);
        // Centres, of every cube: (cube, staging).
        let per_cube = (p.staged / cubes.len()).max(8);
        let mut staged: Vec<(usize, Staged)> = Vec::new();
        for (root, (facelets, _)) in cubes.iter().enumerate() {
            let found = self.stage.stage(facelets, p.stage_extra, 100_000, per_cube, p.stage_budget, until(0.15))?;
            staged.extend(found.into_iter().map(|s| (root, s)));
        }
        lap("centres", staged.len(), staged.iter().map(|s| s.1.moves.len()).min().unwrap_or(0));
        if let (Some(extra), 1) = (p.joint_extra, cubes.len()) {
            // In one step: only stagings no longer than the two-phase ones are of use.
            let budget = Budget {
                max_len: staged.first().map_or(30, |s| s.1.moves.len()),
                extra,
                cap: p.staged * 4,
                deadline: None,
            };
            let joint = self.centres.get_or_init(Centres5::load).stage(&roots[0][0], &budget);
            lap("in one", joint.len(), joint.first().map_or(0, |s| s.len()));
            let mut all: Vec<(usize, Staged)> =
                joint.into_iter().map(|moves| (0, Staged { len_a: moves.len(), moves })).collect();
            all.append(&mut staged);
            all.sort_by_key(|s| s.1.moves.len());
            all.truncate(p.staged);
            staged = all;
        }
        // Phase C, along each axis.
        let from: Vec<Partial> = staged
            .iter()
            .flat_map(|(root, s)| {
                (0..3).map(move |axis| Partial {
                    moves: s.moves.clone(),
                    frame: 2 * axis,
                    ends: vec![s.len_a, s.moves.len()],
                    root: *root,
                })
            })
            .collect();
        // Phase P needs four edge classes with no piece in its slice. The
        // shortest results of phase C can all be alike and offer none; then
        // phase C is asked for more.
        let mut c_keep = p.c_keep;
        let (after_c, after_p) = loop {
            let found = self.edges.phase_c(&starts_of(&from), p.labellings, &limits(c_keep, 10, 0.4));
            let after_c = extend(&from, found);
            lap("phase C", after_c.len(), shortest(&after_c));
            // Phase P, with either of the two other axes turning freely.
            let from_p: Vec<Partial> = after_c
                .iter()
                .flat_map(|x| {
                    (0..2).map(move |quarter| Partial {
                        moves: x.moves.clone(),
                        frame: x.frame + quarter,
                        ends: x.ends.clone(),
                        root: x.root,
                    })
                })
                .collect();
            let found = self.edges.phase_p(&starts_of(&from_p), &limits(p.p_keep, 10, 0.7));
            let after_p = extend(&from_p, found);
            lap("phase P", after_p.len(), shortest(&after_p));
            if !after_p.is_empty() || after_c.is_empty() || c_keep >= p.c_keep * 512 {
                break (after_c, after_p);
            }
            c_keep *= 8;
        };
        let _ = after_c;
        // Phase E.
        let found = self.edges.phase_e(&starts_of(&after_p), &limits(p.e_keep, 2, 0.9));
        let reduced = extend(&after_p, found);
        lap("phase E", reduced.len(), shortest(&reduced));
        if !finish {
            return match reduced.iter().min_by_key(|x| cubes[x.root].1 + x.moves.len()) {
                Some(x) => {
                    let e = &x.ends;
                    let lens = [e[0], e[1] - e[0], e[2] - e[1], e[3] - e[2], e[4] - e[3], 0];
                    Ok((x.root, Solution5 { moves: x.moves.clone(), lens }))
                }
                None if p.time_ms == 0 => Err("no reduction found".to_string()),
                None => self.run(cubes, &Params5 { time_ms: 0, ..p.clone() }, finish),
            };
        }
        // The 3x3x3 that is left.
        // A quick solve of every one, then the full effort on the most promising.
        let left: Vec<(usize, C3)> = reduced
            .par_iter()
            .map(|x| {
                let cube = start_in(x.root, x.frame, &x.moves).cube;
                debug_assert!(is_reduced(&cube));
                (cubes[x.root].1 + x.moves.len(), C3 { cp: cube.c4.cp, co: cube.c4.co, ep: cube.mid.ep, eo: cube.mid.eo })
            })
            .collect();
        let found = self.two.finish(&self.edges.t3, &left, QUICK, &p.steps(), None);
        if trace {
            let mut by: std::collections::BTreeMap<(usize, usize), usize> = std::collections::BTreeMap::new();
            for (k, sol) in found.iter().enumerate() {
                *by.entry((left[k].0, sol.len())).or_insert(0) += 1;
            }
            let roots: std::collections::BTreeSet<usize> = reduced.iter().map(|x| x.root).collect();
            eprintln!("  reductions from {} cubes; (moves before the 3x3x3, 3x3x3) -> count: {:?}", roots.len(), by);
        }
        let best = (0..reduced.len())
            .min_by_key(|&k| (left[k].0 + found[k].len(), k))
            .map(|k| (left[k].0 + found[k].len(), &reduced[k], found[k].clone()));
        let Some((_, x, sol3)) = best else {
            // A step cut short by the clock left the next one nothing to work with.
            return match p.time_ms {
                0 => Err("no reduction found".to_string()),
                _ => self.run(cubes, &Params5 { time_ms: 0, ..p.clone() }, finish),
            };
        };
        lap("3x3x3", 1, cubes[x.root].1 + x.moves.len() + sol3.len());
        let to_orig = &self.frames[x.frame].to_orig;
        let moves: Vec<u8> = x.moves.iter().copied().chain(sol3.iter().map(|&m| to_orig[m as usize])).collect();
        let e = &x.ends;
        let lens = [e[0], e[1] - e[0], e[2] - e[1], e[3] - e[2], e[4] - e[3], sol3.len()];
        Ok((x.root, Solution5 { moves, lens }))
    }
}
