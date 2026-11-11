//! One way in for a cube of any size: check it, solve it, scramble it.
//!
//! The solvers each have their own types and tables; this module is what a
//! program that just has a cube uses. `check` turns a size and either a
//! scramble or facelets into a cube that is known to be solvable, or says
//! in words what is wrong with it. `solve` hands such a cube to the solver
//! for its size and applies the answer to the sticker model before
//! returning it. `random_state` scrambles the 2x2x2 and 3x3x3 from a
//! position drawn from all there are.
//!
//! The interface in `app/` and its local server (`anycube app`) are
//! built on this, and so is the WebAssembly build (`app/wasm`), where only
//! the 2x2x2 and 3x3x3 solvers exist.

pub mod check;
pub mod scramble;
pub mod solve;

pub use check::{check, ApiError, Checked, Input};
pub use scramble::random_state;
pub use solve::{solve, Mode, Solved};

/// Sizes that are solved without tables on disk, in a browser too.
pub const BROWSER_SIZES: [usize; 2] = [2, 3];

/// No request may ask for more than this: the sticker model of a cube has
/// 18 * (n / 2) moves of 6 * n * n entries each.
pub const HARD_MAX_SIZE: usize = 64;

/// The longest scramble accepted, in moves.
pub const MAX_SCRAMBLE_MOVES: usize = 5000;
