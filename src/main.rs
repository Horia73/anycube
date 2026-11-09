use cube444::cube::{format_moves, parse_moves, Cube};
use cube444::solver::{Params, Solver};
use std::io::Write;
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage:\n  cube444 solve <moves...>          solve a scramble given as moves\n  cube444 solve --state <96 facelets> solve a cube given as URFDLB facelets\n  cube444 bench <scrambles.txt> [--out results.tsv] [--tpr tpr-results.tsv] [--limit N]"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("solve") => solve(&args[1..]),
        Some("bench") => bench(&args[1..]),
        _ => usage(),
    }
}

fn solve(args: &[String]) {
    let cube = if args.first().map(String::as_str) == Some("--state") {
        match args.get(1).map(|s| Cube::from_facelets(s)) {
            Some(Ok(c)) => c,
            Some(Err(e)) => fail(&e),
            None => usage(),
        }
    } else {
        match parse_moves(&args.join(" ")) {
            Ok(m) => Cube::from_moves(&m),
            Err(e) => fail(&e),
        }
    };
    let t0 = Instant::now();
    let solver = Solver::new();
    let t1 = Instant::now();
    match solver.solve(&cube, &Params::default()) {
        Ok(sol) => {
            println!("{}", format_moves(&sol.moves));
            eprintln!(
                "{} moves (phases {:?}), tables {:.0} ms, search {:.0} ms",
                sol.moves.len(),
                sol.lens,
                (t1 - t0).as_secs_f64() * 1e3,
                t1.elapsed().as_secs_f64() * 1e3
            );
        }
        Err(e) => fail(&e),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {}", msg);
    std::process::exit(1)
}

fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn bench(args: &[String]) {
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let limit = opt(args, "--limit").map(|s| s.parse::<usize>().unwrap_or_else(|_| usage()));
    let scrambles: Vec<&str> =
        text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit.unwrap_or(usize::MAX)).collect();
    let tpr: Option<Vec<Vec<String>>> = opt(args, "--tpr").map(|p| {
        std::fs::read_to_string(p)
            .unwrap_or_else(|e| fail(&format!("{}: {}", p, e)))
            .lines()
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect()
    });
    let mut out = opt(args, "--out").map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| fail(&format!("{}: {}", p, e))))
    });

    let t0 = Instant::now();
    let solver = Solver::new();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let params = Params::default();
    let mut sums = [0usize; 5];
    let mut hist = std::collections::BTreeMap::new();
    let mut times = Vec::new();
    let mut tpr_total = 0usize;
    for (i, scr) in scrambles.iter().enumerate() {
        let moves = parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let cube = Cube::from_moves(&moves);
        if let Some(t) = &tpr {
            let row = &t[i];
            if row[7] != cube.to_facelets() {
                fail(&format!("scramble {}: facelets differ from TPR", i));
            }
            let mut c = cube;
            c.apply_seq(&parse_moves(&row[8]).unwrap_or_else(|e| fail(&e)));
            if !c.is_solved() {
                fail(&format!("scramble {}: TPR's solution does not solve our model", i));
            }
            tpr_total += row[5].parse::<usize>().unwrap();
        }
        let t = Instant::now();
        let sol = solver.solve(&cube, &params).unwrap_or_else(|e| fail(&format!("scramble {}: {}", i, e)));
        let us = t.elapsed().as_micros();
        times.push(us as f64 / 1e3);
        for k in 0..4 {
            sums[k] += sol.lens[k];
        }
        sums[4] += sol.moves.len();
        *hist.entry(sol.moves.len()).or_insert(0usize) += 1;
        if let Some(o) = out.as_mut() {
            writeln!(
                o,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                i,
                sol.lens[0],
                sol.lens[1],
                sol.lens[2],
                sol.lens[3],
                sol.moves.len(),
                us,
                cube.to_facelets(),
                format_moves(&sol.moves)
            )
            .unwrap();
        }
    }
    let n = scrambles.len() as f64;
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("solved {} cubes, every solution verified", scrambles.len());
    println!(
        "mean length {:.3}  (phase 1 {:.3}, phase 2 {:.3}, phase 3 {:.3}, 3x3x3 {:.3})",
        sums[4] as f64 / n,
        sums[0] as f64 / n,
        sums[1] as f64 / n,
        sums[2] as f64 / n,
        sums[3] as f64 / n
    );
    println!(
        "time per solve: mean {:.1} ms, median {:.1} ms, max {:.1} ms",
        times.iter().sum::<f64>() / n,
        times[times.len() / 2],
        times[times.len() - 1]
    );
    println!("length histogram: {:?}", hist);
    if tpr.is_some() {
        println!("TPR on the same cubes: mean length {:.3} (facelets and solutions cross-checked)", tpr_total as f64 / n);
    }
}
