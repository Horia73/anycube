//! `anycube solve --state`: stickers that are not a real cube are refused
//! before any solver sees them, and what is printed solves the cube.
//!
//! Before, a 3x3x3 with one edge flipped got the solution of the cube
//! without the flip, with exit status 0: the 3x3x3 is read into the 4x4x4
//! model, where an edge turned over is two wings exchanged, which is a cube.

use std::path::PathBuf;
use std::process::{Command, Output};

fn anycube(args: &[&str]) -> Output {
    // Tables go where the caller keeps them, else to a directory of the test's own.
    let tables = std::env::var_os("ANYCUBE_TABLES")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("anycube-cli-tests"));
    Command::new(env!("CARGO_BIN_EXE_anycube"))
        .args(args)
        .env("ANYCUBE_TABLES", tables)
        .output()
        .expect("run anycube")
}

fn facelets(size: usize, moves: &str) -> String {
    let out = anycube(&["facelets", "--size", &size.to_string(), moves]);
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn solved(n: usize) -> String {
    "URFDLB".chars().flat_map(|c| std::iter::repeat(c).take(n * n)).collect()
}

fn swapped(s: &str, pairs: &[(usize, usize)]) -> String {
    let mut b = s.as_bytes().to_vec();
    for &(i, j) in pairs {
        b.swap(i, j);
    }
    String::from_utf8(b).unwrap()
}

/// Refused with a message that contains `why`, and nothing on standard output.
fn refused(state: &str, why: &str) {
    let out = anycube(&["solve", "--state", state]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "accepted {}: {}", state, String::from_utf8_lossy(&out.stdout));
    assert!(out.stdout.is_empty(), "printed a solution for {}", state);
    assert!(err.contains(why), "{}: expected '{}' in '{}'", state, why, err);
}

// Facelet indices: face * n * n + row * n + column, faces in URFDLB order.
const UF3: (usize, usize) = (7, 19);
const UR3: (usize, usize) = (5, 10);
const URF3: [usize; 3] = [8, 9, 20];

#[test]
fn a_3x3_with_one_edge_flipped_is_refused() {
    refused(&swapped(&solved(3), &[UF3]), "flipped");
    let scrambled = facelets(3, "R U R' F2 D L");
    refused(&swapped(&scrambled, &[UF3]), "flipped");
}

#[test]
fn a_3x3_with_two_edges_exchanged_is_refused() {
    // UF and UR change places, each the right way up: an odd permutation of
    // the edges under an even one of the corners.
    refused(&swapped(&solved(3), &[(UF3.0, UR3.0), (UF3.1, UR3.1)]), "exchanged");
}

#[test]
fn a_3x3_with_one_corner_twisted_is_refused() {
    let mut b = solved(3).into_bytes();
    let [a, c, d] = URF3;
    (b[a], b[c], b[d]) = (b[d], b[a], b[c]);
    refused(&String::from_utf8(b).unwrap(), "twisted");
}

#[test]
fn a_5x5_with_one_middle_edge_flipped_is_refused() {
    // U row 4 column 2 and F row 0 column 2: the middle edge UF.
    refused(&swapped(&solved(5), &[(22, 52)]), "flipped");
    let scrambled = facelets(5, "Rw U Lw F2 Dw L B' Uw2 R");
    refused(&swapped(&scrambled, &[(22, 52)]), "flipped");
}

#[test]
fn a_real_2x2_is_solved_and_the_solution_checked() {
    let state = facelets(2, "R U F' L2 D B2 R'");
    let out = anycube(&["solve", "--state", &state]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let solution = String::from_utf8(out.stdout).unwrap();
    let after = facelets(2, &format!("R U F' L2 D B2 R' {}", solution.trim()));
    assert!(after.as_bytes().chunks(4).all(|f| f.iter().all(|&c| c == f[0])), "{}", after);
}

#[test]
fn shorter_accepts_stickers_and_replays_the_printed_solution() {
    let scramble = "Rw U Fw2";
    let state = facelets(4, scramble);
    let out = anycube(&["solve", "--state", &state, "--mode", "shorter"]);
    let details = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && details.contains("projected 3x3x3"), "{details}");
    let solution = String::from_utf8(out.stdout).unwrap();
    let after = facelets(4, &format!("{scramble} {}", solution.trim()));
    assert!(after.as_bytes().chunks(16).all(|face| face.iter().all(|&c| c == face[0])));
}
