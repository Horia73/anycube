// Compare the public, in-process 2x2x2 API in two release builds.
// Arguments are the input kind (moves or stickers) and the output file for exact replay comparison.
use anycube::{
    api::{self, Input, Mode},
    cuben::Geometry,
};
use std::{env, fs, hint::black_box, time::Instant};

fn main() {
    let args: Vec<_> = env::args().collect();
    assert!(args.len() == 3 && matches!(args[1].as_str(), "moves" | "stickers"));
    let stickers = args[1] == "stickers";
    let geometry = Geometry::new(2);
    let scrambles = fs::read_to_string("bench/scrambles222-100.txt").unwrap();
    let cases: Vec<_> = scrambles
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            if stickers {
                let cube = api::check(2, Input::Moves(line), 2).unwrap();
                geometry.to_string(&cube.facelets)
            } else {
                line.to_owned()
            }
        })
        .collect();
    let solve = |case: &str| {
        let input = if stickers {
            Input::Facelets(case)
        } else {
            Input::Moves(case)
        };
        api::solve(&api::check(2, input, 2).unwrap(), Mode::Normal).unwrap()
    };

    let mut words = String::new();
    for case in &cases {
        words.push_str(&solve(case).solution);
        words.push('\n');
    }
    fs::write(&args[2], words).unwrap();
    let repetitions = 240;
    let start = Instant::now();
    for _ in 0..repetitions {
        for case in &cases {
            black_box(solve(black_box(case)));
        }
    }
    let micros = start.elapsed().as_secs_f64() * 1e6 / (repetitions * cases.len()) as f64;
    println!("{micros:.4}");
}
