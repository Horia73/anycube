use cubenxnxn::cube::{format_moves, parse_moves, Cube};
use cubenxnxn::solver::{Params, Solver};
use rayon::prelude::*;
use std::io::Write;
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage:\n  cubenxnxn solve <moves...> [--size 2|3|4|5] [search flags]   solve a scramble given as moves (4x4x4 unless --size says otherwise)\n  cubenxnxn solve --state <24, 54, 96 or 150 facelets> [search flags]   solve a cube from 2x2x2 to 5x5x5 given as URFDLB facelets\n  cubenxnxn bench <scrambles.txt> [--out results.tsv] [--tpr tpr-results.tsv] [--limit N] [--parallel] [search flags]\n  cubenxnxn bench5 <scrambles.txt> [--out results.tsv] [--limit N] [search flags]   the same for 5x5x5 scrambles\n  cubenxnxn facelets --size N <moves...>   print the facelets of a scrambled cube of any size from 2 to 7\n\nsearch flags start with --mode fast|normal|full; see README.md for the rest"
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
        Some("startup") => startup(),
        Some("facelets") => facelets(&args[1..]),
        Some("verify") => verify(&args[1..]),
        Some("scrambles") => scrambles(&args[1..]),
        Some("stage6") => stage6(&args[1..]),
        Some("bench6") => bench6(&args[1..]),
        Some("stage7") => stage7(&args[1..]),
        Some("bench7") => bench7(&args[1..]),
        Some("stage5") => stage5(&args[1..]),
        Some("centres5") => centres5(&args[1..]),
        Some("bench5") => bench5(&args[1..]),
        Some("selftest5") => cubenxnxn::edges5::Edges5::load().self_test(),
        Some("gen-edges5") => {
            let t = Instant::now();
            cubenxnxn::edges5::Edges5::load();
            eprintln!("5x5x5 edge tables ready in {:.1} s", t.elapsed().as_secs_f64());
        }
        _ => usage(),
    }
}

fn solve(args: &[String]) {
    let state = opt(args, "--state");
    let size = match (opt(args, "--size"), state.map(|s| s.trim().len())) {
        (Some(n), _) => n.parse::<usize>().unwrap_or_else(|_| usage()),
        (None, Some(24)) => 2,
        (None, Some(54)) => 3,
        (None, Some(150)) => 5,
        _ => 4,
    };
    if size == 5 {
        return solve5(state, args);
    }
    let cube = match state {
        Some(s) if size == 4 => Cube::from_facelets(s).unwrap_or_else(|e| fail(&e)),
        Some(s) => Cube::from_small_facelets(s).unwrap_or_else(|e| fail(&e)),
        None => {
            let moves = parse_moves(&move_text(args)).unwrap_or_else(|e| fail(&e));
            if size < 4 && moves.iter().any(|&m| m >= 18) {
                fail("wide moves need a 4x4x4");
            }
            Cube::from_moves(&moves)
        }
    };
    match size {
        2 => solve2(&cube),
        3 => solve3(&cube, args),
        4 => solve4(&cube, args),
        _ => fail("supported sizes: 2, 3, 4 and 5"),
    }
}

/// The moves on a command line: everything that is neither a flag nor a flag's value.
fn move_text(args: &[String]) -> String {
    let mut tokens: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            i += if matches!(args[i].as_str(), "--split" | "--parallel") { 1 } else { 2 };
        } else {
            tokens.push(&args[i]);
            i += 1;
        }
    }
    tokens.join(" ")
}

