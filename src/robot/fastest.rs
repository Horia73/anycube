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
//! The plain solver's answer (what /api/solve gives for the cube, in the
//! mode of `Options`) is always the first candidate, costed like every
//! other, so the solution taken is never slower on the robot than it; and
//! since it is always there, a cube that passed the check always gets a
//! solution, whatever becomes of the search for more.
//!
//! 3x3x3: first the plain two-phase search, for the shortest solution
//! (`plain_ms` at most: past that it gives the shortest it has, which
//! `Choice::plain_complete` says). Then the search again, this time keeping
//! every solution it meets of at most `slack` moves more than that one (all
//! ways into the domino subgroup on the three axes, for the cube and its
//! inverse, shortest first), for `search_ms`. Each also counts in the
//! orders of its pairs of opposite-face turns (U D or D U), which the robot
//! makes differently. Searching the 24 orientations of the cube on top of
//! that would add nothing: the axes and the inverse already give every way
//! into the subgroup of each length, which is what the orientations would
//! change.
//!
//! Other sizes: the solver is run on the cube as it lies (the plain
//! answer), then on the cube seen from each of the other 23 orientations,
//! as long as the time allows, and the quickest is taken.

use super::Turn;
use crate::api::check::{ApiError, Checked};
use crate::api::solve::{solve, Mode};
use crate::cube::Cube;
use crate::cuben::Geometry;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Options {
    /// 3x3x3: the longest the plain search for the shortest solution may
    /// take (ms). Within it the answer is /api/solve's own; past it, the
    /// shortest found by then.
    pub plain_ms: f64,
    /// 3x3x3: how long the search for more candidates may run after that (ms).
    pub search_ms: f64,
    /// 3x3x3: no candidate is costed after this (ms from the start of the
    /// search for more; the shortest solutions are costed first).
    pub total_ms: f64,
    /// 3x3x3: at most this many solutions from the search.
    pub max_solutions: usize,
    /// 3x3x3: moves above the plain answer that a candidate may have.
    pub slack: usize,
    /// 3x3x3: the longest first phase followed.
    pub depth: usize,
    /// 3x3x3: second phases of the shortest length kept per first phase.
    pub phase2: usize,
    /// Other sizes: the time (ms) after which no further orientation is started.
    pub orientation_ms: f64,
    /// The plain solver's mode (every size).
    pub mode: Mode,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            plain_ms: 1000.0,
            search_ms: 150.0,
            total_ms: 160.0,
            max_solutions: 2000,
            slack: 2,
            depth: 14,
            phase2: 4,
            orientation_ms: 2000.0,
            mode: Mode::Normal,
        }
    }
}

/// The plain solver's answer, as a candidate.
#[derive(Clone, Debug)]
pub struct Plain {
    pub moves: Vec<Turn>,
    /// The robot's time (ms) for it as the solver gave it, by the same planner.
    pub est_ms: f64,
    /// The search for it ran to its end: it is /api/solve's own answer.
    pub complete: bool,
    /// What finding it took (ms).
    pub compute_ms: f64,
}

