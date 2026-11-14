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
use crate::solver::Params;
use crate::solver5::Params5;
use crate::solver6::Solver6;
use crate::stage7::Stage7;

/// Positions the exact search for the way home may visit before a beam
/// search takes over. On a cube with several orbits out of place it does
/// not finish within a hundred times as many, so it is kept short: it is
/// there for the cubes that are nearly home.
const HOME_BUDGET: u64 = 1_000_000;

/// States per level of the beam search that reduces the innermost ring of
/// an even cube as a 6x6x6.
const SIX_BEAM: usize = 100_000;

/// Wing splits tried there. The 6x6x6 solver reaches this step with many
/// stagings and takes two splits of each; here there is one staging, and
/// the shortest of more splits makes up for some of that.
const SIX_SPLITS: usize = 6;

/// Before that, one beam search from many splits at once, and its width.
const JOINT_SPLITS: usize = 64;
const JOINT_BEAM: usize = 200_000;

pub struct SolverN {
    /// The 6x6x6 solver, whose parts are used one by one: its 4x4x4 and
    /// 5x5x5 solvers, its staging, and its inner steps.
    pub(crate) six: Solver6,
    pub(crate) s7: Stage7,
    home4: Home4,
    pub(crate) orbits5: Vec<crate::orbits::CentreOrbit>,
    /// How wide the staging of the centres goes.
    pub staging: crate::staging::Staging,
    home5: AxisSolver,
    home6: AxisSolver,
    home7: AxisSolver,
}

/// Staged centres of a 4x4x4 brought home, whatever becomes of its edges.
struct Home4 {
    /// Moves still needed, by the patterns of the three axes.
    left: Vec<u8>,
}

/// Moves of a 4x4x4 that keep staged centres staged: the face turns and the wide half turns.
const HOME4_MOVES: [u8; 24] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 22, 25, 28, 31, 34];

impl Home4 {
    fn new(tabs: &crate::tables::Tabs) -> Home4 {
        let index = |mask: u32| tabs.ct3_index(mask, 0) / 2;
        let mut left = vec![255u8; 70 * 70 * 70];
        // Home is the solved cube any of the four ways round that keep every axis.
        let mut frontier: Vec<u32> = Vec::new();
        for r in &crate::cube::tables().rots {
            let ct = &r.state.ct;
            let mask = crate::tables::ct3_mask(&r.state);
            if ct[0] % 3 == 0 && ct[8] % 3 == 2 && ct[16] % 3 == 1 && left[index(mask)] == 255 {
                left[index(mask)] = 0;
                frontier.push(mask);
            }
        }
        let mut depth = 0;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for &x in &frontier {
                for &m in &HOME4_MOVES {
                    let y = tabs.bp.apply(m, x);
                    if left[index(y)] == 255 {
                        left[index(y)] = depth + 1;
                        next.push(y);
                    }
                }
            }
            frontier = next;
            depth += 1;
        }
        Home4 { left }
    }

    /// The shortest way home from the mask of the centres that show U, F
    /// or R; `None` if they are not staged.
    fn solve(&self, tabs: &crate::tables::Tabs, mut mask: u32) -> Option<Vec<u8>> {
        if (0..3).any(|axis| (mask >> (8 * axis) & 255).count_ones() != 4) {
            return None;
        }
        let index = |mask: u32| tabs.ct3_index(mask, 0) / 2;
        let mut out = Vec::new();
        while self.left[index(mask)] > 0 {
            let left = self.left[index(mask)];
            let m = HOME4_MOVES.iter().copied().find(|&m| self.left[index(tabs.bp.apply(m, mask))] + 1 == left)?;
            mask = tabs.bp.apply(m, mask);
            out.push(m);
        }
        Some(out)
    }
}

