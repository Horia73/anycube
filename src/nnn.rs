//! Cubes of any size, by way of the small ones.
//!
//! Some layers of a large cube, taken together, behave like a small cube:
//! pick a few of them symmetrically, let every layer in between follow the
//! picked layer next to it towards the middle, and a turn of the outer one,
//! two or three picked layers of the small cube is a turn of the large cube
//! down to the layer before the next picked one. A piece of the large cube
//! that is not among the picked layers then travels with the piece of the
//! small cube whose block it is in. If it had that piece's colour before, it
//! has it after, so whatever is solved in a block stays solved with it when
//! the small cube is solved.
//!
//! That fixes the order. The centres go ring by ring from the middle
//! outwards, and within a ring from the middle of its sides to its corners,
//! two mirror orbits of obliques at a time: what is already solved then
//! lies either in the middle block, which never leaves its face, or in
//! blocks that are solved throughout. Each step is a 6x6x6 or a 7x7x7 with
//! most of its centres done: its centres are staged with the staging of
//! those cubes (`stage6`, `stage7`) and brought home by `axes`. The same
//! holds for staging with "of its own axis" in place of "solved", so the
//! steps are gone through three times: the R/L centres of every step onto R
//! and L, then the rest onto their axes, then everything home.
//!
//! The edges follow, orbit by orbit from the middle outwards, each one a
//! 5x5x5 whose middle edges are the stretch of edge already joined (a
//! 4x4x4 for the innermost wings of an even cube). The last of them is the
//! whole cube as a 5x5x5, solved to the end.
//!
//! Nothing here depends on the size, and nothing is tuned to one: this is
//! the baseline for every cube from 8x8x8 up.

use crate::axes::AxisSolver;
use crate::cube::Cube;
use crate::cuben::Geometry;
use crate::orbits::centre_orbits;
use crate::solver::{Params, Solver};
use crate::solver5::{Params5, Solver5};
use crate::stage6::Stage6;
use crate::stage7::Stage7;

/// Positions the exact search for the way home may visit before a beam
/// search takes over. On a cube with several orbits out of place it does
/// not finish within a hundred times as many, so it is kept short: it is
/// there for the cubes that are nearly home.
const HOME_BUDGET: u64 = 1_000_000;

pub struct SolverN {
    solver4: Solver,
    s5: Solver5,
    s6: Stage6,
    s7: Stage7,
    orbits5: Vec<crate::orbits::CentreOrbit>,
    home5: AxisSolver,
    home6: AxisSolver,
    home7: AxisSolver,
}

pub struct SolutionN {
    /// Moves of the cube: `(face + 6 * (layers - 1)) * 3 + power`.
    pub moves: Vec<usize>,
    /// Moves spent on the centres, on the edges, and on the 3x3x3 at the end.
    pub lens: [usize; 3],
    /// The matching of the centres found nothing and the cube went the
    /// longer way: every centre home before the edges.
    pub long_way: bool,
    /// Some row of centres was not matched in one search but piece by
    /// piece, which takes many more moves.
    pub laboured: bool,
}

/// The cube while it is being solved: as it is now, seen in a frame in
/// which the centres are (or will be) at home, and the moves so far in the
/// frame it was given in.
struct Work<'a> {
    g: &'a Geometry,
    f: Vec<u8>,
    moves: Vec<usize>,
    to_given: Vec<usize>,
}

impl Work<'_> {
    /// Moves of a small cube made of some layers: a turn of its outer `d + 1`
    /// layers is a turn of ours `depths[d]` layers deep.
    fn apply(&mut self, small: &[u8], depths: &[usize]) {
        for &m in small {
            let (face, d, power) = (m as usize / 3 % 6, m as usize / 18, m as usize % 3);
            let ours = (face + 6 * (depths[d] - 1)) * 3 + power;
            self.g.apply(&mut self.f, ours);
            self.moves.push(self.to_given[ours]);
        }
    }

    /// Moves of the cube itself.
    fn apply_ours(&mut self, ours: &[u8]) {
        for &m in ours {
            self.g.apply(&mut self.f, m as usize);
            self.moves.push(self.to_given[m as usize]);
        }
    }

    /// Turns the frame so that the given cell of every face shows that
    /// face's own colour. For even cubes, whose centres may be solved with
    /// the cube held any way round.
    fn rehome(&mut self, row: usize, col: usize) -> Result<(), String> {
        let n = self.g.n;
        let at_home = |f: &[u8]| (0..6).all(|face| f[face * n * n + row * n + col] as usize == face);
        let mut frames: Vec<Vec<usize>> = vec![Vec::new()];
        let mut at = 0;
        while at < frames.len() {
            let turns = frames[at].clone();
            at += 1;
            let seen = self.g.turned(&self.f, &turns);
            if at_home(&seen) {
                self.f = seen;
                let map = self.g.frame_moves(&turns);
                self.to_given = map.iter().map(|&m| self.to_given[m]).collect();
                return Ok(());
            }
            if turns.len() < 4 {
                frames.extend((0..3).map(|t| turns.iter().copied().chain([t]).collect()));
            }
        }
        Err("the innermost centres are not solved".to_string())
    }
}