#[derive(Clone, Debug)]
pub struct Choice {
    pub moves: Vec<Turn>,
    /// The robot's estimate of its time (ms).
    pub est_ms: f64,
    /// The plain solver's answer: the chosen solution is never slower than it.
    pub plain: Plain,
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

/// The robot's time (ms) for a sequence of moves.
pub type Cost<'a> = dyn Fn(&[Turn]) -> f64 + 'a;

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

/// Keeps the cheapest (the first of equals).
#[derive(Default)]
struct Best(Option<(f64, Vec<Turn>)>);

impl Best {
    fn offer(&mut self, ms: f64, moves: &[Turn]) {
        if self.0.as_ref().is_none_or(|(best, _)| ms < *best) {
            self.0 = Some((ms, moves.to_vec()));
        }
    }
}

fn verify(g: &Geometry, facelets: &[u8], moves: &[usize]) -> bool {
    let mut f = facelets.to_vec();
    g.apply_seq(&mut f, moves);
    g.is_solved(&f)
}

/// The quickest solution of a checked cube for a robot with time `cost`.
///
/// The plain solver's answer is always a candidate (`Choice::plain`), so a
/// cube that passed the check always gets a solution and never a slower
/// one than that. Each search is given a time (`Options`), kept to once it
/// has a solution.
pub fn choose(c: &Checked, cost: &Cost, opts: &Options) -> Result<Choice, ApiError> {
    let t0 = Instant::now();
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
    let after = |t: Instant, ms: f64| t + std::time::Duration::from_secs_f64(ms.clamp(0.0, 3.6e6) / 1000.0);
    let g = Geometry::new(c.size);
    if g.is_solved(&c.facelets) {
        let plain = Plain { moves: Vec::new(), est_ms: cost(&[]), complete: true, compute_ms: 0.0 };
        return Ok(Choice { moves: Vec::new(), est_ms: plain.est_ms, plain, candidates: 1, solutions: 1, compute_ms: 0.0 });
    }
    let mut best = Best::default();
    let mut candidates = 0;
    let mut solutions = 0;
    // A solution's orders of opposite turns, each costed.
    let mut offer = |seq: &[usize], best: &mut Best| {
        for s in orders(seq, 16) {
            let turns: Vec<Turn> = s.iter().map(|&m| Turn::from_index(m)).collect();
            candidates += 1;
            best.offer(cost(&turns), &turns);
        }
    };
    let as_plain = |seq: &[usize], complete: bool| {
        let moves: Vec<Turn> = seq.iter().map(|&m| Turn::from_index(m)).collect();
        Plain { est_ms: cost(&moves), moves, complete, compute_ms: ms(t0) }
    };
    let c3 = if c.size == 3 {
        Cube::from_small_facelets(&g.to_string(&c.facelets)).ok().and_then(|x| x.with_home_centres()).and_then(|x| crate::c3::C3::from_cube(&x))
    } else {
        None
    };
    let plain;
    if let Some(c3) = c3 {
        let (two3, t3) = (crate::api::solve::two3(), crate::api::solve::t3());
        // The plain search: the first candidate, whatever the time.
        let effort = if opts.mode == Mode::Fast { crate::two3::Effort::FAST } else { crate::two3::Effort::NORMAL };
        let (sol, complete) = two3.solve_until(t3, &c3, effort, after(t0, opts.plain_ms));
        let seq: Vec<usize> = sol.iter().map(|&m| m as usize).collect();
        if !verify(&g, &c.facelets, &seq) {
            return Err(ApiError::new("solver_failed", "the solver's answer does not solve the cube; this is a bug in the solver"));
        }
        plain = as_plain(&seq, complete);
        solutions += 1;
        offer(&seq, &mut best);
        // More candidates, none more than the slack longer than that one.
        let t1 = Instant::now();
        let found = two3.candidates(
            t3,
            &c3,
            crate::two3::Candidates {
                depth: opts.depth,
                slack: opts.slack,
                max: opts.max_solutions,
                deadline: Some(after(t1, opts.search_ms)),
                phase2: opts.phase2,
                seed: Some(seq.len()),
            },
        );
        let total = after(t1, opts.total_ms.max(opts.search_ms));
        for found in &found {
            // The shortest come first; they are costed while there is time.
            if Instant::now() > total {
                break;
            }
            let more: Vec<usize> = found.iter().map(|&m| m as usize).collect();
            if more == seq || !verify(&g, &c.facelets, &more) {
                continue;
            }
            solutions += 1;
            offer(&more, &mut best);
        }
    } else {
        let mut first = None;
        for (i, turns) in orientations().iter().enumerate() {
            if i > 0 && ms(t0) > opts.orientation_ms {
                break;
            }
            let seen = if c.size % 2 == 1 { g.reframe(&c.facelets, turns) } else { g.held(&c.facelets, turns) };
            let solved = match solve(&Checked { size: c.size, facelets: seen, scramble: None }, opts.mode) {
                Ok(s) => s,
                // The cube as it lies is the plain answer: without it there is none.
                Err(e) if i == 0 => return Err(e),
                Err(_) => continue,
            };
            let fm = move_map(&g, turns);
            let seq: Vec<usize> = solved.moves.iter().map(|&m| fm[m]).collect();
            if !verify(&g, &c.facelets, &seq) {
                if i == 0 {
                    return Err(ApiError::new("solver_failed", "the solver's answer does not solve the cube; this is a bug in the solver"));
                }
                continue;
            }
            if i == 0 {
                first = Some(as_plain(&seq, true));
            }
            solutions += 1;
            offer(&seq, &mut best);
        }
        plain = first.expect("the first orientation was solved or the error returned");
    }
    let (est_ms, moves) = best.0.expect("the plain answer was offered");
    debug_assert!(est_ms <= plain.est_ms);
    Ok(Choice { moves, est_ms, plain, candidates, solutions, compute_ms: ms(t0) })
}

/// How many cubes `warm` solves.
const WARM_SOLVES: u64 = 4;

/// Makes the first choice of a program as quick as any later one, once for
/// each size: the tables are read through (`api::solve::warm`) and, for the
/// 3x3x3, a few random cubes are solved as `choose` solves them, which
/// wakes the cores and brings the search and its tables into the
/// processor's caches. Without this the first choices of a fresh program
/// spend their few milliseconds waiting for pages and find a handful of
/// candidates where later ones find hundreds. Whoever calls while another
/// call is at it waits until it is done. Returns whether this call did the work.
pub fn warm(size: usize, cost: &Cost, opts: &Options) -> bool {
    static DONE: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
    let mut done = DONE.lock().unwrap_or_else(|e| e.into_inner());
    if done.contains(&size) {
        return false;
    }
    crate::api::solve::warm(size);
    if size == 3 {
        // Cubes mixed by thirty moves each, the same ones every time.
        let mut seed = 0x3a57_e57_u64;
        for _ in 0..WARM_SOLVES {
            let moves: Vec<String> = (0..30)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    Turn::from_index((seed % 18) as usize).name()
                })
                .collect();
            if let Ok(cube) = crate::api::check(3, crate::api::Input::Moves(&moves.join(" ")), 3) {
                // Both searches, each for a short while.
                let quick = Options { plain_ms: opts.plain_ms.min(30.0), search_ms: opts.search_ms.min(15.0), total_ms: opts.total_ms.min(25.0), ..opts.clone() };
                let _ = choose(&cube, cost, &quick);
            }
        }
    }
    done.push(size);
    true
}
