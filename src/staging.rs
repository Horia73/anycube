//! Staging the centres of a cube of any size: every centre onto a face of
//! its own axis, several candidates at a time.
//!
//! The steps are the ones `nnn` describes: ring by ring from the middle
//! outwards, the rows of a ring formed two orbits at a time (each a 6x6x6
//! or a 7x7x7 made of layers), then the ring onto its faces as the x and t
//! centres of a 5x5x5; first for one axis, then for the other two.
//!
//! What the 7x7x7 solver learnt about its own staging is applied here:
//!
//! - Several cubes are carried from step to step, and after each step the
//!   ones shortest in total go on. A step whose result is a move longer may
//!   leave the next one two moves less to do.
//! - A step that is a beam search is one search from all the cubes carried
//!   (`search::beam_roots`), not one search for each: the cubes share the
//!   width of the beam and it goes to the ones that turn out well. Carrying
//!   more cubes then costs next to nothing in those steps.
//! - Which axis is staged first is not fixed. The cube starts as three
//!   candidates, one for each axis, and when the first axis is done every
//!   candidate goes on with either of the other two. On the 7x7x7 this
//!   choice, made inside the searches, was worth more than a move.

use crate::cuben::Geometry;
use crate::nnn::SolverN;
use crate::orbits::CentreOrbit;
use rayon::prelude::*;

/// How wide the staging of a large cube goes.
#[derive(Clone, Debug)]
pub struct Staging {
    /// Cubes carried from step to step, and how many continuations of each stay.
    pub width: usize,
    pub fan: usize,
    /// States per level of the beam searches.
    pub beam: usize,
    /// Whether the order of the axes is chosen.
    pub axes: bool,
    /// Whether the rows of the first axis are formed with every move (the
    /// 7x7x7 solver's steps 1 and 2 in one) or only with the turns that
    /// keep the centres inside them on their faces.
    pub merged: bool,
}

impl Staging {
    pub fn normal() -> Staging {
        Staging { width: 8, fan: 4, beam: 20_000, axes: true, merged: false }
    }

    /// One cube, one axis order: the staging as it was before candidates.
    pub fn single() -> Staging {
        Staging { width: 1, fan: 1, beam: 20_000, axes: false, merged: false }
    }
}

/// A cube part of the way through its staging, as its frame sees it, and
/// its moves so far in the moves of the cube the staging started from.
#[derive(Clone)]
struct Cand {
    f: Vec<u8>,
    moves: Vec<usize>,
    /// The move of the cube the staging started from for each move of this frame.
    to_start: Vec<usize>,
}

/// One step: a small cube made of some layers, and how deep its turns reach in ours.
enum Step {
    /// A ring onto its faces, as the x and t centres of a 5x5x5.
    Faces([usize; 5], [usize; 2]),
    /// Rows formed where the two obliques of a pair have no layer between them: a 6x6x6.
    Rows6([usize; 6], [usize; 3]),
    /// Rows formed, as a 7x7x7.
    Rows7([usize; 7], [usize; 3]),
}

/// The cube turned as a whole (0 = x, 1 = y, 2 = z). Where a cell of each
/// face is known to show that face's colour, the colours are renamed so
/// that it does again.
fn seen(g: &Geometry, f: &[u8], turns: &[usize], named: Option<(usize, usize)>) -> Vec<u8> {
    let turned = g.turned(f, turns);
    let Some((row, col)) = named else { return turned };
    let n = g.n;
    let mut name = [0u8; 6];
    for face in 0..6 {
        name[turned[face * n * n + row * n + col] as usize] = face as u8;
    }
    turned.iter().map(|&c| name[c as usize]).collect()
}

fn mask(o: &CentreOrbit, f: &[u8], axis: u8) -> u32 {
    let col = o.colours(f);
    (0..24).fold(0u32, |m, i| m | ((col[i] % 3 == axis) as u32) << i)
}