pub struct SolutionN {
    /// Moves of the cube: `(face + 6 * (layers - 1)) * 3 + power`.
    pub moves: Vec<usize>,
    /// Moves spent on the centres, on the edges, and on the 3x3x3 at the end.
    pub lens: [usize; 3],
    /// The matching of the centres found nothing and the cube went the
    /// longer way: every centre home before the edges.
    pub long_way: bool,
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
        self.turn_until(|f| (0..6).all(|face| f[face * n * n + row * n + col] as usize == face), "the innermost centres are not solved")
    }

    /// Turns the frame so that the given cell of every face shows a colour
    /// of that face's own axis. For even cubes whose centres were staged
    /// with any pair of colours on any pair of faces, before a step that
    /// takes each pair to be on its own: with two pairs exchanged, such a
    /// step brings the centres home as a mirror image of the cube's
    /// colours, which no way of holding the cube puts right.
    fn reaxis(&mut self, row: usize, col: usize) -> Result<(), String> {
        let n = self.g.n;
        self.turn_until(|f| (0..6).all(|face| f[face * n * n + row * n + col] as usize % 3 == face % 3), "the innermost centres are not staged")
    }

    /// Turns the frame, by the first of the 24 ways of holding the cube that
    /// `ok` accepts.
    fn turn_until(&mut self, ok: impl Fn(&[u8]) -> bool, or: &str) -> Result<(), String> {
        let mut frames: Vec<Vec<usize>> = vec![Vec::new()];
        let mut at = 0;
        while at < frames.len() {
            let turns = frames[at].clone();
            at += 1;
            let seen = self.g.turned(&self.f, &turns);
            if ok(&seen) {
                self.f = seen;
                let map = self.g.frame_moves(&turns);
                self.to_given = map.iter().map(|&m| self.to_given[m]).collect();
                return Ok(());
            }
            if turns.len() < 4 {
                frames.extend((0..3).map(|t| turns.iter().copied().chain([t]).collect()));
            }
        }
        Err(or.to_string())
    }
}

impl SolverN {
    pub fn load() -> SolverN {
        let six = Solver6::load();
        let s6 = &six.s6;
        let s7 = Stage7::load();
        let orbits5 = centre_orbits(&Geometry::new(5));
        fn all(orbits: &[crate::orbits::CentreOrbit]) -> Vec<&crate::orbits::CentreOrbit> {
            orbits.iter().collect()
        }
        let home5 = AxisSolver::new("555", &all(&orbits5), 36, &[&[0, 1]]);
        let home6 = AxisSolver::new("666", &all(&s6.orbits), 54, &[&[0, 1, 2, 3]]);
        // Outer x, oblique, outer t, oblique, inner x, inner t: the obliques with the inner and with the outer ones.
        let home7 = AxisSolver::new("777", &all(&s7.orbits), 54, &[&[1, 3, 4, 5], &[1, 3, 0, 2]]);
        let home4 = Home4::new(&six.solver4.tabs);
        SolverN { six, s7, home4, orbits5, staging: crate::staging::Staging::normal(), home5, home6, home7 }
    }

    /// The four layers as a 4x4x4, reduced: its moves.
    fn reduce4(&self, g: &Geometry, f: &[u8], layers: &[usize]) -> Result<Vec<u8>, String> {
        let cube = Cube::from_facelets(&g.to_string(&g.sub_facelets(f, layers)))?;
        let sol = self.six.solver4.solve(&cube, &Params::fast())?;
        Ok(sol.moves[..sol.moves.len() - sol.lens[3]].to_vec())
    }

