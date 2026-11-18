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
use crate::solver5::{Params5, Solution5};
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Search {
    Normal,
    ChooseWings,
    ChooseWidthsAndWings,
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

/// A turn of a single width can leave each face in a 2 by 2 or 3 by 3
/// grid of uniform rectangles. Rotation exchanges rows and columns, so
/// every boundary on every face must have the same distance to the rim.
fn block_width(n: usize, f: &[u8]) -> Option<usize> {
    if n < 6 || f.len() != 6 * n * n {
        return None;
    }
    let mut width = None;
    for face in f.chunks_exact(n * n) {
        for i in 1..n {
            let w = i.min(n - i);
            if width == Some(w) {
                continue;
            }
            let boundary = (0..n).any(|j| {
                face[j * n + i] != face[j * n + i - 1] || face[i * n + j] != face[(i - 1) * n + j]
            });
            if boundary {
                if width.is_some() {
                    return None;
                }
                width = Some(w);
            }
        }
    }
    width
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
        self.solve_blocks_or_edges(g, facelets, false)
    }

    /// Compare complete solutions from different penultimate wing reductions.
    /// Starting at size nine, also try centre turns whose widths prepare the
    /// next column. Keep the shorter verified solution, including normal mode's.
    pub fn solve_full(&self, g: &Geometry, facelets: &[u8]) -> Result<SolutionN, String> {
        self.solve_blocks_or_edges(g, facelets, true)
    }

    fn solve_blocks_or_edges(&self, g: &Geometry, facelets: &[u8], full: bool) -> Result<SolutionN, String> {
        let blocks = block_width(g.n, facelets).and_then(|width| self.solve_blocks(g, facelets, width));
        let candidate = match blocks {
            Some((solution, true)) => return Ok(solution),
            Some((solution, false)) => Some(solution),
            None => None,
        };
        let search = if full { Search::ChooseWings } else { Search::Normal };
        let longest = candidate.as_ref().map_or(usize::MAX, |s| s.moves.len());
        let mut best = match (self.solve_edges(g, facelets, search, longest), candidate) {
            (Ok(regular), Some(small)) if small.moves.len() < regular.moves.len() => small,
            (Ok(regular), _) => regular,
            (Err(_), Some(small)) => small,
            (Err(e), None) => return Err(e),
        };
        if full && g.n >= 9 && crate::rings::Ring::supports(g) {
            let longest = best.moves.len().saturating_sub(1);
            match self.attempt(g, facelets, true, Search::ChooseWidthsAndWings, longest) {
                Ok(other) => {
                    if std::env::var_os("CUBEN_TRACE").is_some() {
                        eprintln!("  fixed widths {} moves, flexible widths {} moves", best.moves.len(), other.moves.len());
                    }
                    if other.moves.len() < best.moves.len() {
                        best = other;
                    }
                }
                Err(e) if std::env::var_os("CUBEN_TRACE").is_some() => eprintln!("  flexible widths: {e}"),
                Err(_) => (),
            }
        }
        Ok(best)
    }

    /// A block turn projects to one face turn on the corners. A lifted
    /// solution attaining their exact 2x2 distance is globally shortest.
    /// Other verified block solutions compete with the ordinary search.
    fn solve_blocks(&self, g: &Geometry, f: &[u8], width: usize) -> Option<(SolutionN, bool)> {
        use crate::api::{check, solve, Input, Mode};
        let last = g.n - 1;
        let corners = g.sub_facelets(f, &[0, last]);
        if corners.iter().any(|&c| c >= 6) {
            return None;
        }
        let text = g.to_string(&corners);
        let small = check(2, Input::Facelets(&text), 2).ok()?;
        let optimal_corners = solve(&small, Mode::Normal).ok()?.moves;
        let lower = optimal_corners.len();
        let lift = |word: &[usize]| {
            let moves: Vec<_> = word.iter().map(|&m| (width - 1) * 18 + m).collect();
            let mut after = f.to_vec();
            g.apply_seq(&mut after, &moves);
            if !g.is_solved(&after) {
                return None;
            }
            let len = moves.len();
            let lens = if width == 1 { [0, 0, len] } else { [len, 0, 0] };
            Some((SolutionN { moves, lens, long_way: false }, len == lower))
        };
        if let Some(answer) = lift(&optimal_corners) {
            return Some(answer);
        }
        if 2 * width == g.n {
            return None;
        }

        // On an even cube the middle rectangle can have a rotated colour
        // scheme. Name its six colours after their current faces.
        let mut middle = g.sub_facelets(f, &[0, g.n / 2, last]);
        let mut name = [u8::MAX; 6];
        for (face, &centre) in middle.iter().skip(4).step_by(9).enumerate() {
            let slot = name.get_mut(centre as usize)?;
            if *slot != u8::MAX {
                return None;
            }
            *slot = face as u8;
        }
        for c in &mut middle {
            *c = *name.get(*c as usize)?;
        }
        crate::api::check::validate(3, &middle).ok()?;
        let cube = Cube::from_small_facelets(&g.to_string(&middle)).ok()?;
        let c3 = crate::c3::C3::from_cube(&cube)?;
        let solver = &self.six.solver4;
        let path = solver.two.solve(&solver.t3, &c3, crate::two3::Effort::NORMAL);
        let word: Vec<_> = path.into_iter().map(usize::from).collect();
        let mut answer = lift(&word)?;
        // On an odd cube every physical block turn also acts as exactly
        // one face turn on the true middle edges. The exact 3x3 distance
        // is therefore a lower bound, even for paths using other widths.
        if g.n % 2 == 1 && !answer.1 {
            use crate::opt3::{Opt3, Outcome};
            use std::sync::atomic::AtomicI64;
            let exact = Opt3::new(&solver.t3, &solver.big2);
            // Short depths usually finish outright. A longer projection
            // already gives an upper bound for every ordinary centre path.
            let budget = AtomicI64::new(if word.len() <= 12 { 250_000 } else { 8_192 });
            match exact.solve(&exact.prepare(&c3), lower, word.len() - 1, &budget) {
                Outcome::Solved(path) => {
                    let word: Vec<_> = path.into_iter().map(usize::from).collect();
                    answer = (lift(&word)?.0, true);
                }
                Outcome::None => answer.1 = true,
                Outcome::Aborted => (),
            }
        }
        Some(answer)
    }

    fn solve_edges(&self, g: &Geometry, facelets: &[u8], search: Search, longest: usize) -> Result<SolutionN, String> {
        if crate::rings::Ring::supports(g) {
            match self.attempt(g, facelets, true, search, longest) {
                Ok(solution) => return Ok(solution),
                Err(e) if std::env::var_os("CUBEN_TRACE").is_some() => eprintln!("  {e}; the long way instead"),
                Err(_) => (),
            }
        }
        self.attempt(g, facelets, false, search, longest)
    }

    /// The 3-layer turns of the penultimate orbit are wide turns on the
    /// outer 5x5x5 as well. Compare full endings on that projection before
    /// committing either set of turns on the large cube.
    fn last_two_wings(&self, inner: &[u8], outer: &[u8], p: &Params5, full: bool) -> Result<(Vec<u8>, Solution5), String> {
        let small = &crate::c5::tables5().geometry;
        let residual = |seq: &[u8]| {
            let mut f = outer.to_vec();
            for &m in seq {
                small.apply(&mut f, m as usize);
            }
            f
        };
        let mut paths = self.six.outer.reductions(inner, p)?.into_iter();
        let first = paths.next().ok_or("no penultimate wing reduction found")?;
        let base = residual(&first);
        let mut best = (first, self.six.outer.solve(&base, p)?);
        let mut seen = std::collections::HashSet::from([base]);
        let mut alternatives = Vec::new();
        for path in paths {
            let f = residual(&path);
            if seen.insert(f.clone()) {
                // Each inner slice turn can put at most four wings beside
                // their own oriented middle edge. Prefer an outer orbit
                // that already has more of these wings in place.
                let cube = crate::c5::Cube5::from_facelets(&f)?;
                let wrong = (0..24).filter(|&i| cube.c4.ep[i] != cube.mid.ep[i % 12] + 12 * (cube.mid.eo[i % 12] ^ (i / 12) as u8)).count();
                alternatives.push((path.len() + wrong.div_ceil(4), wrong, path, f));
            }
        }
        let mut ranked: Vec<_> = (0..alternatives.len()).collect();
        ranked.sort_by_key(|&i| (alternatives[i].0, alternatives[i].1, i));
        let mut compare = |ids: &[usize]| {
            if ids.is_empty() {
                return;
            }
            let roots: Vec<_> = ids.iter().map(|&i| (alternatives[i].3.clone(), alternatives[i].2.len())).collect();
            if let Ok((k, tail)) = self.six.outer.solve_any(&roots, p) {
                let prefix = &alternatives[ids[k]].2;
                if prefix.len() + tail.moves.len() < best.0.len() + best.1.moves.len() {
                    best = (prefix.clone(), tail);
                }
            }
        };
        ranked.truncate(6);
        compare(&ranked);
        if full {
            let first: Vec<_> = (0..alternatives.len().min(8)).collect();
            if first != ranked {
                compare(&first);
            }
        }
        Ok(best)
    }

    /// `matched`: the centres are only made to agree with their neighbours
    /// towards the middle (`rings`), and the edge steps bring them home.
    /// Otherwise every centre is brought home first (`axes`).
    fn attempt(&self, g: &Geometry, facelets: &[u8], matched: bool, search: Search, longest: usize) -> Result<SolutionN, String> {
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
        let flexible = search == Search::ChooseWidthsAndWings;
        let staged = self.stage_centres(g, &w.f, odd || !matched, &self.staging, flexible, longest)?;
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
                if w.moves.len() > longest {
                    return Err("the centre prefix exceeds the incumbent".into());
                }
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
            if w.moves.len() > longest {
                return Err("the wing prefix exceeds the incumbent".into());
            }
            let layers = [0, k, mid, last - k, last];
            let sub = g.sub_facelets(&w.f, &layers);
            // Without a clock: the same cube gives the same solution however busy the machine is.
            let wide = Params5 { c_keep: 6000, time_ms: 0, ..Params5::normal() };
            if k == 2 {
                let outer = g.sub_facelets(&w.f, &[0, 1, mid, last - 1, last]);
                let (prefix, ending) = self.last_two_wings(&sub, &outer, &wide, search != Search::Normal)?;
                w.apply(&prefix, &[1, k + 1]);
                w.apply(&ending.moves, &[1, 2]);
                tail = ending.lens[5];
                break;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_blocks_project_and_lift_across_different_sizes_and_widths() {
        let solver = SolverN::load();
        for (n, width) in [(8, 4), (9, 2), (21, 7), (30, 15)] {
            let g = Geometry::new(n);
            let mut f = g.solved();
            assert_eq!(block_width(n, &f), None);
            let turns = [0, 4, 8, 10, 12, 2, 7, 3];
            for m in turns {
                g.apply(&mut f, (width - 1) * 18 + m);
            }
            assert_eq!(block_width(n, &f), Some(width));
            let (solution, exact) = solver.solve_blocks(&g, &f, width).unwrap();
            assert!(exact);
            assert!(solution.moves.iter().all(|m| 1 + m / 18 == width));
            let mut after = f.clone();
            g.apply_seq(&mut after, &solution.moves);
            assert!(g.is_solved(&after));
            g.apply(&mut f, 3);
            assert_eq!(block_width(n, &f), None);
            if n == 9 {
                assert!(solver.solve_edges(&g, &f, Search::Normal, 0).is_err());
            }
        }
    }
}