/// True if on every face the centres are all of the two colours of one
/// axis, and no two pairs of faces share an axis.
pub fn is_staged(g: &Geometry, f: &[u8]) -> bool {
    let n = g.n;
    let axis_of = |face: usize| {
        let mut axes = (1..n - 1).flat_map(|r| (1..n - 1).map(move |c| f[face * n * n + r * n + c] % 3));
        let first = axes.next()?;
        axes.all(|a| a == first).then_some(first)
    };
    let axes: Vec<Option<u8>> = (0..6).map(axis_of).collect();
    axes.iter().all(Option::is_some) && (0..3).all(|k| axes[k] == axes[k + 3]) && axes[0] != axes[1] && axes[1] != axes[2] && axes[0] != axes[2]
}

impl SolverN {
    /// The staging of a whole cube as given (`stage_centres`): for an odd
    /// cube onto the faces of the centres' own colours.
    pub fn stage(&self, g: &Geometry, f: &[u8]) -> Result<Vec<usize>, String> {
        let named = (g.n % 2 == 1).then_some((g.n / 2, g.n / 2));
        self.stage_centres(g, f, named, &self.staging)
    }

    /// Moves that bring every centre of the cube onto a face of its own
    /// axis, in the moves of the cube as given here.
    ///
    /// `named`: a cell that shows its face's colour on every face (the
    /// middle of an odd cube, a solved innermost centre of an even one). The
    /// centres then go onto the faces of their own colours' axes. Without
    /// one (an even cube with nothing solved yet) any colour axis may end
    /// on any pair of faces, and the cube is held accordingly afterwards.
    pub(crate) fn stage_centres(&self, g: &Geometry, f: &[u8], named: Option<(usize, usize)>, p: &Staging) -> Result<Vec<usize>, String> {
        let n = g.n;
        let trace = std::env::var_os("CUBEN_TRACE").is_some();
        let (odd, half, last) = (n % 2 == 1, n / 2, n - 1);
        let (first, mid) = if odd { (half - 1, half) } else { (half - 1, half - 1) };
        let mut steps = Vec::new();
        if odd {
            steps.push(Step::Faces([0, half - 1, half, half + 1, last], [1, half]));
        }
        for a in (1..first).rev() {
            for b in (a + 1..=first).rev() {
                let depths = [1, a + 1, b + 1];
                match !odd && b == first {
                    true => steps.push(Step::Rows6([0, a, b, last - b, last - a, last], depths)),
                    false => steps.push(Step::Rows7([0, a, b, mid, last - b, last - a, last], depths)),
                }
            }
            steps.push(Step::Faces([0, a, mid, last - a, last], [1, a + 1]));
        }
        let start = |turns: &[usize]| Cand { f: seen(g, f, turns, named), moves: Vec::new(), to_start: g.frame_moves(turns) };
        let mut cands = match p.axes {
            true => vec![start(&[]), start(&[1]), start(&[2])],
            false => vec![start(&[])],
        };
        for pass in 0..2 {
            if pass == 1 && p.axes {
                // Either of the other two axes may come second.
                let quarter = g.frame_moves(&[0]);
                let other: Vec<Cand> = cands
                    .iter()
                    .map(|c| Cand { f: seen(g, &c.f, &[0], named), moves: c.moves.clone(), to_start: quarter.iter().map(|&m| c.to_start[m]).collect() })
                    .collect();
                cands.extend(other);
            }
            for (number, step) in steps.iter().enumerate() {
                cands = self.advance(g, cands, step, pass, p)?;
                if trace {
                    let lens: Vec<String> = cands.iter().map(|c| c.moves.len().to_string()).collect();
                    eprintln!("  staging, axis {}, step {} of {}: {}", pass + 1, number + 1, steps.len(), lens.join(" "));
                }
            }
        }
        cands.into_iter().min_by_key(|c| c.moves.len()).map(|c| c.moves).ok_or_else(|| "no staging found".to_string())
    }

