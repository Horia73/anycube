use cube444::cube::{format_moves, parse_moves, Cube};
use cube444::solver::{Params, Solver};
use rayon::prelude::*;
use std::io::Write;
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage:\n  cube444 solve <moves...> [search flags]   solve a scramble given as moves\n  cube444 solve --state <96 facelets> [search flags]   solve a cube given as URFDLB facelets\n  cube444 bench <scrambles.txt> [--out results.tsv] [--tpr tpr-results.tsv] [--limit N] [--parallel] [search flags]"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("solve") => solve(&args[1..]),
        Some("bench") => bench(&args[1..]),
        Some("probe333") => probe333(&args[1..]),
        Some("gen-centres") => gen_centres(),
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
        let moves: Vec<&str> = args.iter().map(String::as_str).take_while(|a| !a.starts_with("--")).collect();
        match parse_moves(&moves.join(" ")) {
            Ok(m) => Cube::from_moves(&m),
            Err(e) => fail(&e),
        }
    };
    let t0 = Instant::now();
    let solver = Solver::new();
    let t1 = Instant::now();
    match solver.solve(&cube, &params(args)) {
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

fn params(args: &[String]) -> Params {
    let mut p = Params::default();
    let num = |name: &str| opt(args, name).map(|s| s.parse::<usize>().unwrap_or_else(|_| usage()));
    if let Some(v) = num("--n1") {
        p.n1 = v;
    }
    if let Some(v) = num("--k1") {
        p.k1 = v;
    }
    if let Some(v) = num("--n2") {
        p.n2 = v;
    }
    if let Some(v) = num("--k3") {
        p.k3 = v;
    }
    if let Some(v) = num("--n3") {
        p.n3 = v;
    }
    if let Some(v) = num("--extra3") {
        p.extra3 = v as u8;
    }
    if let Some(v) = num("--exact") {
        p.exact_nodes = v as i64 * 1_000_000;
    }
    if args.iter().any(|a| a == "--split") {
        p.joint = false;
    }
    if let Some(v) = num("--joint-nodes") {
        p.joint_nodes = v as i64 * 1_000_000;
    }
    if let Some(v) = num("--far-g") {
        p.far_g = v as u8;
    }
    if let Some(v) = num("--far-k") {
        p.far_k = v;
    }
    if let Some(v) = num("--time") {
        p.time_limit_ms = v as u64;
    }
    if let Some(v) = num("--refine") {
        p.refine = v as u64;
    }
    if let Some(v) = num("--exact-cap") {
        p.exact_cap = v;
    }
    if let Some(v) = num("--probes") {
        p.probes333 = v as u64;
    }
    p
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
    let params = params(args);
    eprintln!("{:?}", params);
    let mut sums = [0usize; 5];
    let mut hist = std::collections::BTreeMap::new();
    let mut times = Vec::new();
    let mut tpr_total = 0usize;
    let run = |(i, scr): (usize, &&str)| {
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
        }
        let t = Instant::now();
        let sol = solver.solve(&cube, &params).unwrap_or_else(|e| fail(&format!("scramble {}: {}", i, e)));
        (cube, sol, t.elapsed().as_micros())
    };
    // --parallel solves several cubes at once: right for length statistics, wrong for timings.
    let parallel = args.iter().any(|a| a == "--parallel");
    let results: Vec<_> = if parallel {
        scrambles.par_iter().enumerate().map(run).collect()
    } else {
        scrambles.iter().enumerate().map(run).collect()
    };
    for (i, (cube, sol, us)) in results.iter().enumerate() {
        if let Some(t) = &tpr {
            tpr_total += t[i][5].parse::<usize>().unwrap();
        }
        times.push(*us as f64 / 1e3);
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
        "time per solve{}: mean {:.1} ms, median {:.1} ms, max {:.1} ms",
        if parallel { " (several cubes at once, not a timing)" } else { "" },
        times.iter().sum::<f64>() / n,
        times[times.len() / 2],
        times[times.len() - 1]
    );
    println!("length histogram: {:?}", hist);
    if tpr.is_some() {
        println!("TPR on the same cubes: mean length {:.3} (facelets and solutions cross-checked)", tpr_total as f64 / n);
    }
}

/// Development aid: speed of the exact 3x3x3 search on random outer-move scrambles.
fn probe333(args: &[String]) {
    use cube444::big1::Big1;
    use cube444::c3::{C3, T3};
    use cube444::opt3::{Opt3, Outcome};
    use std::sync::atomic::{AtomicI64, Ordering::Relaxed};
    let n: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(10);
    let max_len: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(15);
    let t = T3::new();
    let big = Big1::load(&t);
    let o = Opt3::new(&t, &big);
    let mut seed = 12345u64;
    let mut rnd = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for i in 0..n {
        let moves: Vec<u8> = (0..40).map(|_| (rnd() % 18) as u8).collect();
        let c3 = C3::from_cube(&Cube::from_moves(&moves)).unwrap();
        let two = t.solve(&c3, 500).len();
        let budget = AtomicI64::new(i64::MAX);
        let t0 = Instant::now();
        let root = o.prepare(&c3);
        let res = o.solve(&root, 0, max_len, &budget);
        let nodes = i64::MAX - budget.load(Relaxed);
        let what = match res {
            Outcome::Solved(s) => format!("solved in {}", s.len()),
            Outcome::None => format!("none within {}", max_len),
            Outcome::Aborted => "aborted".to_string(),
        };
        println!(
            "{:3}  two-phase {}  lower bound {}  {}  {} nodes  {:.1} ms",
            i,
            two,
            root.lower_bound(),
            what,
            nodes,
            t0.elapsed().as_secs_f64() * 1e3
        );
    }
}

/// Generates the exact centre table (2.37 GB) and checks a few lookups.
fn gen_centres() {
    use cube444::ctab::{Ctab, GOAL_RL, GOAL_UD};
    use cube444::tables::{axis_masks, Tabs};
    let tabs = Tabs::load();
    let t0 = Instant::now();
    let ctab = Ctab::load(&tabs.bp, &tabs.ranks);
    eprintln!("centre table ready in {:.1} s", t0.elapsed().as_secs_f64());
    assert_eq!(ctab.exact(&tabs.bp, &tabs.ranks, GOAL_RL, GOAL_UD), 0);
    for scr in ["Uw", "Uw Rw", "Rw U Fw' D2 Uw", "Fw R Uw2 B Rw' D Fw2 L Uw R2 Fw"] {
        let c = Cube::from_moves(&parse_moves(scr).unwrap());
        let m = axis_masks(&c);
        println!("{:40} distance {}", scr, ctab.exact(&tabs.bp, &tabs.ranks, m[2], m[0]));
    }
}