fn solve5(state: Option<&str>, args: &[String]) {
    use cubenxnxn::c5::tables5;
    let g = &tables5().geometry;
    let facelets = match state {
        Some(s) => g.from_string(s).unwrap_or_else(|e| fail(&e)),
        None => {
            let moves = g.parse_moves(&move_text(args)).unwrap_or_else(|e| fail(&e));
            let mut f = g.solved();
            g.apply_seq(&mut f, &moves);
            f
        }
    };
    let p = params5(args);
    let t0 = Instant::now();
    let solver = cubenxnxn::solver5::Solver5::load();
    let t1 = Instant::now();
    match solver.solve(&facelets, &p) {
        Ok(sol) => {
            let names: Vec<String> = sol.moves.iter().map(|&m| g.move_name(m as usize)).collect();
            println!("{}", names.join(" "));
            eprintln!(
                "{} moves (centres {} + {}, edges {} + {} + {}, 3x3x3 {}), tables {:.0} ms, search {:.0} ms",
                sol.moves.len(),
                sol.lens[0],
                sol.lens[1],
                sol.lens[2],
                sol.lens[3],
                sol.lens[4],
                sol.lens[5],
                (t1 - t0).as_secs_f64() * 1e3,
                t1.elapsed().as_secs_f64() * 1e3
            );
        }
        Err(e) => fail(&e),
    }
}

fn params5(args: &[String]) -> cubenxnxn::solver5::Params5 {
    use cubenxnxn::solver5::Params5;
    let d = match opt(args, "--mode") {
        Some("fast") => Params5::fast(),
        Some("full") => Params5::full(),
        Some("normal") | None => Params5::normal(),
        Some(_) => usage(),
    };
    let num = |name: &str, default: usize| {
        opt(args, name).map_or(default, |s| s.parse::<usize>().unwrap_or_else(|_| usage()))
    };
    Params5 {
        staged: num("--staged", d.staged),
        stage_extra: num("--extra", d.stage_extra),
        joint_extra: match opt(args, "--joint") {
            None => d.joint_extra,
            Some("off") => None,
            Some(n) => Some(n.parse().unwrap_or_else(|_| usage())),
        },
        labellings: num("--labellings", d.labellings),
        c_keep: num("--c-keep", d.c_keep),
        p_keep: num("--p-keep", d.p_keep),
        e_keep: num("--e-keep", d.e_keep),
        pool: num("--pool", d.pool),
        probes: num("--probes", d.probes as usize) as u64,
        time_ms: num("--time", d.time_ms as usize) as u64,
    }
}

fn solve2(cube: &Cube) {
    let t0 = Instant::now();
    let t = cubenxnxn::c2::T2::load();
    let sol = t.solve(cube);
    println!("{}", format_moves(&sol));
    eprintln!("{} moves, optimal, {:.0} ms", sol.len(), t0.elapsed().as_secs_f64() * 1e3);
}

fn solve3(cube: &Cube, args: &[String]) {
    use cubenxnxn::c3::{C3, T3};
    let c3 = cube
        .with_home_centres()
        .and_then(|c| C3::from_cube(&c))
        .unwrap_or_else(|| fail("not a 3x3x3 state"));
    let mode = opt(args, "--mode").unwrap_or("normal");
    let t0 = Instant::now();
    let t = T3::new();
    let probes = if mode == "fast" { 500 } else { 30_000 };
    let mut sol = t.solve(&c3, probes, None);
    let mut note = "two-phase search".to_string();
    if mode == "full" {
        // Exact search on all cores; falls back to the two-phase solution at the time limit.
        let secs = opt(args, "--time").and_then(|s| s.parse::<u64>().ok()).unwrap_or(120_000);
        let big = cubenxnxn::big2::Big2::load(&t);
        let o = cubenxnxn::opt3::Opt3::new(&t, &big);
        let res = o.solve_optimal(&o.prepare(&c3), Instant::now() + std::time::Duration::from_millis(secs));
        note = match res.solution {
            Some(s) => {
                sol = s;
                "optimal".to_string()
            }
            None => format!("not proven optimal: no solution shorter than {} moves", res.proven_lower_bound),
        };
    } else if mode != "fast" && mode != "normal" {
        usage();
    }
    println!("{}", format_moves(&sol));
    eprintln!("{} moves, {}, {:.0} ms", sol.len(), note, t0.elapsed().as_secs_f64() * 1e3);
}

