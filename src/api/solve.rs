//! From a checked cube to a solution, by the solver for its size.
//!
//! This is what `solve` in the command line does, without the command line:
//! the solvers are loaded once and kept, the cube goes in as stickers and
//! the answer is applied to the sticker model before it is returned.

use crate::api::check::{sticker, ApiError, Checked};
use crate::c2::T2;
use crate::c3::{C3, T3};
use crate::cube::Cube;
use crate::cuben::Geometry;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// A fraction of the time for a move or so more.
    Fast,
    /// The solver's defaults.
    Normal,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "fast" => Some(Mode::Fast),
            "normal" => Some(Mode::Normal),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Fast => "fast",
            Mode::Normal => "normal",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Solved {
    /// The moves in the numbering of the sticker model.
    pub moves: Vec<usize>,
    /// The same as text: `U R2 F' Rw 3Uw2`.
    pub solution: String,
    /// The steps of the solution in order and the moves each took; empty
    /// when the solver does not say, or when they do not add up to the whole.
    pub parts: Vec<(&'static str, usize)>,
    /// Whether no shorter solution exists.
    pub optimal: bool,
}

pub(crate) fn t2() -> &'static T2 {
    static T: OnceLock<T2> = OnceLock::new();
    T.get_or_init(T2::load)
}

pub(crate) fn t3() -> &'static T3 {
    static T: OnceLock<T3> = OnceLock::new();
    T.get_or_init(T3::new)
}

/// The 3x3x3 search on all cores, with the table of its first phase.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn two3() -> &'static crate::two3::Two3 {
    static T: OnceLock<crate::two3::Two3> = OnceLock::new();
    T.get_or_init(|| crate::two3::Two3::load(t3()))
}

/// A 3x3x3 solved with the effort of a mode.
#[cfg(not(target_arch = "wasm32"))]
fn solve3(c3: &C3, mode: Mode) -> Vec<u8> {
    use crate::two3::Effort;
    two3().solve(t3(), c3, if mode == Mode::Fast { Effort::FAST } else { Effort::NORMAL })
}

/// In the browser there is one core and no file to keep a table in: the search of `c3.rs`.
#[cfg(target_arch = "wasm32")]
fn solve3(c3: &C3, mode: Mode) -> Vec<u8> {
    t3().solve(c3, if mode == Mode::Fast { 500 } else { 30_000 }, None)
}

/// Loads the tables for a size, building any that are missing. A solve does
/// this by itself; a server calls it first so that a request does not wait.
pub fn load(size: usize) {
    match size {
        2 => {
            t2();
        }
        3 => {
            t3();
            #[cfg(not(target_arch = "wasm32"))]
            two3();
        }
        #[cfg(not(target_arch = "wasm32"))]
        n => big::load(n),
        #[cfg(target_arch = "wasm32")]
        _ => {}
    }
}

/// A cube of even size has no middle stickers to say which colour belongs
/// on which face, so the same cube can be written down in 24 ways. This
/// picks the one in which the corner at D, B and L is at home: the colours
/// are renamed, no sticker moves, and a solution of the result is a
/// solution of the cube as given.
fn renamed(n: usize, f: &[u8]) -> Vec<u8> {
    let far = -(n as i32 - 1);
    let mut name = [0u8; 6];
    for (i, &colour) in f.iter().enumerate() {
        let s = sticker(n, i);
        if s.cubie == [far, far, far] {
            name[colour as usize] = s.face as u8;
            name[(colour as usize + 3) % 6] = (s.face as u8 + 3) % 6;
        }
    }
    f.iter().map(|&c| name[c as usize]).collect()
}