    /// Solves a cube of size 6 or more given as its facelets (faces U R F D
    /// L B, row by row; colours 0 to 5 in that order).
    pub fn solve(&self, g: &Geometry, facelets: &[u8]) -> Result<SolutionN, String> {
        match self.attempt(g, facelets, true) {
            Ok(sol) => Ok(sol),
            Err(e) => {
                if std::env::var_os("CUBEN_TRACE").is_some() {
                    eprintln!("  {}; the long way instead", e);
                }
                self.attempt(g, facelets, false)
            }
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
        // Staging: every centre onto a face of its own axis (`staging`).
        // An even cube with nothing solved yet has no faces of its own: its
        // centres may end on any faces, an axis to a pair.
        let none = |what: &str, layers: &[usize]| format!("no {} found for the centres of layers {:?}", what, layers);
        let staged = self.stage_centres(g, &w.f, odd || !matched, &self.staging)?;
        for &m in &staged {
            g.apply(&mut w.f, m);
            w.moves.push(w.to_given[m]);
        }
        if trace {
            eprintln!("  centres staged in {} moves, {} so far", staged.len(), w.moves.len());
        }
        // The long way: every step's centres home, from the middle outwards.
        if !matched && !odd {
            // The innermost centres of an even cube say which colour belongs
            // on which face. They are staged like the rest by now, and come
            // home in a handful of face turns and half turns of half the
            // cube, which keep every centre staged. (This used to be a whole
            // reduction of the innermost 4x4x4 before the staging, at 24
            // moves, whose edges the staging then took apart again.)
            let layers = [0, half - 1, half, last];
            let cube = Cube::from_facelets(&g.to_string(&g.sub_facelets(&w.f, &layers)))?;
            let home = self.home4.solve(&self.six.solver4.tabs, crate::tables::ct3_mask(&cube));
            let moves = home.ok_or_else(|| none("way home", &layers))?;
            w.apply(&moves, &[1, half]);
            w.rehome(half - 1, half - 1)?;
            if trace {
                eprintln!("  innermost centres home in {} moves, {} so far", moves.len(), w.moves.len());
            }
        }
        if !matched {
            for (size, layers, depths) in &steps {
                let sub = g.sub_facelets(&w.f, layers);
                let home = match size {
                    5 => self.home5.solve(&sub, HOME_BUDGET),
                    6 => self.home6.solve(&sub, HOME_BUDGET),
                    _ => self.home7.solve(&sub, HOME_BUDGET),
                };
                let moves = home.ok_or_else(|| none("way home", layers))?;
                w.apply(&moves, depths);
                if trace {
                    eprintln!("  centres of layers {:?} home in {} moves, {} so far", &layers[1..layers.len() / 2 + 1], moves.len(), w.moves.len());
                }
            }
        }
        // Every centre made to agree with the one under it, ring by ring
        // from the rim inwards; the edge steps then bring them home.
        // On an even cube the innermost ring is not matched: the layers of
        // that ring and the four middle ones are a 6x6x6, and the inner
        // steps of the 6x6x6 solver do the work for less (see below).
        let as_six = matched && !odd;
        if matched {
            for a in 1..if as_six { first - 1 } else { first } {
                let moves = crate::rings::Ring::new(g, a).solve(g, &w.f)?;
                w.apply_ours(&moves);
                if trace {
                    eprintln!("  centres of layer {} matched to layer {}: {} moves, {} so far", a + 1, a + 2, moves.len(), w.moves.len());
                }
            }
        }
        let centres = w.moves.len();

        // The edges, from the middle outwards.
        let mut tail = 0;
        if as_six {
            // The innermost ring with the four middle layers, as a 6x6x6
            // whose centres are staged: its inner wings are split and its
            // inner 4x4x4 reduced by a search that also makes each pair of
            // obliques one colour (`solver6`). That is the matching of the
            // innermost ring and the reduction of the innermost wings in
            // one, for about a dozen moves less than the two apart. Its
            // moves are face turns and half turns as deep as that ring or
            // deeper, which leave the rows nearer the rim matched.
            // The staging may have left any pair of colours on any pair of faces.
            w.reaxis(half - 1, half - 1)?;
            let layers = [0, half - 2, half - 1, half, half + 1, last];
            let sub = g.sub_facelets(&w.f, &layers);
            // One search for many splits first: a sixth of the time of a
            // search for each of a few, and as short where it finds
            // anything, which is on five cubes in six.
            let mut found = self.six.inner_joint(&sub, JOINT_BEAM, JOINT_SPLITS, 4);
            if found.is_empty() {
                found = self.six.inner_steps(&[], &sub, 0, SIX_BEAM, SIX_SPLITS);
            }
            let best = found.iter().min_by_key(|c| c.0.len()).ok_or_else(|| none("reduction of the innermost ring", &layers))?;
            w.apply(&best.0, &[1, half - 1, half]);
            w.rehome(half - 1, half - 1)?;
            if trace {
                eprintln!("  innermost ring and wings as a 6x6x6: {} moves, {} so far", best.0.len(), w.moves.len());
            }
        } else if !odd {
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
            // Without a clock: the same cube gives the same solution however busy the machine is.
            let wide = Params5 { time_ms: 0, ..Params5::normal() };
            let moves = if k > 1 {
                self.six.outer.reduce(&sub, &wide)?
            } else {
                let sol = self.six.outer.solve(&sub, &wide)?;
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
        Ok(SolutionN { moves: w.moves, lens: [centres, total - centres - tail, tail], long_way: !matched })
    }
}
