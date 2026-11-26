//! The solution a robot makes quickest: candidates are compared by the
//! robot's own estimate of their time (its planner run on its time model),
//! not by their number of moves.
//!
//! On a robot like MultiCuber a turn of the top or bottom face costs a lift
//! move and a rotator turn, while bringing a side face to the rotator costs
//! more than a second of lift and flipper motions, and a half turn hardly
//! more than a quarter. So which of the many solutions of about the same
//! length is taken, and on which axis it works, changes the robot's time a
//! lot.
//!
//! 3x3x3: the two-phase search goes on past its first solution and keeps
//! every solution within two moves of the shortest it has met (all ways into
//! the domino subgroup on the three axes, for the cube and its inverse,
//! shortest first), for a few milliseconds. Each also counts in the orders
//! of its pairs of opposite-face turns (U D or D U), which the robot makes
//! differently. Searching the 24 orientations of the cube on top of that
//! would add nothing: the axes and the inverse already give every way into
//! the subgroup of each length, which is what the orientations would change.
//!
//! Other sizes: the solver is run on the cube seen from each of the 24
//! orientations, as long as the time allows, and the quickest is taken.

use super::{PlanMode, Turn};
use crate::api::check::{ApiError, Checked};
use crate::api::solve::{solve, Mode};
use crate::cube::Cube;
use crate::cuben::Geometry;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Options {
    /// 3x3x3: how long the candidate search may run (ms).
    pub search_ms: f64,
    /// 3x3x3: no candidate is costed after this (ms from the start; the
    /// shortest solutions are costed first).
    pub total_ms: f64,
    /// 3x3x3: at most this many solutions from the search.
    pub max_solutions: usize,
    /// 3x3x3: moves above the shortest that a candidate may have.
    pub slack: usize,
    /// 3x3x3: the longest first phase followed.
    pub depth: usize,
    /// 3x3x3: second phases of the shortest length kept per first phase.
    pub phase2: usize,
    /// Other sizes: the time (ms) after which no further orientation is started.
    pub orientation_ms: f64,
    /// The solver's mode for other sizes.
    pub mode: Mode,
    /// How the robot plans: with the optimizing planner, the best few by
    /// the legacy plan are planned again with it.
    pub plan: PlanMode,
    pub optimize_best: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            search_ms: 15.0,
            total_ms: 25.0,
            max_solutions: 2000,
            slack: 2,
            depth: 14,
            phase2: 4,
            orientation_ms: 2000.0,
            mode: Mode::Normal,
            plan: PlanMode::Legacy,
            optimize_best: 6,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Choice {
    pub moves: Vec<Turn>,
    /// The robot's estimate of its time (ms), under the plan mode.
    pub est_ms: f64,
    /// Move sequences whose robot time was computed.
    pub candidates: usize,
    /// Solutions the solver gave (before the orders of opposite turns).
    pub solutions: usize,
    pub compute_ms: f64,
}

impl Choice {
    pub fn names(&self) -> Vec<String> {
        self.moves.iter().map(Turn::name).collect()
    }
}

/// The robot's time for a sequence under a plan mode.
pub type Cost<'a> = dyn Fn(&[Turn], PlanMode) -> f64 + 'a;

/// The 24 ways to hold a cube, as whole-cube quarter turns (0 = x, 1 = y,
/// 2 = z) in the sense of `Geometry::turned`; the first is none.
pub fn orientations() -> Vec<Vec<usize>> {
    // The rotations do not depend on the size: told apart on a 3x3x3's stickers.
    let g = Geometry::new(3);
    let ids: Vec<u8> = (0..54).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut queue = vec![Vec::new()];
    let mut at = 0;
    while at < queue.len() {
        let turns: Vec<usize> = queue[at].clone();
        at += 1;
        if !seen.insert(g.turned(&ids, &turns)) {
            continue;
        }
        out.push(turns.clone());
        // Some rotations take four quarter turns in one sense (x' y' is x x x y y y, or shorter otherwise).
        if turns.len() < 4 && out.len() < 24 {
            for t in 0..3 {
                queue.push(turns.iter().copied().chain([t]).collect());
            }
        }
    }
    out
}

/// For the cube held as `turns` (`Geometry::turned`): `out[m]` is the move
/// of the cube as it was that does what move `m` does to the cube held so.
/// Found on the stickers, so that it holds whatever order the turns compose in.
pub fn move_map(g: &Geometry, turns: &[usize]) -> Vec<usize> {
    let total = 6 * g.n * g.n;
    // Stickers numbered in base 256 over two arrays, to stay in u8.
    let lo: Vec<u8> = (0..total).map(|i| (i % 256) as u8).collect();
    let hi: Vec<u8> = (0..total).map(|i| (i / 256) as u8).collect();
    let held = (g.turned(&lo, turns), g.turned(&hi, turns));
    let mut out = Vec::with_capacity(g.move_count());
    for m in 0..g.move_count() {
        let mut want = held.clone();
        g.apply(&mut want.0, m);
        g.apply(&mut want.1, m);
        let found = (0..g.move_count()).find(|&k| {
            let (mut a, mut b) = (lo.clone(), hi.clone());
            g.apply(&mut a, k);
            g.apply(&mut b, k);
            g.turned(&a, turns) == want.0 && g.turned(&b, turns) == want.1
        });
        out.push(found.expect("a turned cube's move is a move of the cube"));
    }
    out
}

/// Every order of the neighbouring opposite-face turns of a sequence (at most `cap`).
pub fn orders(seq: &[usize], cap: usize) -> Vec<Vec<usize>> {
    let axis = |m: usize| m / 3 % 3;
    let face = |m: usize| m / 3 % 6;
    let mut pairs = Vec::new();
    let mut i = 0;
    while i + 1 < seq.len() {
        if axis(seq[i]) == axis(seq[i + 1]) && face(seq[i]) != face(seq[i + 1]) && seq[i] / 18 == seq[i + 1] / 18 {
            pairs.push(i);
            i += 2;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    let k = pairs.len().min(usize::BITS as usize - 1);
    for mask in 0..(1usize << k).min(cap.max(1)) {
        let mut s = seq.to_vec();
        for (b, &p) in pairs.iter().enumerate().take(k) {
            if mask >> b & 1 == 1 {
                s.swap(p, p + 1);
            }
        }
        out.push(s);
    }
    out
}

/// Keeps the `k` cheapest.
struct Best {
    k: usize,
    items: Vec<(f64, Vec<Turn>)>,
}

impl Best {
    fn offer(&mut self, ms: f64, moves: &[Turn]) {
        if self.items.len() == self.k && ms >= self.items[self.k - 1].0 {
            return;
        }
        let at = self.items.partition_point(|(c, _)| *c <= ms);
        self.items.insert(at, (ms, moves.to_vec()));
        self.items.truncate(self.k);
    }
}

fn verify(g: &Geometry, facelets: &[u8], moves: &[usize]) -> bool {
    let mut f = facelets.to_vec();
    g.apply_seq(&mut f, moves);
    g.is_solved(&f)
}

fn layers_index(t: &Turn) -> Option<usize> {
    match *t {
        Turn::Layers { face, depth, turns } => Some((face as usize + 6 * (depth as usize - 1)) * 3 + match turns {
            1 => 0,
            2 => 1,
            _ => 2,
        }),
        Turn::Whole { .. } => None,
    }
}

/// The quickest solution of a checked cube for a robot with time `cost`.
pub fn choose(c: &Checked, cost: &Cost, opts: &Options) -> Result<Choice, ApiError> {
    let t0 = Instant::now();
    let g = if c.size == 2 { Geometry::new(2) } else { Geometry::new(c.size) };
    if g.is_solved(&c.facelets) {
        return Ok(Choice { moves: Vec::new(), est_ms: cost(&[], opts.plan), candidates: 1, solutions: 1, compute_ms: 0.0 });
    }
    let keep = if opts.plan == PlanMode::Optimized { opts.optimize_best.max(1) } else { 1 };
    let mut best = Best { k: keep, items: Vec::new() };
    let mut candidates = 0;
    let solutions;
    if c.size == 3 {
        let c3 = Cube::from_small_facelets(&g.to_string(&c.facelets))
            .ok()
            .and_then(|x| x.with_home_centres())
            .and_then(|x| crate::c3::C3::from_cube(&x))
            .ok_or_else(|| ApiError::new("solver_failed", "not a 3x3x3 state"))?;
        let deadline = t0 + std::time::Duration::from_secs_f64(opts.search_ms.max(0.0) / 1000.0);
        let found = crate::api::solve::two3().candidates(
            crate::api::solve::t3(),
            &c3,
            crate::two3::Candidates { depth: opts.depth, slack: opts.slack, max: opts.max_solutions, deadline: Some(deadline), phase2: opts.phase2 },
        );
        solutions = found.len();
        let total = std::time::Duration::from_secs_f64(opts.total_ms.max(opts.search_ms).max(0.0) / 1000.0);
        for (k, sol) in found.iter().enumerate() {
            if k > 0 && k % 32 == 0 && t0.elapsed() > total {
                break;
            }
            let seq: Vec<usize> = sol.iter().map(|&m| m as usize).collect();
            for s in orders(&seq, 16) {
                let turns: Vec<Turn> = s.iter().map(|&m| Turn::from_index(m)).collect();
                candidates += 1;
                best.offer(cost(&turns, PlanMode::Legacy), &turns);
            }
        }
    } else {
        let mut count = 0;
        for (i, turns) in orientations().iter().enumerate() {
            if i > 0 && t0.elapsed().as_secs_f64() * 1000.0 > opts.orientation_ms {
                break;
            }
            let seen = if c.size % 2 == 1 { g.reframe(&c.facelets, turns) } else { g.held(&c.facelets, turns) };
            let solved = match solve(&Checked { size: c.size, facelets: seen, scramble: None }, opts.mode) {
                Ok(s) => s,
                Err(e) if i == 0 => return Err(e),
                Err(_) => continue,
            };
            count += 1;
            let fm = move_map(&g, turns);
            let seq: Vec<usize> = solved.moves.iter().map(|&m| fm[m]).collect();
            if !verify(&g, &c.facelets, &seq) {
                continue;
            }
            for s in orders(&seq, 16) {
                let moves: Vec<Turn> = s.iter().map(|&m| Turn::from_index(m)).collect();
                candidates += 1;
                best.offer(cost(&moves, PlanMode::Legacy), &moves);
            }
        }
        solutions = count;
    }
    // With the optimizing planner, the best few are planned again with it.
    let (est_ms, moves) = if opts.plan == PlanMode::Optimized {
        best.items
            .iter()
            .map(|(_, m)| (cost(m, PlanMode::Optimized), m.clone()))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .ok_or_else(|| ApiError::new("solver_failed", "no solution was found"))?
    } else {
        best.items.first().cloned().ok_or_else(|| ApiError::new("solver_failed", "no solution was found"))?
    };
    let seq: Vec<usize> = moves.iter().filter_map(layers_index).collect();
    if !verify(&g, &c.facelets, &seq) {
        return Err(ApiError::new("solver_failed", "the chosen solution does not solve the cube; this is a bug"));
    }
    Ok(Choice { moves, est_ms, candidates, solutions, compute_ms: t0.elapsed().as_secs_f64() * 1000.0 })
}