    /// One step for all the cubes carried: the continuations shortest in total.
    fn advance(&self, g: &Geometry, cands: Vec<Cand>, step: &Step, pass: usize, p: &Staging) -> Result<Vec<Cand>, String> {
        let stage5 = &self.s5.stage;
        let least = cands.iter().map(|c| c.moves.len()).min().unwrap_or(0);
        // Evenly from all the solutions of an exact search, so that they differ from the first move on.
        let thin = |mut sols: Vec<Vec<u8>>| {
            if sols.len() > p.fan {
                let all = sols.len();
                sols = (0..p.fan).map(|i| std::mem::take(&mut sols[i * all / p.fan])).collect();
            }
            sols
        };
        // (the cube continued, moves of the small cube).
        let (found, depths): (Vec<(usize, Vec<u8>)>, &[usize]) = match step {
            Step::Faces(layers, depths) => {
                let (x5, t5) = (&self.orbits5[0], &self.orbits5[1]);
                let found = cands
                    .par_iter()
                    .enumerate()
                    .flat_map_iter(|(i, c)| {
                        let sub = g.sub_facelets(&c.f, layers);
                        let sols = match pass {
                            0 => thin(stage5.phase_a(mask(x5, &sub, 1), mask(t5, &sub, 1), 0, p.fan, None)),
                            // Of the two parities of the wings either will do here.
                            _ => vec![stage5.phase_b(mask(x5, &sub, 0), mask(t5, &sub, 0), false)],
                        };
                        sols.into_iter().map(move |sol| (i, sol))
                    })
                    .collect();
                (found, &depths[..])
            }
            Step::Rows6(layers, depths) => {
                let found = cands
                    .par_iter()
                    .enumerate()
                    .flat_map_iter(|(i, c)| {
                        let sub = g.sub_facelets(&c.f, layers);
                        let sols: Vec<Vec<u8>> = match pass {
                            0 => self.s6.stage_steps(stage5, &sub, p.fan.max(4), 0..1).into_iter().map(|x| x.moves).collect(),
                            _ => self.s6.pair_ud(&sub, p.fan),
                        };
                        sols.into_iter().take(p.fan).map(move |sol| (i, sol))
                    })
                    .collect();
                (found, &depths[..])
            }
            Step::Rows7(layers, depths) => {
                let subs: Vec<(Vec<u8>, usize)> = cands.iter().map(|c| (g.sub_facelets(&c.f, layers), c.moves.len() - least)).collect();
                // A beam that finds nothing is rare; a wider one then does.
                let found = [(1, 0), (10, 1), (50, 2)]
                    .iter()
                    .map(|&(times, salt)| self.s7.rows(stage5, pass, &subs, p.beam * times, p.fan * cands.len(), salt, p.merged))
                    .find(|found| !found.is_empty())
                    .unwrap_or_default();
                (found, &depths[..])
            }
        };
        if found.is_empty() {
            return Err(format!("no staging found for a step {} layers deep", depths[depths.len() - 1]));
        }
        let mut next: Vec<(usize, Cand)> = found
            .par_iter()
            .map(|(i, sol)| {
                let mut c = cands[*i].clone();
                for &m in sol {
                    let (face, d, power) = (m as usize / 3 % 6, m as usize / 18, m as usize % 3);
                    let ours = (face + 6 * (depths[d] - 1)) * 3 + power;
                    g.apply(&mut c.f, ours);
                    c.moves.push(c.to_start[ours]);
                }
                (*i, c)
            })
            .collect();
        // Of the continuations of one cube the `fan` shortest stay, then the `width` shortest of all.
        next.sort_by(|x, y| (x.0, x.1.moves.len()).cmp(&(y.0, y.1.moves.len())));
        let mut run = (usize::MAX, 0);
        next.retain(|x| {
            if x.0 != run.0 {
                run = (x.0, 0);
            }
            run.1 += 1;
            run.1 <= p.fan
        });
        let mut next: Vec<Cand> = next.into_iter().map(|x| x.1).collect();
        next.sort_by(|x, y| (x.moves.len(), &x.f).cmp(&(y.moves.len(), &y.f)));
        next.dedup_by(|x, y| x.f == y.f);
        next.truncate(p.width);
        Ok(next)
    }
}