/// Solves a checked cube. The solution is applied to the stickers as they
/// were given before it is returned.
pub fn solve(c: &Checked, mode: Mode) -> Result<Solved, ApiError> {
    let g = Geometry::new(c.size);
    if g.is_solved(&c.facelets) {
        return Ok(Solved { moves: Vec::new(), solution: String::new(), parts: Vec::new(), optimal: true });
    }
    // Scrambles go to the solvers as the command line gives them; a picture
    // of an even cube is first written the standard way round.
    let work = if c.size % 2 == 0 && c.scramble.is_none() { renamed(c.size, &c.facelets) } else { c.facelets.clone() };
    let (moves, parts, optimal) = run(&g, &work, mode)?;
    let mut after = c.facelets.clone();
    g.apply_seq(&mut after, &moves);
    if !g.is_solved(&after) {
        return Err(ApiError::new("solver_failed", "the solver's answer does not solve the cube; this is a bug in the solver"));
    }
    let names: Vec<String> = moves.iter().map(|&m| g.move_name(m)).collect();
    let parts: Vec<(&'static str, usize)> = parts.into_iter().filter(|p| p.1 > 0).collect();
    let whole = parts.iter().map(|p| p.1).sum::<usize>() == moves.len();
    Ok(Solved { solution: names.join(" "), moves, parts: if whole { parts } else { Vec::new() }, optimal })
}

type Answer = (Vec<usize>, Vec<(&'static str, usize)>, bool);

fn failed(e: String) -> ApiError {
    ApiError::new("solver_failed", format!("the solver gave up: {}", e))
}

fn run(g: &Geometry, f: &[u8], mode: Mode) -> Result<Answer, ApiError> {
    match g.n {
        2 => {
            let cube = Cube::from_small_facelets(&g.to_string(f)).map_err(failed)?;
            let sol = t2().solve(&cube);
            Ok((sol.iter().map(|&m| m as usize).collect(), Vec::new(), true))
        }
        3 => {
            let c3 = Cube::from_small_facelets(&g.to_string(f))
                .ok()
                .and_then(|c| c.with_home_centres())
                .and_then(|c| C3::from_cube(&c))
                .ok_or_else(|| failed("not a 3x3x3 state".into()))?;
            let sol = solve3(&c3, mode);
            Ok((sol.iter().map(|&m| m as usize).collect(), Vec::new(), false))
        }
        #[cfg(not(target_arch = "wasm32"))]
        _ => big::run(g, f, mode),
        #[cfg(target_arch = "wasm32")]
        _ => Err(ApiError::new("not_in_browser", "sizes from 4x4x4 up are solved on the server")),
    }
}

/// Sizes from 4x4x4 up: their tables are gigabytes, so they exist only
/// where there is a file system to map them from.
#[cfg(not(target_arch = "wasm32"))]
mod big {
    use super::{failed, Answer, Mode};
    use crate::api::check::ApiError;
    use crate::cube::Cube;
    use crate::cuben::Geometry;
    use crate::nnn::SolverN;
    use crate::solver::{Params, Solver};
    use crate::solver5::{Params5, Solver5};
    use crate::solver6::{Params6, Solver6};
    use crate::solver7::{Params7, Solver7};
    use std::sync::OnceLock;

    fn s4() -> &'static Solver {
        static S: OnceLock<Solver> = OnceLock::new();
        S.get_or_init(Solver::new)
    }

    fn s5() -> &'static Solver5 {
        static S: OnceLock<Solver5> = OnceLock::new();
        S.get_or_init(Solver5::load)
    }

    fn s6() -> &'static Solver6 {
        static S: OnceLock<Solver6> = OnceLock::new();
        S.get_or_init(Solver6::load)
    }

    fn s7() -> &'static Solver7 {
        static S: OnceLock<Solver7> = OnceLock::new();
        S.get_or_init(Solver7::load)
    }

    fn sn() -> &'static SolverN {
        static S: OnceLock<SolverN> = OnceLock::new();
        S.get_or_init(SolverN::load)
    }

    pub fn load(size: usize) {
        match size {
            4 => {
                s4();
            }
            5 => {
                s5();
            }
            6 => {
                s6();
            }
            7 => {
                s7();
            }
            _ => {
                sn();
            }
        }
    }

    pub fn run(g: &Geometry, f: &[u8], mode: Mode) -> Result<Answer, ApiError> {
        // A solver that panics takes this request with it, not the process.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Answer, String> {
            Ok(match g.n {
                4 => {
                    let cube = Cube::from_facelets(&g.to_string(f))?;
                    let p = if mode == Mode::Fast { Params::fast() } else { Params::default() };
                    let sol = s4().solve(&cube, &p)?;
                    let l = sol.lens;
                    let parts = vec![("centres and edge parity", l[0] + l[1]), ("edge pairing", l[2]), ("3x3x3", l[3])];
                    (sol.moves.iter().map(|&m| m as usize).collect(), parts, false)
                }
                5 => {
                    let p = if mode == Mode::Fast { Params5::fast() } else { Params5::normal() };
                    let sol = s5().solve(f, &p)?;
                    let l = sol.lens;
                    let parts = vec![("centres", l[0] + l[1]), ("edges", l[2] + l[3] + l[4]), ("3x3x3", l[5])];
                    (sol.moves.iter().map(|&m| m as usize).collect(), parts, false)
                }
                6 => {
                    let p = if mode == Mode::Fast { Params6::fast() } else { Params6::normal() };
                    let sol = s6().solve(f, &p)?;
                    let l = sol.lens;
                    let parts =
                        vec![("centre staging", l[0]), ("inner cube set-up", l[1]), ("inner reduction", l[2]), ("5x5x5", l[3])];
                    (sol.moves.iter().map(|&m| m as usize).collect(), parts, false)
                }
                7 => {
                    let p = if mode == Mode::Fast { Params7::fast() } else { Params7::normal() };
                    let sol = s7().solve(f, &p)?;
                    let l = sol.lens;
                    let parts = vec![
                        ("centre staging", l[0]),
                        ("centre matching", l[1]),
                        ("inner edges", l[2]),
                        ("outer edges", l[3]),
                        ("3x3x3", l[4]),
                    ];
                    (sol.moves.iter().map(|&m| m as usize).collect(), parts, false)
                }
                _ => {
                    // One solver for every size from 8 up; it has no modes.
                    let sol = sn().solve(g, f)?;
                    let l = sol.lens;
                    (sol.moves, vec![("centres", l[0]), ("edges", l[1]), ("3x3x3", l[2])], false)
                }
            })
        }));
        match result {
            Ok(answer) => answer.map_err(failed),
            Err(_) => Err(ApiError::new("solver_failed", "the solver stopped on this cube; this is a bug in the solver")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::check::{check, Input};

    /// These load the tables of the 2x2x2 and the 3x3x3 from the tables directory
    /// (`ANYCUBE_TABLES`), 190 MB, and build them if they are missing.
    #[test]
    fn small_cubes_from_moves_and_from_facelets() {
        for (size, scramble) in [(2, "R U2 F' R2 U F R' U'"), (3, "R U R' U' F2 D L' B U2 R D' F L2 B' U")] {
            let from_moves = check(size, Input::Moves(scramble), 3).unwrap();
            let a = solve(&from_moves, Mode::Fast).unwrap();
            assert!(!a.moves.is_empty() && a.moves.len() <= if size == 2 { 11 } else { 24 });
            assert_eq!(a.optimal, size == 2);
            let g = Geometry::new(size);
            let from_facelets = check(size, Input::Facelets(&g.to_string(&from_moves.facelets)), 3).unwrap();
            let b = solve(&from_facelets, Mode::Fast).unwrap();
            assert_eq!(a.moves.len(), b.moves.len());
        }
    }

    #[test]
    fn a_2x2x2_held_another_way_is_renamed_and_solved() {
        let g = Geometry::new(2);
        let c = check(2, Input::Moves("R U2 F' R2 U F R' U' L D"), 3).unwrap();
        let turned = g.turned(&c.facelets, &[0, 2, 2, 1]);
        let other = check(2, Input::Facelets(&g.to_string(&turned)), 3).unwrap();
        let r = renamed(2, &other.facelets);
        assert_eq!((r[14], r[18], r[23]), (3, 4, 5), "the corner at D, L and B is at home");
        assert!(solve(&other, Mode::Normal).unwrap().moves.len() <= 11);
    }

    #[test]
    fn a_solved_cube_needs_no_moves() {
        let c = check(3, Input::Moves("R R'"), 3).unwrap();
        let s = solve(&c, Mode::Normal).unwrap();
        assert!(s.moves.is_empty() && s.solution.is_empty());
    }

    #[test]
    fn modes() {
        assert_eq!(Mode::parse("fast"), Some(Mode::Fast));
        assert_eq!(Mode::parse("normal"), Some(Mode::Normal));
        assert_eq!(Mode::parse("full"), None);
        assert_eq!(Mode::Fast.name(), "fast");
    }
}