impl SolverN {
    pub fn load() -> SolverN {
        let s6 = Stage6::load();
        let s7 = Stage7::load();
        let orbits5 = centre_orbits(&Geometry::new(5));
        fn all(orbits: &[crate::orbits::CentreOrbit]) -> Vec<&crate::orbits::CentreOrbit> {
            orbits.iter().collect()
        }
        let home5 = AxisSolver::new("555", &all(&orbits5), 36, &[&[0, 1]]);
        let home6 = AxisSolver::new("666", &all(&s6.orbits), 54, &[&[0, 1, 2, 3]]);
        // Outer x, oblique, outer t, oblique, inner x, inner t: the obliques with the inner and with the outer ones.
        let home7 = AxisSolver::new("777", &all(&s7.orbits), 54, &[&[1, 3, 4, 5], &[1, 3, 0, 2]]);
        SolverN { solver4: Solver::new(), s5: Solver5::load(), s6, s7, orbits5, home5, home6, home7 }
    }

    /// The four layers as a 4x4x4, reduced: its moves.
    fn reduce4(&self, g: &Geometry, f: &[u8], layers: &[usize]) -> Result<Vec<u8>, String> {
        let cube = Cube::from_facelets(&g.to_string(&g.sub_facelets(f, layers)))?;
        let sol = self.solver4.solve(&cube, &Params::fast())?;
        Ok(sol.moves[..sol.moves.len() - sol.lens[3]].to_vec())
    }

    /// Solves a cube of size 6 or more given as its facelets (faces U R F D
    /// L B, row by row; colours 0 to 5 in that order).
    pub fn solve(&self, g: &Geometry, facelets: &[u8]) -> Result<SolutionN, String> {
        let trace = std::env::var_os("CUBEN_TRACE").is_some();
        let matched = match self.attempt(g, facelets, true) {
            // Every row matched in one search: this is the shorter way.
            Ok(sol) if !sol.laboured => return Ok(sol),
            Ok(sol) => Some(sol),
            Err(e) => {
                if trace {
                    eprintln!("  {}; the long way instead", e);
                }
                None
            }
        };
        // Otherwise the matching may have cost more than it saves: both ways, and the shorter.
        let long = self.attempt(g, facelets, false);
        match (matched, long) {
            (Some(a), Ok(b)) => Ok(if a.moves.len() <= b.moves.len() { a } else { b }),
            (Some(a), Err(_)) => Ok(a),
            (None, long) => long,
        }
    }