fn solve4(cube: &Cube, args: &[String]) {
    let t0 = Instant::now();
    let solver = Solver::new();
    let t1 = Instant::now();
    match solver.solve(cube, &params(args)) {
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
    let mut p = match opt(args, "--mode") {
        None | Some("normal") => Params::default(),
        Some("fast") => Params::fast(),
        Some("full") => Params::full(),
        Some(_) => usage(),
    };
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
    use cubenxnxn::big2::Big2;
    use cubenxnxn::c3::{C3, T3};
    use cubenxnxn::opt3::{Opt3, Outcome};
    use std::sync::atomic::{AtomicI64, Ordering::Relaxed};
    let n: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(10);
    let max_len: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(15);
    let t = T3::new();
    let big = Big2::load(&t);
    let o = Opt3::new(&t, &big);
    let mut seed = 12345u64;
    let mut rnd = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for i in 0..n {
        let moves: Vec<u8> = (0..40).map(|_| (rnd() % 18) as u8).collect();
        let c3 = C3::from_cube(&Cube::from_moves(&moves)).unwrap();
        let two = t.solve(&c3, 500, None).len();
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
    use cubenxnxn::ctab::{Ctab, GOAL_RL, GOAL_UD};
    use cubenxnxn::tables::{axis_masks, Tabs};
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

/// Development aid: where the start-up time goes.
fn startup() {
    use cubenxnxn::tables::Tabs;
    let lap = |name: &str, t: Instant| eprintln!("{:28} {:7.1} ms", name, t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    cubenxnxn::cube::tables();
    lap("cube model", t);
    let t = Instant::now();
    let tabs = Tabs::load();
    lap("phase tables", t);
    let t = Instant::now();
    let t3 = cubenxnxn::c3::T3::new();
    lap("3x3x3 tables", t);
    let t = Instant::now();
    let _b1 = cubenxnxn::big1::Big1::load(&t3);
    lap("domino table (map)", t);
    let t = Instant::now();
    let _b2 = cubenxnxn::big2::Big2::load(&t3);
    lap("slice table (map + small)", t);
    let t = Instant::now();
    let _c = cubenxnxn::ctab::Ctab::load(&tabs.bp, &tabs.ranks);
    lap("centre table (map + index)", t);
    let t = Instant::now();
    let _n = cubenxnxn::ctab::Near::new(&tabs.bp);
    lap("near-goal centre table", t);
}

/// Prints the facelets of a scrambled cube of any size.
fn facelets(args: &[String]) {
    let n = opt(args, "--size").and_then(|s| s.parse::<usize>().ok()).unwrap_or_else(|| usage());
    if !(2..=7).contains(&n) {
        fail("supported sizes: 2 to 7");
    }
    let g = cubenxnxn::cuben::Geometry::new(n);
    let tokens: Vec<&str> =
        args.iter().map(String::as_str).filter(|a| !a.starts_with("--") && *a != n.to_string()).collect();
    let moves = g.parse_moves(&tokens.join(" ")).unwrap_or_else(|e| fail(&e));
    let mut f = g.solved();
    g.apply_seq(&mut f, &moves);
    println!("{}", g.to_string(&f));
}

/// Checks solutions of any size: each line of the file is `scramble<TAB>solution`.
/// With `--facelets`, prints the scrambled facelets of each line instead.
fn verify(args: &[String]) {
    let n = opt(args, "--size").and_then(|s| s.parse::<usize>().ok()).unwrap_or_else(|| usage());
    let path = args.iter().find(|a| !a.starts_with("--") && **a != n.to_string()).unwrap_or_else(|| usage());
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let g = cubenxnxn::cuben::Geometry::new(n);
    let only_facelets = args.iter().any(|a| a == "--facelets");
    let (mut ok, mut total, mut moves) = (0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.split('\t');
        let scramble = g.parse_moves(parts.next().unwrap_or("")).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        if only_facelets {
            println!("{}", g.to_string(&f));
            continue;
        }
        // Dots separate phases in some solvers' output.
        let solution = parts.next().unwrap_or("").replace('.', " ");
        let solution = g.parse_moves(&solution).unwrap_or_else(|e| fail(&e));
        g.apply_seq(&mut f, &solution);
        total += 1;
        moves += solution.len();
        if g.is_solved(&f) {
            ok += 1;
        } else {
            println!("line {}: not solved", total);
        }
    }
    if !only_facelets {
        println!("{} of {} solved, mean length {:.3}", ok, total, moves as f64 / total.max(1) as f64);
    }
}

/// 5x5x5 centre staging on a scramble file: lengths and times.
fn stage5(args: &[String]) {
    use cubenxnxn::c5::{tables5, Cube5};
    use cubenxnxn::stage5::Stage5;
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (limit, extra, per_job) = (num("--limit", usize::MAX), num("--extra", 1), num("--per-job", 2000));
    let t0 = Instant::now();
    let stage = Stage5::load();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let g = &tables5().geometry;
    let (mut sum_a, mut sum_b, mut n) = (0usize, 0usize, 0usize);
    let mut times = Vec::new();
    let mut hist = std::collections::BTreeMap::new();
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let t = Instant::now();
        let deadline = opt(args, "--time").and_then(|s| s.parse::<u64>().ok()).map(|ms| t + std::time::Duration::from_millis(ms));
        let found = stage.stage(&f, extra, per_job, 1, deadline).unwrap_or_else(|e| fail(&e));
        times.push(t.elapsed().as_secs_f64() * 1e3);
        let best = found.first().unwrap_or_else(|| fail("no staging found"));
        g.apply_seq(&mut f, &best.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
        if !Stage5::is_staged(&Cube5::from_facelets(&f).unwrap_or_else(|e| fail(&e))) {
            fail(&format!("cube {}: centres are not staged", n));
        }
        sum_a += best.len_a;
        sum_b += best.moves.len() - best.len_a;
        *hist.entry(best.moves.len()).or_insert(0usize) += 1;
        n += 1;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let nf = n as f64;
    println!("staged the centres of {} cubes, every result verified", n);
    println!(
        "mean length {:.3}  (phase A {:.3}, phase B {:.3})",
        (sum_a + sum_b) as f64 / nf,
        sum_a as f64 / nf,
        sum_b as f64 / nf
    );
    println!(
        "time per cube: mean {:.1} ms, median {:.1} ms, max {:.1} ms",
        times.iter().sum::<f64>() / nf,
        times[times.len() / 2],
        times[times.len() - 1]
    );
    println!("length histogram: {:?}", hist);
}

/// 5x5x5 on a scramble file: full solves, lengths per stage and times.
fn bench5(args: &[String]) {
    use cubenxnxn::c5::tables5;
    use cubenxnxn::solver5::Solver5;
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let limit = opt(args, "--limit").and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let p = params5(args);
    let t0 = Instant::now();
    let solver = Solver5::load();
    eprintln!("tables ready in {:.2} s; {:?}", t0.elapsed().as_secs_f64(), p);
    let g = &tables5().geometry;
    let mut sums = [0usize; 6];
    let mut times = Vec::new();
    let mut hist = std::collections::BTreeMap::new();
    let mut n = 0usize;
    let mut out = opt(args, "--out").map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| fail(&format!("{}: {}", p, e))))
    });
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let t = Instant::now();
        let sol = solver.solve(&f, &p).unwrap_or_else(|e| fail(&format!("cube {}: {}", n, e)));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        g.apply_seq(&mut f, &sol.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
        if !g.is_solved(&f) {
            fail(&format!("cube {}: the solution does not solve the cube", n));
        }
        for k in 0..6 {
            sums[k] += sol.lens[k];
        }
        *hist.entry(sol.moves.len()).or_insert(0usize) += 1;
        times.push(ms);
        if let Some(o) = out.as_mut() {
            let names: Vec<String> = sol.moves.iter().map(|&m| g.move_name(m as usize)).collect();
            writeln!(o, "{}\t{:?}\t{}\t{:.0}\t{}", n, sol.lens, sol.moves.len(), ms, names.join(" ")).unwrap();
        }
        n += 1;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let nf = n as f64;
    let mean = |k: usize| sums[k] as f64 / nf;
    println!("solved {} cubes, every solution verified on the sticker model", n);
    println!(
        "mean length {:.3}  (centres {:.2} + {:.2}, edges {:.2} + {:.2} + {:.2}, 3x3x3 {:.2})",
        sums.iter().sum::<usize>() as f64 / nf,
        mean(0),
        mean(1),
        mean(2),
        mean(3),
        mean(4),
        mean(5)
    );
    println!(
        "time per cube: mean {:.0} ms, median {:.0} ms, max {:.0} ms",
        times.iter().sum::<f64>() / nf,
        times[times.len() / 2],
        times[times.len() - 1]
    );
    println!("length histogram: {:?}", hist);
}

/// 5x5x5 centres in one step, on a scramble file: lengths, bounds and times.
fn centres5(args: &[String]) {
    use cubenxnxn::c5::{tables5, Cube5};
    use cubenxnxn::centres5::{Budget, Centres5};
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (limit, extra, time) = (num("--limit", usize::MAX), num("--extra", 0), num("--time", 0));
    let t0 = Instant::now();
    let c5 = Centres5::load();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let g = &tables5().geometry;
    for (i, scr) in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit).enumerate() {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let cube = Cube5::from_facelets(&f).unwrap_or_else(|e| fail(&e));
        let t = Instant::now();
        let deadline = (time > 0).then(|| t + std::time::Duration::from_millis(time as u64));
        let sols = c5.stage(&cube, &Budget { max_len: 30, extra, cap: 100_000, deadline });
        let ms = t.elapsed().as_secs_f64() * 1e3;
        for s in &sols {
            let mut c = cube;
            c.apply_seq(s);
            if !cubenxnxn::stage5::Stage5::is_staged(&c) {
                fail(&format!("cube {}: a result does not stage the centres", i));
            }
        }
        println!(
            "cube {:3}: bounds {:?}, shortest {:?}, {} results, {:.0} ms",
            i,
            c5.bounds(&cube),
            sols.first().map(|s| s.len()),
            sols.len(),
            ms
        );
    }
}

/// Random-move scrambles for a cube of any size, one per line.
fn scrambles(args: &[String]) {
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (size, count, length) = (num("--size", 6), num("--count", 100), num("--length", 200));
    let mut seed = num("--seed", size * 111) as u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    let g = cubenxnxn::cuben::Geometry::new(size);
    for _ in 0..count {
        let mut names = Vec::with_capacity(length);
        let mut last = usize::MAX;
        while names.len() < length {
            let m = rnd(g.move_count());
            // Not the same face and depth twice in a row.
            if m / 3 != last {
                names.push(g.move_name(m));
                last = m / 3;
            }
        }
        println!("{}", names.join(" "));
    }
}

/// 6x6x6 centre staging on a scramble file: lengths and times per step.
fn stage6(args: &[String]) {
    use cubenxnxn::stage6::Stage6;
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (limit, width) = (num("--limit", usize::MAX), num("--width", 8));
    let t0 = Instant::now();
    let s5 = cubenxnxn::stage5::Stage5::load();
    let s6 = Stage6::load();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let (mut sums, mut total, mut ms, mut n) = ([0usize; 4], 0usize, 0.0, 0usize);
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = s6.g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = s6.g.solved();
        s6.g.apply_seq(&mut f, &scramble);
        let t = Instant::now();
        let found = s6.stage(&s5, &f, width);
        ms += t.elapsed().as_secs_f64() * 1e3;
        let Some(best) = found.first() else { fail(&format!("cube {}: no staging found", n)) };
        s6.g.apply_seq(&mut f, &best.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
        if !s6.is_staged(&f) {
            fail(&format!("cube {}: the result is not staged", n));
        }
        for k in 0..4 {
            sums[k] += best.lens[k];
        }
        total += best.moves.len();
        n += 1;
    }
    let nf = n as f64;
    println!("staged the centres of {} cubes, every result verified", n);
    println!(
        "mean length {:.2}  (steps {})",
        total as f64 / nf,
        sums.iter().map(|&s| format!("{:.2}", s as f64 / nf)).collect::<Vec<_>>().join(" + ")
    );
    println!("mean time {:.0} ms", ms / nf);
}

/// 6x6x6 on a scramble file: full solves, lengths per step and times.
fn bench6(args: &[String]) {
    use cubenxnxn::solver6::{Params6, Solver6};
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let limit = num("--limit", usize::MAX);
    let d = match opt(args, "--mode") {
        Some("fast") => Params6::fast(),
        Some("full") => Params6::full(),
        Some("normal") | None => Params6::normal(),
        Some(_) => usage(),
    };
    let outer = match opt(args, "--mode") {
        // The 5x5x5 part follows the mode unless its own flags say otherwise.
        Some("fast") | None | Some("normal") => cubenxnxn::solver5::Params5 { ..params5(args) },
        _ => cubenxnxn::solver5::Params5 { time_ms: 0, ..d.outer.clone() },
    };
    let p = Params6 {
        width: num("--width", d.width),
        tries: num("--tries", d.tries),
        finals: num("--finals", d.finals),
        budget: opt(args, "--budget").and_then(|s| s.parse().ok()).unwrap_or(d.budget),
        outer,
    };
    let t0 = Instant::now();
    let solver = Solver6::load();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let g = &solver.s6.g;
    let (mut sums, mut times, mut n) = ([0usize; 4], Vec::new(), 0usize);
    let mut hist = std::collections::BTreeMap::new();
    let mut out = opt(args, "--out").map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| fail(&format!("{}: {}", p, e))))
    });
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let t = Instant::now();
        let sol = solver.solve(&f, &p).unwrap_or_else(|e| fail(&format!("cube {}: {}", n, e)));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        g.apply_seq(&mut f, &sol.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
        if !g.is_solved(&f) {
            fail(&format!("cube {}: the solution does not solve the cube", n));
        }
        for k in 0..4 {
            sums[k] += sol.lens[k];
        }
        *hist.entry(sol.moves.len()).or_insert(0usize) += 1;
        times.push(ms);
        if let Some(o) = out.as_mut() {
            let names: Vec<String> = sol.moves.iter().map(|&m| g.move_name(m as usize)).collect();
            writeln!(o, "{}\t{:?}\t{}\t{:.0}\t{}", n, sol.lens, sol.moves.len(), ms, names.join(" ")).unwrap();
            o.flush().unwrap();
        }
        n += 1;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let nf = n as f64;
    println!("solved {} cubes, every solution verified on the sticker model", n);
    println!(
        "mean length {:.2}  (staging {:.2}, preparing {:.2}, inner reduction {:.2}, outer 5x5x5 {:.2})",
        sums.iter().sum::<usize>() as f64 / nf,
        sums[0] as f64 / nf,
        sums[1] as f64 / nf,
        sums[2] as f64 / nf,
        sums[3] as f64 / nf
    );
    println!(
        "time per cube: mean {:.0} ms, median {:.0} ms, max {:.0} ms",
        times.iter().sum::<f64>() / nf,
        times[times.len() / 2],
        times[times.len() - 1]
    );
    println!("length histogram: {:?}", hist);
}

