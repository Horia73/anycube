//! The browser's solver and the native one on the same cubes.
//!
//! Both are this crate compiled twice. The native build has the table of
//! corners and U/D edges for the second phase of the 3x3x3 and a file
//! system to keep tables in; the WebAssembly build has neither. Each must
//! give a solution that solves the cube on the sticker model, the 2x2x2
//! ones of the same (shortest) length.
//!
//! Needs `node` and the WebAssembly build:
//!
//! ```text
//! cargo build --release -p anycube-wasm --target wasm32-unknown-unknown
//! ```
//!
//! Where either is missing the test says so and passes, unless
//! `ANYCUBE_REQUIRE_WASM` is set.

use anycube::cuben::Geometry;
use anycube::api::{check, solve, Input, Mode};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Random-move scrambles, the same every run.
fn scrambles(size: usize, count: usize, length: usize) -> Vec<String> {
    let g = Geometry::new(size);
    let mut seed = 0x9E3779B97F4A7C15u64 ^ size as u64;
    let mut next = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    (0..count)
        .map(|_| (0..length).map(|_| g.move_name(next(g.move_count()))).collect::<Vec<_>>().join(" "))
        .collect()
}

#[test]
fn wasm_and_native_solve_the_same_cubes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let module = root.join("target/wasm32-unknown-unknown/release/anycube_wasm.wasm");
    let script = root.join("app/tests/wasm-solve.mjs");
    let required = std::env::var_os("ANYCUBE_REQUIRE_WASM").is_some();
    let node = Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !node || !module.is_file() {
        let why = if node { "the WebAssembly build is not there (cargo build --release -p anycube-wasm --target wasm32-unknown-unknown)" } else { "node is not installed" };
        assert!(!required, "{}", why);
        eprintln!("skipped: {}", why);
        return;
    }

    // Each cube twice: as its scramble and as its stickers.
    let mut cubes = Vec::new();
    for (size, length) in [(2, 14), (3, 30)] {
        let g = Geometry::new(size);
        for scramble in scrambles(size, 12, length) {
            let c = check(size, Input::Moves(&scramble), 3).unwrap();
            cubes.push((size, "facelets", g.to_string(&c.facelets), c.facelets.clone()));
            cubes.push((size, "moves", scramble, c.facelets));
        }
    }

    let mut child = Command::new("node")
        .arg(&script)
        .arg(&module)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("node starts");
    {
        let mut stdin = child.stdin.take().unwrap();
        for (size, kind, input, _) in &cubes {
            writeln!(stdin, "{}\t{}\tfast\t{}", size, kind, input).unwrap();
        }
    }
    let out = child.wait_with_output().expect("node runs");
    assert!(out.status.success(), "the WebAssembly solver failed");
    let answers: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();
    assert_eq!(answers.len(), cubes.len());

    for ((size, kind, input, facelets), answer) in cubes.iter().zip(answers) {
        let g = Geometry::new(*size);
        // The browser's answer, applied to the stickers here.
        let wasm = answer.strip_prefix("ok\t").unwrap_or_else(|| panic!("{} {}: {}", size, kind, answer));
        let moves = g.parse_moves(wasm).unwrap();
        let mut f = facelets.clone();
        g.apply_seq(&mut f, &moves);
        assert!(g.is_solved(&f), "the browser's solution of a {0}x{0}x{0} given as {1} does not solve it", size, kind);
        // The native answer; `solve` applies it to the stickers itself.
        let input = if *kind == "moves" { Input::Moves(input) } else { Input::Facelets(input) };
        let native = solve(&check(*size, input, 3).unwrap(), Mode::Fast).unwrap();
        if *size == 2 {
            assert_eq!(moves.len(), native.moves.len(), "both are shortest solutions");
            assert!(moves.len() <= 11);
        } else {
            assert!(moves.len() <= 24 && native.moves.len() <= 24, "{} and {} moves", moves.len(), native.moves.len());
        }
    }
}