    /// `matched`: the centres are only made to agree with their neighbours
    /// towards the middle (`rings`), and the edge steps bring them home.
    /// Otherwise every centre is brought home first (`axes`).
    fn attempt(&self, g: &Geometry, facelets: &[u8], matched: bool) -> Result<SolutionN, String> {
        let n = g.n;
        if n < 6 {
            return Err("sizes below 6 have solvers of their own".to_string());
        }
        if facelets.len() != 6 * n * n {
            return Err(format!("expected {} facelets, got {}", 6 * n * n, facelets.len()));
        }
        let trace = std::env::var_os("CUBEN_TRACE").is_some();
        let mut w = Work { g, f: facelets.to_vec(), moves: Vec::new(), to_given: (0..g.move_count()).collect() };
        let (odd, half, last) = (n % 2 == 1, n / 2, n - 1);
        // The layer the innermost ring of centres is in, and the layer that stands for the middle.
        let (first, mid) = if odd { (half - 1, half) } else { (half - 1, half - 1) };

        // The steps of the centres, from the middle outwards: each a small
        // cube made of some layers, and how deep its turns reach in ours.
        let mut steps: Vec<(usize, Vec<usize>, Vec<usize>)> = Vec::new();
        if odd {
            steps.push((5, vec![0, half - 1, half, half + 1, last], vec![1, half]));
        } else if !matched {
            // The innermost centres of an even cube say which colour belongs on which face.
            let reduction = self.reduce4(g, &w.f, &[0, half - 1, half, last])?;
            w.apply(&reduction, &[1, half]);
            w.rehome(half - 1, half - 1)?;
        }
        for a in (1..first).rev() {
            for b in (a + 1..=first).rev() {
                match !odd && b == first {
                    // No layer between the two obliques of a pair: a 6x6x6.
                    true => steps.push((6, vec![0, a, b, last - b, last - a, last], vec![1, a + 1, b + 1])),
                    false => steps.push((7, vec![0, a, b, mid, last - b, last - a, last], vec![1, a + 1, b + 1])),
                }
            }
        }
        // One axis at a time on every step, then everything home. A step's
        // staging mixes the colours of the centres staged before it; done
        // this way they are still to be solved then, so nothing is solved twice.
        let mask = |o: &crate::orbits::CentreOrbit, f: &[u8], axis: u8| {
            let col = o.colours(f);
            (0..24).fold(0u32, |m, i| m | ((col[i] % 3 == axis) as u32) << i)
        };
        for pass in 0..if matched { 2 } else { 3 } {
            for (size, layers, depths) in &steps {
                let sub = g.sub_facelets(&w.f, layers);
                let what = ["staging on R and L", "staging on the other axes", "way home"][pass];
                let none = || format!("no {} found for the centres of layers {:?} (as a {}x{}x{})", what, layers, size, size, size);
                let home = none;
                // A beam that finds nothing is rare; a wider one then does.
                let stage7 = |range: std::ops::Range<usize>| {
                    [(20_000, 0), (200_000, 1), (1_000_000, 2)]
                        .iter()
                        .map(|&(beam, salt)| self.s7.stage_steps(&self.s5.stage, &sub, 4, 4, beam, salt, range.clone()))
                        .find(|found| !found.is_empty())
                        .and_then(|found| found.into_iter().next())
                        .map(|x| x.moves)
                        .ok_or_else(none)
                };
                let (x, t) = (&self.orbits5[0], &self.orbits5[1]);
                let moves: Vec<u8> = match (size, pass) {
                    (5, 0) => {
                        let found = self.s5.stage.phase_a(mask(x, &sub, 1), mask(t, &sub, 1), 0, 1, None);
                        found.into_iter().next().ok_or_else(none)?
                    }
                    // Of the two parities of the wings either will do here.
                    (5, 1) => self.s5.stage.phase_b(mask(x, &sub, 0), mask(t, &sub, 0), false),
                    (5, _) => self.home5.solve(&sub, HOME_BUDGET).ok_or_else(home)?,
                    (6, 0) => self.s6.stage_steps(&self.s5.stage, &sub, 4, 0..2).into_iter().next().map(|x| x.moves).ok_or_else(none)?,
                    (6, 1) => self.s6.stage_steps(&self.s5.stage, &sub, 4, 2..3).into_iter().next().map(|x| x.moves).ok_or_else(none)?,
                    (6, _) => self.home6.solve(&sub, HOME_BUDGET).ok_or_else(home)?,
                    (_, 0) => stage7(0..3)?,
                    (_, 1) => stage7(3..5)?,
                    _ => self.home7.solve(&sub, HOME_BUDGET).ok_or_else(home)?,
                };
                w.apply(&moves, depths);
                if trace {
                    eprintln!(
                        "  {} of layers {:?} as a {}x{}x{}: {} moves, {} so far",
                        ["R/L centres", "U/D centres", "centres home"][pass],
                        &layers[1..layers.len() / 2 + 1],
                        size,
                        size,
                        size,
                        moves.len(),
                        w.moves.len()
                    );
                }
            }
        }
        // Every centre made to agree with the one under it, ring by ring
        // from the rim inwards; the edge steps then bring them home.
        let mut laboured = false;
        if matched {
            for a in 1..first {
                let (moves, whole) = crate::rings::Ring::new(g, a).solve(g, &w.f)?;
                laboured |= !whole;
                w.apply_ours(&moves);
                if trace {
                    eprintln!("  centres of layer {} matched to layer {}: {} moves, {} so far", a + 1, a + 2, moves.len(), w.moves.len());
                }
            }
        }
        let centres = w.moves.len();

        // The edges, from the middle outwards.
        let mut tail = 0;
        if !odd {
            let layers = [0, half - 1, half, last];
            let reduction = self.reduce4(g, &w.f, &layers)?;
            w.apply(&reduction, &[1, half]);
            w.rehome(half - 1, half - 1)?;
            if trace {
                eprintln!("  innermost wings: {} moves so far", w.moves.len());
            }
        }
        for k in (1..if odd { half } else { half - 1 }).rev() {
            let layers = [0, k, mid, last - k, last];
            let sub = g.sub_facelets(&w.f, &layers);
            let moves = if k > 1 {
                self.s5.reduce(&sub, &Params5 { time_ms: 0, ..Params5::fast() })?
            } else {
                let sol = self.s5.solve(&sub, &Params5::normal())?;
                tail = sol.lens[5];
                sol.moves
            };
            w.apply(&moves, &[1, k + 1]);
            if trace {
                eprintln!("  wings of layer {}: {} moves so far", k + 1, w.moves.len());
            }
        }
        if !g.is_solved(&w.f) {
            return Err("the steps did not solve the cube".to_string());
        }
        let total = w.moves.len();
        Ok(SolutionN { moves: w.moves, lens: [centres, total - centres - tail, tail], long_way: !matched, laboured })
    }
}