/// 7x7x7 centre staging on a scramble file: lengths and times per step.
fn stage7(args: &[String]) {
    use cubenxnxn::stage7::Stage7;
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (limit, width) = (num("--limit", usize::MAX), num("--width", 4));
    let t0 = Instant::now();
    let s5 = cubenxnxn::stage5::Stage5::load();
    let s7 = Stage7::load();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let (mut sums, mut total, mut ms, mut n) = ([0usize; 5], 0usize, 0.0, 0usize);
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = s7.g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = s7.g.solved();
        s7.g.apply_seq(&mut f, &scramble);
        if let Some(c) = opt(args, "--probe2").and_then(|s| s.parse::<usize>().ok()) {
            s7.probe_step2(&s5, &f, num("--extra", 0), c, num("--beam", 20_000) as u64);
            continue;
        }
        let t = Instant::now();
        let found = s7.stage(&s5, &f, width, num("--beam", 20_000));
        let took = t.elapsed().as_secs_f64() * 1e3;
        ms += took;
        let Some(best) = found.first() else { fail(&format!("cube {}: no staging found", n)) };
        s7.g.apply_seq(&mut f, &best.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
        if !s7.is_staged(&f) {
            fail(&format!("cube {}: the result is not staged", n));
        }
        eprintln!("cube {:3}: {:?} = {} moves, {:.0} ms", n, best.lens, best.moves.len(), took);
        for k in 0..5 {
            sums[k] += best.lens[k];
        }
        total += best.moves.len();
        n += 1;
    }
    let nf = n as f64;
    println!("staged the centres of {} cubes, every result verified", n);
    println!(
        "mean length {:.2}  (steps {})",
        total as f64 / nf,
        sums.iter().map(|&s| format!("{:.2}", s as f64 / nf)).collect::<Vec<_>>().join(" + ")
    );
    println!("mean time {:.0} ms", ms / nf);
}

