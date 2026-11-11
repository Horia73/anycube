//! The 2x2x2 and 3x3x3 solvers for the browser.
//!
//! No binding generator: the page calls a handful of C functions and passes
//! text through the module's memory. A call that returns text returns a
//! pointer to four bytes of length (little endian) followed by UTF-8; the
//! page frees it with `free_text`.
//!
//! The answer of `check` and `solve` is lines of text:
//!
//! ```text
//! ok                      err
//! <solution>              <code>
//! <facelets>              <impossible: 0 or 1>
//! <optimal: 0 or 1>       <message>
//! ```
//!
//! (`check` leaves the solution line empty.)
//!
//! There is no file system here, so the solvers find no tables and build
//! them: a tenth of a second for the 2x2x2 and under a second for the
//! 3x3x3, once per page. Nothing is downloaded but this module.

use anycube::api::{check, solve, ApiError, Checked, Input, Mode};

/// Room for the page to write `len` bytes into.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Gives back room from `alloc`.
///
/// # Safety
/// `ptr` and `len` are those of one `alloc` call.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

/// Frees a text this module returned.
///
/// # Safety
/// `ptr` came from `check` or `solve` and is freed once.
#[no_mangle]
pub unsafe extern "C" fn free_text(ptr: *mut u8) {
    let len = u32::from_le_bytes([*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)]) as usize;
    drop(Vec::from_raw_parts(ptr, len + 4, len + 4));
}

unsafe fn text<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("\u{fffd}")
}

fn give(s: String) -> *mut u8 {
    let mut v = Vec::with_capacity(s.len() + 4);
    v.extend_from_slice(&(s.len() as u32).to_le_bytes());
    v.extend_from_slice(s.as_bytes());
    debug_assert_eq!(v.len(), v.capacity());
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Builds the tables of a size, so that the first solve does not wait.
#[no_mangle]
pub extern "C" fn prepare(size: usize) {
    if size == 2 || size == 3 {
        anycube::api::solve::load(size);
    }
}

fn checked(size: usize, kind: u32, input: &str) -> Result<Checked, ApiError> {
    // Checking is the same for every size; only solving is limited here.
    let input = if kind == 0 { Input::Moves(input) } else { Input::Facelets(input) };
    check(size, input, anycube::api::HARD_MAX_SIZE)
}

fn error(e: ApiError) -> String {
    format!("err\n{}\n{}\n{}", e.code, e.impossible() as u8, e.message.replace('\n', " "))
}

fn facelets(c: &Checked) -> String {
    c.facelets.iter().map(|&x| b"URFDLB"[x as usize] as char).collect()
}

/// Checks a cube of any size. `kind` is 0 for moves and 1 for facelets.
///
/// # Safety
/// `input` points to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn check_cube(size: usize, kind: u32, input: *const u8, len: usize) -> *mut u8 {
    give(match checked(size, kind, text(input, len)) {
        Ok(c) => format!("ok\n\n{}\n0", facelets(&c)),
        Err(e) => error(e),
    })
}

/// Solves a 2x2x2 or a 3x3x3. `mode` is 0 for fast and 1 for normal.
///
/// # Safety
/// `input` points to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn solve_cube(size: usize, kind: u32, mode: u32, input: *const u8, len: usize) -> *mut u8 {
    let mode = if mode == 0 { Mode::Fast } else { Mode::Normal };
    give(match checked(size, kind, text(input, len)).and_then(|c| solve(&c, mode).map(|s| (c, s))) {
        Ok((c, s)) => format!("ok\n{}\n{}\n{}", s.solution, facelets(&c), s.optimal as u8),
        Err(e) => error(e),
    })
}

/// A scramble that leads to a 2x2x2 or 3x3x3 picked at random from all its
/// positions, each equally likely. The page gives the randomness, as two
/// halves of a 64-bit number. The text is empty for other sizes.
#[no_mangle]
pub extern "C" fn random_state_scramble(size: usize, seed_high: u32, seed_low: u32) -> *mut u8 {
    let seed = (seed_high as u64) << 32 | seed_low as u64;
    give(anycube::api::random_state(size, seed).unwrap_or_default())
}