fn params7(args: &[String]) -> cubenxnxn::solver7::Params7 {
    use cubenxnxn::solver7::Params7;
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let d = Params7::normal();
    let outer = params5(args);
    Params7 {
        width: num("--width", d.width),
        tries: num("--tries", d.tries),
        matchings: num("--matchings", d.matchings),
        match_extra: num("--match-extra", d.match_extra),
        beam: num("--beam", d.beam),
        reductions: num("--reductions", d.reductions),
        inner: outer.clone(),
        outer,
    }
}

/// 7x7x7 on a scramble file: full solves, lengths per step and times.
fn bench7(args: &[String]) {
    use cubenxnxn::solver7::Solver7;
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (limit, skip) = (num("--limit", usize::MAX), num("--skip", 0));
    let p = params7(args);
    let t0 = Instant::now();
    let solver = Solver7::load();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let g = &solver.s7.g;
    let (mut sums, mut times, mut n) = ([0usize; 5], Vec::new(), 0usize);
    let mut hist = std::collections::BTreeMap::new();
    let mut out = opt(args, "--out").map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| fail(&format!("{}: {}", p, e))))
    });
    for (i, scr) in text.lines().map(str::trim).filter(|l| !l.is_empty()).enumerate().skip(skip).take(limit) {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let t = Instant::now();
        let sol = solver.solve(&f, &p).unwrap_or_else(|e| fail(&format!("cube {}: {}", i, e)));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        g.apply_seq(&mut f, &sol.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
        if !g.is_solved(&f) {
            fail(&format!("cube {}: the solution does not solve the cube", i));
        }
        eprintln!("cube {:3}: {:?} = {} moves, {:.0} ms", i, sol.lens, sol.moves.len(), ms);
        for k in 0..5 {
            sums[k] += sol.lens[k];
        }
        *hist.entry(sol.moves.len()).or_insert(0usize) += 1;
        times.push(ms);
        if let Some(o) = out.as_mut() {
            let names: Vec<String> = sol.moves.iter().map(|&m| g.move_name(m as usize)).collect();
            writeln!(o, "{}\t{:?}\t{}\t{:.0}\t{}", i, sol.lens, sol.moves.len(), ms, names.join(" ")).unwrap();
            o.flush().unwrap();
        }
        n += 1;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let nf = n as f64;
    println!("solved {} cubes, every solution verified on the sticker model", n);
    println!(
        "mean length {:.2}  (staging {:.2}, matching {:.2}, inner edges {:.2}, outer edges {:.2}, 3x3x3 {:.2})",
        sums.iter().sum::<usize>() as f64 / nf,
        sums[0] as f64 / nf,
        sums[1] as f64 / nf,
        sums[2] as f64 / nf,
        sums[3] as f64 / nf,
        sums[4] as f64 / nf
    );
    println!(
        "time per cube: mean {:.0} ms, median {:.0} ms, max {:.0} ms",
        times.iter().sum::<f64>() / nf,
        times[times.len() / 2],
        times[times.len() - 1]
    );
    println!("length histogram: {:?}", hist);
}
