use cubenxnxn::cube::{format_moves, parse_moves, Cube};
use cubenxnxn::solver::{Params, Solver};
use rayon::prelude::*;
use std::io::Write;
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage:\n  cubenxnxn solve <moves...> [--size N] [search flags]   solve a scramble given as moves (4x4x4 unless --size says otherwise; any size from 2 up)\n  cubenxnxn solve --state <facelets> [search flags]   solve a cube given as 6*N*N facelets in URFDLB order\n  cubenxnxn facelets --size N <moves...>   print the facelets of a scrambled cube\n  cubenxnxn scrambles --size N [--count C] [--length L] [--seed S]   random-move scrambles, one per line\n  cubenxnxn verify --size N <file>   check solutions; each line is scramble<TAB>solution\n  cubenxnxn bench <scrambles.txt> [--out results.tsv] [--tpr tpr-results.tsv] [--limit N] [--parallel] [search flags]   4x4x4 scrambles\n  cubenxnxn known --walk K [--count N] [--seed S] [--base reduced|solved] [--out results.tsv] [search flags]   4x4x4 cubes K moves away from a reduced (or the solved) cube: what the solver finds against what is known to exist\n  cubenxnxn bench5 <scrambles.txt> [--out results.tsv] [--limit N] [search flags]   5x5x5 scrambles\n  cubenxnxn bench6 <scrambles.txt> [--out results.tsv] [--limit N] [--mode fast|normal|full]   6x6x6 scrambles\n  cubenxnxn bench7 <scrambles.txt> [--out results.tsv] [--limit N] [--mode fast|normal]   7x7x7 scrambles\n  cubenxnxn benchn <scrambles.txt> --size N [--out results.tsv] [--limit N]   scrambles of size 8 and up\n  cubenxnxn row <scrambles.txt> --size N [--limit N] [--mode M]   any size from 2 up in one mode: a line of size, cubes, mean moves, mean and longest seconds per cube\n\nsearch flags start with --mode fast|normal|full; see README.md for the rest"
    );
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("solve") => solve(&args[1..]),
        Some("bench") => bench(&args[1..]),
        Some("known") => known(&args[1..]),
        Some("free3") => free3(&args[1..]),
        Some("free-features") => free_features(&args[1..]),
        Some("free-beam") => free_beam(&args[1..]),
        Some("free-tail") => free_tail(&args[1..]),
        Some("free-dump") => free_dump(&args[1..]),
        Some("short") => short(&args[1..]),
        Some("free-export") => free_export(&args[1..]),
        Some("free-verify") => free_verify(&args[1..]),
        Some("probe333") => probe333(&args[1..]),
        Some("gen-centres") => gen_centres(),
        Some("startup") => startup(),
        Some("facelets") => facelets(&args[1..]),
        Some("verify") => verify(&args[1..]),
        Some("scrambles") => scrambles(&args[1..]),
        Some("stage6") => stage6(&args[1..]),
        Some("bench6") => bench6(&args[1..]),
        Some("benchn") => benchn(&args[1..]),
        Some("row") => row(&args[1..]),
        Some("rowtest") => rowtest(&args[1..]),
        Some("stagen") => stagen(&args[1..]),
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
        (None, Some(216)) => 6,
        (None, Some(294)) => 7,
        _ => 4,
    };
    // Any other length that is six squares is a larger cube.
    let size = match (opt(args, "--size"), state.map(|s| s.trim().len())) {
        (None, Some(len)) if len > 294 => (8..100).find(|n| 6 * n * n == len).unwrap_or(size),
        _ => size,
    };
    if size >= 8 {
        return solven(size, state, args);
    }
    if size == 5 {
        return solve5(state, args);
    }
    if size == 6 {
        return solve6(state, args);
    }
    if size == 7 {
        return solve7(state, args);
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
        _ => fail("supported sizes: 2 to 7"),
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

fn params6(args: &[String]) -> cubenxnxn::solver6::Params6 {
    use cubenxnxn::solver6::Params6;
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let d = match opt(args, "--mode") {
        Some("fast") => Params6::fast(),
        Some("full") => Params6::full(),
        Some("normal") | None => Params6::normal(),
        Some(_) => usage(),
    };
    // The 5x5x5 part follows the mode unless its own flags say otherwise.
    let o = &d.outer;
    let outer = cubenxnxn::solver5::Params5 {
        labellings: num("--labellings", o.labellings),
        c_keep: num("--c-keep", o.c_keep),
        p_keep: num("--p-keep", o.p_keep),
        e_keep: num("--e-keep", o.e_keep),
        pool: num("--pool", o.pool),
        probes: num("--probes", o.probes as usize) as u64,
        time_ms: num("--time", o.time_ms as usize) as u64,
        ..o.clone()
    };
    Params6 {
        width: num("--width", d.width),
        first: num("--first", d.first),
        fan: num("--fan", d.fan),
        tries: num("--tries", d.tries),
        splits: num("--splits", d.splits),
        holds: num("--holds", d.holds),
        finals: num("--finals", d.finals),
        budget: opt(args, "--budget").and_then(|s| s.parse().ok()).unwrap_or(d.budget),
        beam: num("--beam", d.beam),
        outer,
    }
}

fn solve6(state: Option<&str>, args: &[String]) {
    let t0 = Instant::now();
    let solver = cubenxnxn::solver6::Solver6::load();
    let g = &solver.s6.g;
    let mut facelets = match state {
        Some(s) => g.from_string(s).unwrap_or_else(|e| fail(&e)),
        None => {
            let moves = g.parse_moves(&move_text(args)).unwrap_or_else(|e| fail(&e));
            let mut f = g.solved();
            g.apply_seq(&mut f, &moves);
            f
        }
    };
    let t1 = Instant::now();
    match solver.solve(&facelets, &params6(args)) {
        Ok(sol) => {
            g.apply_seq(&mut facelets, &sol.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
            if !g.is_solved(&facelets) {
                fail("the solution found does not solve the cube");
            }
            let names: Vec<String> = sol.moves.iter().map(|&m| g.move_name(m as usize)).collect();
            println!("{}", names.join(" "));
            eprintln!(
                "{} moves (staging {}, preparing {}, inner reduction {}, outer 5x5x5 {}), tables {:.0} ms, search {:.0} ms",
                sol.moves.len(),
                sol.lens[0],
                sol.lens[1],
                sol.lens[2],
                sol.lens[3],
                (t1 - t0).as_secs_f64() * 1e3,
                t1.elapsed().as_secs_f64() * 1e3
            );
        }
        Err(e) => fail(&e),
    }
}

/// Sizes from 8 up: the general solver.
fn solven(size: usize, state: Option<&str>, args: &[String]) {
    let t0 = Instant::now();
    let g = cubenxnxn::cuben::Geometry::new(size);
    let solver = cubenxnxn::nnn::SolverN::load();
    let mut facelets = match state {
        Some(s) => g.from_string(s).unwrap_or_else(|e| fail(&e)),
        None => {
            let moves = g.parse_moves(&move_text(args)).unwrap_or_else(|e| fail(&e));
            let mut f = g.solved();
            g.apply_seq(&mut f, &moves);
            f
        }
    };
    let t1 = Instant::now();
    match solver.solve(&g, &facelets) {
        Ok(sol) => {
            g.apply_seq(&mut facelets, &sol.moves);
            if !g.is_solved(&facelets) {
                fail("the solution found does not solve the cube");
            }
            let names: Vec<String> = sol.moves.iter().map(|&m| g.move_name(m)).collect();
            println!("{}", names.join(" "));
            eprintln!(
                "{} moves (centres {}, edges {}, 3x3x3 {}), tables {:.0} ms, search {:.0} ms",
                sol.moves.len(),
                sol.lens[0],
                sol.lens[1],
                sol.lens[2],
                (t1 - t0).as_secs_f64() * 1e3,
                t1.elapsed().as_secs_f64() * 1e3
            );
        }
        Err(e) => fail(&e),
    }
}

/// The matching of one row of centres (`rings`) on random staged cubes:
/// how often it is found, how long it is and how long it takes.
fn rowtest(args: &[String]) {
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (size, row, count) = (num("--size", 10), num("--row", 1), num("--count", 10));
    let g = cubenxnxn::cuben::Geometry::new(size);
    let ring = cubenxnxn::rings::Ring::new(&g, row);
    // Face turns and half turns keep the centres of a solved cube staged.
    let staged: Vec<usize> = (0..g.move_count()).filter(|m| m / 18 == 0 || m % 3 == 1).collect();
    let mut seed = num("--seed", 12345) as u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    let (mut found, mut total, mut ms) = (0usize, 0usize, 0.0);
    for trial in 0..count {
        let mut f = g.solved();
        for _ in 0..400 {
            g.apply(&mut f, staged[rnd(staged.len())]);
        }
        let t = Instant::now();
        let result = ring.solve(&g, &f);
        let took = t.elapsed().as_secs_f64() * 1e3;
        ms += took;
        match result {
            Ok(moves) => {
                g.apply_seq(&mut f, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
                if !ring.matched(&f) {
                    fail(&format!("trial {}: the row is not matched", trial));
                }
                eprintln!("trial {:2}: {} moves, {:.0} ms", trial, moves.len(), took);
                found += 1;
                total += moves.len();
            }
            Err(e) => eprintln!("trial {:2}: {}, {:.0} ms", trial, e, took),
        }
    }
    println!(
        "row {} of a {}x{}x{} ({} centres): matched {} of {}, mean {:.1} moves, {:.1} s per trial",
        row,
        size,
        size,
        size,
        ring.len(),
        found,
        count,
        total as f64 / found.max(1) as f64,
        ms / count as f64 / 1e3
    );
}

/// The general solver on a scramble file of any size from 6 up.
fn staging(args: &[String], d: &cubenxnxn::staging::Staging) -> cubenxnxn::staging::Staging {
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    cubenxnxn::staging::Staging {
        width: num("--width", d.width),
        fan: num("--fan", d.fan),
        beam: num("--beam", d.beam),
        axes: num("--axes", d.axes as usize) == 1,
    }
}

/// The staging of the centres alone, for a cube of any size from 6 up, on a
/// scramble file: lengths and times, every result checked.
fn stagen(args: &[String]) {
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (size, limit) = (num("--size", 8), num("--limit", usize::MAX));
    let g = cubenxnxn::cuben::Geometry::new(size);
    let t0 = Instant::now();
    let mut solver = cubenxnxn::nnn::SolverN::load();
    solver.staging = staging(args, &solver.staging);
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let (mut total, mut times, mut n) = (0usize, Vec::new(), 0usize);
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let t = Instant::now();
        let moves = solver.stage(&g, &f).unwrap_or_else(|e| fail(&format!("cube {}: {}", n, e)));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        g.apply_seq(&mut f, &moves);
        if !cubenxnxn::staging::is_staged(&g, &f) {
            fail(&format!("cube {}: the centres are not staged", n));
        }
        eprintln!("cube {:3}: {} moves, {:.0} ms", n, moves.len(), ms);
        total += moves.len();
        times.push(ms);
        n += 1;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let nf = n as f64;
    println!("staged the centres of {} cubes of size {}, every result checked", n, size);
    println!("mean length {:.2}", total as f64 / nf);
    println!("time per cube: mean {:.0} ms, max {:.0} ms", times.iter().sum::<f64>() / nf, times[times.len() - 1]);
}

fn benchn(args: &[String]) {
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (size, limit) = (num("--size", 8), num("--limit", usize::MAX));
    let g = cubenxnxn::cuben::Geometry::new(size);
    let t0 = Instant::now();
    let mut solver = cubenxnxn::nnn::SolverN::load();
    solver.staging = staging(args, &solver.staging);
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let (mut sums, mut times, mut n, mut long_way) = ([0usize; 3], Vec::new(), 0usize, 0usize);
    let mut out = opt(args, "--out").map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| fail(&format!("{}: {}", p, e))))
    });
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let t = Instant::now();
        let sol = solver.solve(&g, &f).unwrap_or_else(|e| fail(&format!("cube {}: {}", n, e)));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        g.apply_seq(&mut f, &sol.moves);
        if !g.is_solved(&f) {
            fail(&format!("cube {}: the solution does not solve the cube", n));
        }
        let way = if sol.long_way { ", the long way" } else { "" };
        eprintln!("cube {:3}: {:?} = {} moves, {:.0} ms{}", n, sol.lens, sol.moves.len(), ms, way);
        for k in 0..3 {
            sums[k] += sol.lens[k];
        }
        long_way += sol.long_way as usize;
        times.push(ms);
        if let Some(o) = out.as_mut() {
            let names: Vec<String> = sol.moves.iter().map(|&m| g.move_name(m)).collect();
            writeln!(o, "{}\t{:?}\t{}\t{:.0}\t{}", n, sol.lens, sol.moves.len(), ms, names.join(" ")).unwrap();
            o.flush().unwrap();
        }
        n += 1;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let nf = n as f64;
    println!("solved {} cubes of size {}, every solution verified on the sticker model", n, size);
    if long_way > 0 {
        println!("{} of them the long way (no matching of the centres found)", long_way);
    }
    println!(
        "mean length {:.2}  (centres {:.2}, edges {:.2}, 3x3x3 {:.2})",
        sums.iter().sum::<usize>() as f64 / nf,
        sums[0] as f64 / nf,
        sums[1] as f64 / nf,
        sums[2] as f64 / nf
    );
    println!(
        "time per cube: mean {:.0} ms, median {:.0} ms, max {:.0} ms",
        times.iter().sum::<f64>() / nf,
        times[times.len() / 2],
        times[times.len() - 1]
    );
}

/// Any size on a scramble file, in one mode (the default one unless
/// `--mode` says otherwise): one line of size, cubes, mean moves, mean and
/// longest seconds per cube, separated by tabs. Every solution is applied
/// on the sticker model. Tables are loaded before the clock starts.
fn row(args: &[String]) {
    use cubenxnxn::cuben::Geometry;
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (size, limit) = (num("--size", 0), num("--limit", usize::MAX));
    if size < 2 {
        usage();
    }
    let g = Geometry::new(size);
    // The small cubes speak the 4x4x4's move numbers: their solutions go through names.
    let named = |g: &Geometry, moves: &[u8]| g.parse_moves(&format_moves(moves));
    let small = |scramble: &str| parse_moves(scramble).map(|m| Cube::from_moves(&m));
    let wide = |moves: Vec<u8>| moves.into_iter().map(|m| m as usize).collect::<Vec<usize>>();
    // From the scramble as written and the scrambled stickers to a solution.
    type Solve = Box<dyn Fn(&Geometry, &str, &[u8]) -> Result<Vec<usize>, String>>;
    let solve: Solve = match size {
        2 => {
            let t = cubenxnxn::c2::T2::load();
            Box::new(move |g, scr, _| named(g, &t.solve(&small(scr)?)))
        }
        3 => {
            use cubenxnxn::c3::{C3, T3};
            let t = T3::new();
            let probes = if opt(args, "--mode") == Some("fast") { 500 } else { 30_000 };
            Box::new(move |g, scr, _| {
                let c3 = small(scr)?.with_home_centres().and_then(|c| C3::from_cube(&c));
                named(g, &t.solve(&c3.ok_or_else(|| "not a 3x3x3 state".to_string())?, probes, None))
            })
        }
        4 => {
            let (s, p) = (Solver::new(), params(args));
            Box::new(move |g, scr, _| named(g, &s.solve(&small(scr)?, &p)?.moves))
        }
        5 => {
            let (s, p) = (cubenxnxn::solver5::Solver5::load(), params5(args));
            Box::new(move |_, _, f| Ok(wide(s.solve(f, &p)?.moves)))
        }
        6 => {
            let (s, p) = (cubenxnxn::solver6::Solver6::load(), params6(args));
            Box::new(move |_, _, f| Ok(wide(s.solve(f, &p)?.moves)))
        }
        7 => {
            let (s, p) = (cubenxnxn::solver7::Solver7::load(), params7(args));
            Box::new(move |_, _, f| Ok(wide(s.solve(f, &p)?.moves)))
        }
        _ => {
            let s = cubenxnxn::nnn::SolverN::load();
            Box::new(move |g, _, f| s.solve(g, f).map(|sol| sol.moves))
        }
    };
    let (mut moves, mut times) = (0usize, Vec::new());
    for (n, scr) in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit).enumerate() {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let t = Instant::now();
        let sol = solve(&g, scr, &f).unwrap_or_else(|e| fail(&format!("cube {}: {}", n, e)));
        let secs = t.elapsed().as_secs_f64();
        g.apply_seq(&mut f, &sol);
        if !g.is_solved(&f) {
            fail(&format!("cube {}: the solution does not solve the cube", n));
        }
        eprintln!("cube {:3}: {} moves, {:.2} s", n, sol.len(), secs);
        moves += sol.len();
        times.push(secs);
    }
    if times.is_empty() {
        fail("no scrambles");
    }
    let nf = times.len() as f64;
    println!(
        "{}\t{}\t{:.2}\t{:.2}\t{:.2}",
        size,
        times.len(),
        moves as f64 / nf,
        times.iter().sum::<f64>() / nf,
        times.iter().cloned().fold(0.0, f64::max)
    );
}

fn solve7(state: Option<&str>, args: &[String]) {
    let t0 = Instant::now();
    let solver = cubenxnxn::solver7::Solver7::load();
    let g = &solver.s7.g;
    let mut facelets = match state {
        Some(s) => g.from_string(s).unwrap_or_else(|e| fail(&e)),
        None => {
            let moves = g.parse_moves(&move_text(args)).unwrap_or_else(|e| fail(&e));
            let mut f = g.solved();
            g.apply_seq(&mut f, &moves);
            f
        }
    };
    let t1 = Instant::now();
    match solver.solve(&facelets, &params7(args)) {
        Ok(sol) => {
            g.apply_seq(&mut facelets, &sol.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
            if !g.is_solved(&facelets) {
                fail("the solution found does not solve the cube");
            }
            let names: Vec<String> = sol.moves.iter().map(|&m| g.move_name(m as usize)).collect();
            println!("{}", names.join(" "));
            eprintln!(
                "{} moves (staging {}, matching {}, inner edges {}, outer edges {}, 3x3x3 {}), tables {:.0} ms, search {:.0} ms",
                sol.moves.len(),
                sol.lens[0],
                sol.lens[1],
                sol.lens[2],
                sol.lens[3],
                sol.lens[4],
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
        stage_budget: num("--a-budget", d.stage_budget as usize) as u64,
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

/// Experiment: training data for a learned estimate of the distance to a
/// reduced cube. Each record is a cube `K` moves away from a reduced cube
/// (`--kmin`..`--kmax`, or the list `--ks`), 52 bytes: the 24 recoloured
/// centres, the 24 partner positions, the parity bits, K, the exact distance
/// when it is at most 6 (else 255), and with `--hand` the score the beam search
/// uses (2 x staging distance - dedges paired + 12, else 255).
fn free_dump(args: &[String]) {
    use cubenxnxn::free::{class_parts, Ball};
    use cubenxnxn::tables::axis_masks;
    let num = |name: &str, default: usize| {
        opt(args, name).map_or(default, |s| s.parse::<usize>().unwrap_or_else(|_| usage()))
    };
    let (count, kmin, kmax, seed) = (num("--count", 1_000_000), num("--kmin", 1), num("--kmax", 26), num("--seed", 1));
    let ks: Option<Vec<usize>> =
        opt(args, "--ks").map(|s| s.split(',').map(|v| v.parse().unwrap_or_else(|_| usage())).collect());
    let out = opt(args, "--out").unwrap_or_else(|| usage());
    let hand = args.iter().any(|a| a == "--hand");
    let t0 = Instant::now();
    let ball = Ball::build(6);
    let solver = hand.then(Solver::new);
    let record = |i: usize| -> [u8; 52] {
        let mut state = (seed as u64) << 32 ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
        let mut rnd = |n: usize| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as usize % n
        };
        // A few steps to separate neighbouring seeds.
        for _ in 0..4 {
            rnd(2);
        }
        let k = match &ks {
            Some(list) => list[i % list.len()],
            None => kmin + rnd(kmax - kmin + 1),
        };
        let cube = Cube::from_moves(&free_walk(&mut rnd, k));
        let (centres, partner, bits) = class_parts(&cube);
        let mut rec = [255u8; 52];
        rec[..24].copy_from_slice(&centres);
        rec[24..48].copy_from_slice(&partner);
        rec[48] = bits;
        rec[49] = k as u8;
        if let Some(d) = ball.dist(&cube) {
            rec[50] = d;
        }
        if let Some(s) = &solver {
            let m = axis_masks(&cube);
            let stage = [(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)]
                .iter()
                .map(|&(a, b)| s.ctab.exact(&s.tabs.bp, &s.tabs.ranks, m[a], m[b]))
                .min()
                .unwrap();
            let paired = (0..12).filter(|&p| cube.ep[p + 12] == (cube.ep[p] + 12) % 24).count() as u8;
            rec[51] = 2 * stage + 12 - paired;
        }
        rec
    };
    let mut file = std::io::BufWriter::new(std::fs::File::create(out).unwrap_or_else(|e| fail(&format!("{}: {}", out, e))));
    for start in (0..count).step_by(1 << 20) {
        let end = (start + (1 << 20)).min(count);
        let chunk: Vec<[u8; 52]> = (start..end).into_par_iter().map(&record).collect();
        for rec in &chunk {
            file.write_all(rec).unwrap();
        }
    }
    eprintln!("{} records in {:.1} s", count, t0.elapsed().as_secs_f64());
}

/// Experiment: lightly scrambled cubes. The solved cube gets `--walk` random
/// moves; the shortest solution is found exactly when it has at most
/// `--forward` + `--ball` moves (every sequence of up to `--forward` moves is
/// tried against a table of all cubes within `--ball` moves of solved), and
/// compared with what the solver finds.
fn short(args: &[String]) {
    use cubenxnxn::free::Ball;
    let num = |name: &str, default: usize| {
        opt(args, name).map_or(default, |s| s.parse::<usize>().unwrap_or_else(|_| usage()))
    };
    let (walk, count, forward, depth) = (num("--walk", 12), num("--count", 10), num("--forward", 6), num("--ball", 5));
    let mut seed = num("--seed", 5000 + walk) as u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    let t0 = Instant::now();
    let ball = Ball::build_solved(depth as u8);
    eprintln!("cubes by distance from solved: {:?}, {:.1} s", ball.levels, t0.elapsed().as_secs_f64());
    let solver = Solver::new();
    let params = params(args);
    let (mut sum_exact, mut sum_solver, mut found, mut secs) = (0usize, 0usize, 0usize, 0.0);
    for i in 0..count {
        let (_, steps) = known_cube(&mut rnd, walk, true);
        let cube = Cube::from_moves(&steps);
        let t = Instant::now();
        let exact = ball.shortest(&cube, forward).map(|p| {
            let mut c = cube;
            c.apply_seq(&p);
            if !c.is_solved() {
                fail(&format!("cube {}: the exact solution does not solve the cube", i));
            }
            p.len()
        });
        let s = t.elapsed().as_secs_f64();
        let sol = solver.solve(&cube, &params).unwrap_or_else(|e| fail(&format!("cube {}: {}", i, e)));
        println!("cube {:2}: scrambled with {}, exact {:?} in {:.1} s, solver {}", i, walk, exact, s, sol.moves.len());
        if let Some(e) = exact {
            found += 1;
            sum_exact += e;
            sum_solver += sol.moves.len();
            secs += s;
        }
    }
    if found > 0 {
        println!(
            "{} cubes scrambled with {} moves: exact solution on {}, mean {:.2} in {:.1} s; solver on the same cubes {:.2}",
            count,
            walk,
            found,
            sum_exact as f64 / found as f64,
            secs / found as f64,
            sum_solver as f64 / found as f64
        );
    }
}

/// Experiment: what a search outside this program needs, as JSON: the 27 moves
/// as permutations of the centre and wing positions, the centres of the 24
/// orientations with the parity a reduced cube has in each, and the cubes of a
/// scramble file (with `--solve`, also the shortest reduction the solver finds).
fn free_export(args: &[String]) {
    use cubenxnxn::cube::{perm_parity, MOVE_NAMES};
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let limit = opt(args, "--limit").and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let solve = args.iter().any(|a| a == "--solve");
    let ct = cubenxnxn::cube::tables();
    let list = |v: &[u8]| format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
    let rows = |v: Vec<String>| format!("[{}]", v.join(","));
    let dedges = |c: &Cube| -> Vec<u8> { c.ep[..12].iter().map(|&e| e % 12).collect() };
    println!("{{");
    println!("\"names\": {},", rows(MOVE_NAMES[..27].iter().map(|n| format!("\"{}\"", n)).collect()));
    println!("\"ct_src\": {},", rows((0..27).map(|m| list(&ct.ct_src[m])).collect()));
    println!("\"ep_src\": {},", rows((0..27).map(|m| list(&ct.ep_src[m])).collect()));
    println!("\"rots\": {},", rows(ct.rots.iter().map(|r| list(&r.state.ct)).collect()));
    // Parity of the dedge permutation against the corners in each orientation of a solved cube;
    // face turns change neither, so every reduced, solvable cube in that orientation has it.
    println!(
        "\"rot_parity\": {},",
        list(&ct.rots.iter().map(|r| perm_parity(&dedges(&r.state)) ^ perm_parity(&r.state.cp)).collect::<Vec<_>>())
    );
    let solver = solve.then(Solver::new);
    let params = params(args);
    let mut cubes = Vec::new();
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let cube = Cube::from_moves(&parse_moves(scr).unwrap_or_else(|e| fail(&e)));
        let shortest = solver.as_ref().map_or(0, |s| s.solve(&cube, &params).unwrap_or_else(|e| fail(&e)).shortest_reduction);
        cubes.push(format!(
            "{{\"scramble\": \"{}\", \"ct\": {}, \"ep\": {}, \"corner_parity\": {}, \"solver\": {}}}",
            scr,
            list(&cube.ct),
            list(&cube.ep),
            perm_parity(&cube.cp),
            shortest
        ));
    }
    println!("\"cubes\": {}", rows(cubes));
    println!("}}");
}

/// Checks reductions found outside this program: each line of the file is
/// `scramble<TAB>moves`, and the moves must leave a reduced cube.
fn free_verify(args: &[String]) {
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let (mut ok, mut total, mut moves) = (0usize, 0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut parts = line.split('\t');
        let mut cube = Cube::from_moves(&parse_moves(parts.next().unwrap_or("")).unwrap_or_else(|e| fail(&e)));
        let reduction = parse_moves(parts.next().unwrap_or("")).unwrap_or_else(|e| fail(&e));
        cube.apply_seq(&reduction);
        total += 1;
        moves += reduction.len();
        if cubenxnxn::free::is_reduced(&cube) {
            ok += 1;
        } else {
            println!("line {}: not reduced", total);
        }
    }
    println!("{} of {} reduced, mean length {:.3}", ok, total, moves as f64 / total.max(1) as f64);
}

/// Experiment: is the end of the solver's reduction as short as it could be?
/// The cube is solved, the reduction of the solution is cut `j` moves before
/// the end of the joint phase, and the rest (j moves of the joint phase plus
/// phase 3) is compared with the shortest reduction from that point with all
/// moves, exact up to 12 moves.
fn free_tail(args: &[String]) {
    use cubenxnxn::free::{is_reduced, Ball};
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| {
        opt(args, name).map_or(default, |s| s.parse::<usize>().unwrap_or_else(|_| usage()))
    };
    let (limit, max_back, forward) = (num("--limit", 20), num("--back", 3), num("--forward", 6));
    let ball = Ball::build(6);
    let solver = Solver::new();
    let params = params(args);
    // Per j: cubes where the rest is short enough to be decided, where it got shorter, moves saved.
    let mut decided = vec![0usize; max_back + 1];
    let mut shorter = vec![0usize; max_back + 1];
    let mut saved = vec![0usize; max_back + 1];
    for (i, scr) in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit).enumerate() {
        let cube = Cube::from_moves(&parse_moves(scr).unwrap_or_else(|e| fail(&e)));
        let sol = solver.solve(&cube, &params).unwrap_or_else(|e| fail(&format!("cube {}: {}", i, e)));
        let (a, k) = (sol.lens[0] + sol.lens[1], sol.lens[2]);
        let mut line = format!("cube {:2}: joint {} + phase 3 {}:", i, a, k);
        for j in 0..=max_back.min(a) {
            let rest = j + k;
            let mut x = cube;
            x.apply_seq(&sol.moves[..a - j]);
            // Exact when the answer is at most forward + 6; the rest is known to be at most `rest`.
            if rest > forward + 7 {
                line += &format!("  j={} ?", j);
                continue;
            }
            let free = ball.shortest(&x, forward).map(|p| {
                let mut c = x;
                c.apply_seq(&p);
                if !is_reduced(&c) {
                    fail("a free reduction does not reduce the cube");
                }
                p.len()
            });
            // None can only mean rest == forward + 7 and nothing shorter exists.
            let f = free.unwrap_or(rest);
            decided[j] += 1;
            if f < rest {
                shorter[j] += 1;
                saved[j] += rest - f;
            }
            line += &format!("  j={} {}->{}", j, rest, f);
        }
        println!("{}", line);
    }
    for j in 0..=max_back {
        println!(
            "cut {} moves before the end of the joint phase: decided on {} cubes, shorter on {}, {} moves saved in all",
            j, decided[j], shorter[j], saved[j]
        );
    }
}

/// A cube for `known`: the moves of its base and of the walk away from it.
fn known_cube(rnd: &mut impl FnMut(usize) -> usize, walk: usize, from_solved: bool) -> (Vec<u8>, Vec<u8>) {
    use cubenxnxn::cube::skip_move;
    // The base: a 3x3x3 state on the 4x4x4, from 40 random face turns.
    let mut base: Vec<u8> = Vec::new();
    while !from_solved && base.len() < 40 {
        let m = rnd(18) as u8;
        if base.last().is_none_or(|&l| !skip_move(l, m)) {
            base.push(m);
        }
    }
    // The walk. Undone, it must be a reduction that cannot be shortened at sight:
    // its last move is wide, and no face turn can be commuted to its end. So face
    // turns wait until the walk has left the axis of its first move. Two opposite
    // wide moves in a row are one wide move and a rotation, so they are left out.
    let mut steps: Vec<u8> = Vec::new();
    while steps.len() < walk {
        let m = rnd(36) as u8;
        let axis = m / 3 % 3;
        let one_axis = steps.iter().all(|&s| s / 3 % 3 == axis);
        let wide_before = steps.iter().rev().take_while(|&&s| s / 3 % 3 == axis).any(|&s| s >= 18);
        if (m < 18 && one_axis && !from_solved)
            || (m >= 18 && wide_before)
            || steps.last().is_some_and(|&l| skip_move(l, m))
        {
            continue;
        }
        steps.push(m);
    }
    (base, steps)
}

/// Experiment: beam search for a reduction with all moves, on the cubes of
/// `known --walk K` (same seed, same cubes). `--solver file` is the result
/// file of `known` for the same K, to put the solver's reductions next to it.
fn free_beam(args: &[String]) {
    use cubenxnxn::free::{is_reduced, Ball, Beam};
    let num = |name: &str, default: usize| {
        opt(args, name).map_or(default, |s| s.parse::<usize>().unwrap_or_else(|_| usage()))
    };
    let (walk, count) = (num("--walk", 14), num("--count", 10));
    let mut seed = num("--seed", 4000 + walk) as u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    let theirs: Option<Vec<usize>> = opt(args, "--solver").map(|p| {
        std::fs::read_to_string(p)
            .unwrap_or_else(|e| fail(&format!("{}: {}", p, e)))
            .lines()
            .map(|l| l.split('\t').nth(3).and_then(|v| v.parse().ok()).unwrap_or_else(|| fail("bad result file")))
            .collect()
    });
    let t0 = Instant::now();
    let ball = Ball::build(num("--ball", 6) as u8);
    let solver = Solver::new();
    eprintln!("ball {:?} and tables ready in {:.1} s", ball.levels, t0.elapsed().as_secs_f64());
    let beam = Beam {
        ball: &ball,
        solver: &solver,
        width: num("--width", 300_000),
        w_stage: num("--w-stage", 2) as i32,
        w_paired: num("--w-paired", 1) as i32,
        max_depth: num("--depth", walk.saturating_sub(4)),
    };
    let (mut found, mut sum, mut sum_theirs, mut within, mut ms) = (0usize, 0usize, 0usize, 0usize, 0.0);
    for i in 0..count {
        let (base, steps) = known_cube(&mut rnd, walk, false);
        let mut cube = Cube::from_moves(&base);
        cube.apply_seq(&steps);
        let t = Instant::now();
        let (path, looked) = beam.reduce(&cube);
        let secs = t.elapsed().as_secs_f64();
        ms += secs * 1e3;
        let len = path.as_ref().map(|p| {
            let mut c = cube;
            c.apply_seq(p);
            if !is_reduced(&c) {
                fail(&format!("cube {}: the beam's reduction does not reduce the cube", i));
            }
            p.len()
        });
        let s = theirs.as_ref().map(|t| t[i]);
        println!(
            "cube {:2}: known {}, beam {:?}, solver {:?}, {:.1} M cubes, {:.1} s",
            i,
            walk,
            len,
            s,
            looked as f64 / 1e6,
            secs
        );
        if let Some(l) = len {
            found += 1;
            sum += l;
            sum_theirs += s.unwrap_or(0);
            within += (l <= walk) as usize;
        }
    }
    println!(
        "{} cubes, {} moves from a reduced cube, width {}: beam found a reduction on {}, at most {} moves on {}",
        count, walk, beam.width, found, walk, within
    );
    if found > 0 {
        println!(
            "on those {}: beam {:.2}, solver {:.2}; mean time {:.1} s",
            found,
            sum as f64 / found as f64,
            sum_theirs as f64 / found as f64,
            ms / 1e3 / count as f64
        );
    }
}

/// 4x4x4 cubes with a known short answer. A random reduced cube (or the solved
/// cube with `--base solved`) gets `--walk` random moves out of all 36, so a
/// reduction (or a solution) of at most that many moves exists: the walk
/// undone. The solver is then run on the cube and its lengths are compared
/// with the known ones.
fn known(args: &[String]) {
    use cubenxnxn::c3::C3;
    use cubenxnxn::cube::inv_seq;
    let num = |name: &str, default: usize| {
        opt(args, name).map_or(default, |s| s.parse::<usize>().unwrap_or_else(|_| usage()))
    };
    let (walk, count) = (num("--walk", 0), num("--count", 50));
    if walk == 0 {
        usage();
    }
    let from_solved = match opt(args, "--base") {
        None | Some("reduced") => false,
        Some("solved") => true,
        Some(_) => usage(),
    };
    let mut seed = num("--seed", 4000 + walk) as u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    let mut out = opt(args, "--out").map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| fail(&format!("{}: {}", p, e))))
    });
    let t0 = Instant::now();
    let solver = Solver::new();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let params = params(args);
    eprintln!("{:?}", params);

    // Sums of: 3x3x3 length of the base, shortest reduction found, reduction used,
    // 3x3x3 used, total.
    let mut sums = [0usize; 5];
    let mut gap_hist = std::collections::BTreeMap::new();
    let mut times = Vec::new();
    for i in 0..count {
        let (base, steps) = known_cube(&mut rnd, walk, from_solved);
        let base_cube = Cube::from_moves(&base);
        let mut cube = base_cube;
        cube.apply_seq(&steps);
        // The known answer, checked: the walk undone, then the 3x3x3 of the base.
        let c3 = C3::from_cube(&base_cube).unwrap_or_else(|| fail("the base is not a 3x3x3 state"));
        let base333 = solver.t3.solve(&c3, 100_000, Some(&solver.big));
        let mut check = cube;
        check.apply_seq(&inv_seq(&steps));
        check.apply_seq(&base333);
        if !check.is_solved() {
            fail(&format!("cube {}: the known solution does not solve the cube", i));
        }

        let t = Instant::now();
        let sol = solver.solve(&cube, &params).unwrap_or_else(|e| fail(&format!("cube {}: {}", i, e)));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        times.push(ms);
        let used = sol.lens[0] + sol.lens[1] + sol.lens[2];
        for (k, v) in [base333.len(), sol.shortest_reduction, used, sol.lens[3], sol.moves.len()].into_iter().enumerate() {
            sums[k] += v;
        }
        *gap_hist.entry(sol.shortest_reduction as i64 - walk as i64).or_insert(0usize) += 1;
        if let Some(o) = out.as_mut() {
            writeln!(
                o,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.0}\t{}\t{}\t{}",
                i,
                walk,
                base333.len(),
                sol.shortest_reduction,
                used,
                sol.lens[3],
                sol.moves.len(),
                ms,
                format_moves(&base),
                format_moves(&steps),
                format_moves(&sol.moves)
            )
            .unwrap();
        }
    }
    let n = count as f64;
    let mean = |k: usize| sums[k] as f64 / n;
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("{} cubes, {} moves away from {}", count, walk, if from_solved { "the solved cube" } else { "a reduced cube" });
    if from_solved {
        println!("known to exist: a solution of {} moves", walk);
        println!("found: total {:.2}  (reduction {:.2} + 3x3x3 {:.2})", mean(4), mean(2), mean(3));
    } else {
        println!("known to exist: reduction {} + 3x3x3 {:.2} = {:.2}", walk, mean(0), walk as f64 + mean(0));
        println!(
            "found: shortest reduction {:.2}; solution {:.2}  (reduction {:.2} + 3x3x3 {:.2})",
            mean(1),
            mean(4),
            mean(2),
            mean(3)
        );
        println!("shortest reduction found minus the known one: {:?}", gap_hist);
    }
    println!(
        "time per solve: mean {:.0} ms, median {:.0} ms, max {:.0} ms",
        times.iter().sum::<f64>() / n,
        times[times.len() / 2],
        times[times.len() - 1]
    );
}

/// Experiment: is phase 3 longer than it has to be? A reduced cube gets `--walk`
/// random phase-3 moves; the shortest phase-3 solution is compared with the
/// shortest reduction that may use any move, found exactly up to
/// `--forward` + `--ball` moves.
fn free3(args: &[String]) {
    use cubenxnxn::cube::skip_move;
    use cubenxnxn::free::{is_reduced, key, Ball};
    use cubenxnxn::tables::P3_MOVES;
    let num = |name: &str, default: usize| {
        opt(args, name).map_or(default, |s| s.parse::<usize>().unwrap_or_else(|_| usage()))
    };
    let (walk, count, forward, depth) = (num("--walk", 9), num("--count", 20), num("--forward", 4), num("--ball", 5));
    let mut seed = num("--seed", 3000 + walk) as u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };

    // The key must not change when face turns and rotations are done first, and
    // must tell a reduced cube from one with a parity the 3x3x3 cannot fix.
    let ct = cubenxnxn::cube::tables();
    let reduced_key = key(&Cube::solved());
    for _ in 0..2000 {
        let w: Vec<u8> = (0..rnd(12)).map(|_| rnd(36) as u8).collect();
        let mut h: Vec<u8> = ct.rots[rnd(24)].seq.clone();
        h.extend((0..rnd(30)).map(|_| rnd(18) as u8));
        let k = key(&Cube::from_moves(&w));
        h.extend(&w);
        if key(&Cube::from_moves(&h)) != k {
            fail("the class key changed under face turns and rotations done first");
        }
        if (k == reduced_key) != is_reduced(&Cube::from_moves(&w)) {
            fail("the class key and the reduced test disagree");
        }
    }
    let swapped = Cube::from_moves(&parse_moves("Uw2 Rw2 U2 Rw2 R2 U2 Rw2 Uw2").unwrap());
    if is_reduced(&swapped) || key(&swapped) == reduced_key || swapped.with_home_centres().is_none() {
        fail("two swapped dedges pass for a reduced cube");
    }

    let t0 = Instant::now();
    let ball = Ball::build(depth as u8);
    eprintln!("classes by distance from a reduced cube: {:?}, {:.1} s", ball.levels, t0.elapsed().as_secs_f64());
    let solver = Solver::new();
    let mut table = std::collections::BTreeMap::new();
    let (mut sum3, mut sum_free, mut both) = (0usize, 0usize, 0usize);
    let t0 = Instant::now();
    for i in 0..count {
        let mut steps: Vec<u8> = Vec::new();
        while steps.len() < walk {
            let m = P3_MOVES[rnd(P3_MOVES.len())];
            if steps.last().is_none_or(|&l| !skip_move(l, m)) {
                steps.push(m);
            }
        }
        let cube = Cube::from_moves(&steps);
        let d3 = solver.phase3_length(&cube, walk).unwrap_or_else(|| fail(&format!("cube {}: phase 3 found nothing", i)));
        let free = ball.shortest(&cube, forward).map(|path| {
            let mut c = cube;
            c.apply_seq(&path);
            if !is_reduced(&c) {
                fail(&format!("cube {}: the free reduction does not reduce the cube", i));
            }
            path.len()
        });
        if let Some(f) = free {
            sum3 += d3;
            sum_free += f;
            both += 1;
        }
        *table.entry((d3, free)).or_insert(0usize) += 1;
    }
    println!("{} cubes, {} phase-3 moves away from a reduced cube; {:.1} s", count, walk, t0.elapsed().as_secs_f64());
    println!("(phase-3 length, shortest reduction with all moves; None = more than {}): cubes", forward + depth);
    for (k, v) in &table {
        println!("  {:?}: {}", k, v);
    }
    if both > 0 {
        println!(
            "where both are known ({} cubes): phase 3 {:.2}, all moves {:.2}",
            both,
            sum3 as f64 / both as f64,
            sum_free as f64 / both as f64
        );
    }
}

/// A walk of `len` moves out of the 27 that matter, away from a reduced cube:
/// it starts with a wide move and keeps face turns off the axis of its first moves,
/// so that undoing it is a reduction that cannot be shortened at sight.
fn free_walk(rnd: &mut impl FnMut(usize) -> usize, len: usize) -> Vec<u8> {
    let mut steps: Vec<u8> = Vec::new();
    while steps.len() < len {
        let m = rnd(27) as u8;
        let one_axis = steps.iter().all(|&s| s / 3 % 3 == m / 3 % 3);
        if (m < 18 && one_axis) || steps.last().is_some_and(|&l| cubenxnxn::cube::skip_move(l, m)) {
            continue;
        }
        steps.push(m);
    }
    steps
}

/// Experiment: what a cube K moves away from a reduced cube looks like, for
/// several K: staging distance of the centres (best orientation), dedges
/// already paired, dedges with one wing in each half (best frame).
fn free_features(args: &[String]) {
    use cubenxnxn::tables::axis_masks;
    let num = |name: &str, default: usize| {
        opt(args, name).map_or(default, |s| s.parse::<usize>().unwrap_or_else(|_| usage()))
    };
    let count = num("--count", 200);
    let mut seed = num("--seed", 77) as u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    let solver = Solver::new();
    println!("K      stage (mean, share <= 6, <= 8)   paired   split");
    for k in [4usize, 6, 8, 10, 12, 14, 16, 18, 20, 60] {
        let (mut stage, mut le6, mut le8, mut paired, mut split) = (0usize, 0usize, 0usize, 0usize, 0usize);
        for _ in 0..count {
            let cube = Cube::from_moves(&free_walk(&mut rnd, k));
            let variants = solver.variant_cubes(&cube);
            let st = variants
                .iter()
                .map(|(v, _)| {
                    let m = axis_masks(v);
                    solver.ctab.exact(&solver.tabs.bp, &solver.tabs.ranks, m[2], m[0])
                })
                .min()
                .unwrap() as usize;
            let sp = variants
                .iter()
                .map(|(v, _)| (0..12).fold(0u32, |a, s| a | 1 << (v.ep[s] % 12)).count_ones())
                .max()
                .unwrap() as usize;
            let pr = (0..12).filter(|&s| cube.ep[s + 12] == (cube.ep[s] + 12) % 24).count();
            stage += st;
            le6 += (st <= 6) as usize;
            le8 += (st <= 8) as usize;
            paired += pr;
            split += sp;
        }
        let n = count as f64;
        println!(
            "{:2}     {:5.2}  {:4.2}  {:4.2}                {:5.2}    {:5.2}",
            k,
            stage as f64 / n,
            le6 as f64 / n,
            le8 as f64 / n,
            paired as f64 / n,
            split as f64 / n
        );
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
    if n < 2 {
        fail("supported sizes: 2 and up");
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
        let found = stage.stage(&f, extra, per_job, 1, num("--a-budget", 0) as u64, deadline).unwrap_or_else(|e| fail(&e));
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
    let reduce_only = args.iter().any(|a| a == "--reduce");
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
        if reduce_only {
            // Only as far as a 3x3x3, the way the larger cubes use this solver for their inner layers.
            let moves = solver.reduce(&f, &p).unwrap_or_else(|e| fail(&format!("cube {}: {}", n, e)));
            times.push(t.elapsed().as_secs_f64() * 1e3);
            g.apply_seq(&mut f, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
            let reduced = cubenxnxn::c5::Cube5::from_facelets(&f).is_ok_and(|c| cubenxnxn::edges5::is_reduced(&c));
            if !reduced {
                fail(&format!("cube {}: the moves do not reduce the cube", n));
            }
            sums[0] += moves.len();
            *hist.entry(moves.len()).or_insert(0usize) += 1;
            n += 1;
            continue;
        }
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
    if reduce_only {
        println!("reduced {} cubes to 3x3x3, every result verified", n);
        println!("mean length of the reduction {:.3}", sums[0] as f64 / nf);
    } else {
        println!("solved {} cubes, every solution verified on the sticker model", n);
    }
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
    let work = |k: usize| cubenxnxn::edges5::POSITIONS[k].load(std::sync::atomic::Ordering::Relaxed) as f64 / nf / 1e6;
    println!(
        "positions per cube, millions: centres {:.2}, phase C {:.2}, P {:.2}, E {:.2}",
        cubenxnxn::stage5::POSITIONS.load(std::sync::atomic::Ordering::Relaxed) as f64 / nf / 1e6,
        work(0),
        work(1),
        work(2)
    );
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
    let (mut sums, mut total, mut ms, mut n, mut cpu) = ([0usize; 3], 0usize, 0.0, 0usize, 0.0);
    // Stagings returned, and the lengths of the first eight of each cube.
    let (mut kept, mut at, mut have) = (0usize, [0usize; 8], [0usize; 8]);
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = s6.g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = s6.g.solved();
        s6.g.apply_seq(&mut f, &scramble);
        let hold = num("--hold", 0);
        if hold > 0 {
            f = s6.g.held(&f, &s6.g.holds()[hold]);
        }
        let (t, c) = (Instant::now(), cubenxnxn::clock::cpu_ms());
        let wide = cubenxnxn::stage6::Wide {
            width,
            fan: num("--fan", width),
            beam: num("--beam", 20_000),
            first: num("--first", width),
        };
        let found = s6.stage_wide(&s5, &f, &wide, 0..3);
        kept += found.len();
        for (k, x) in found.iter().enumerate().take(8) {
            at[k] += x.moves.len();
            have[k] += 1;
        }
        ms += t.elapsed().as_secs_f64() * 1e3;
        cpu += cubenxnxn::clock::cpu_ms() - c;
        let Some(best) = found.first() else { fail(&format!("cube {}: no staging found", n)) };
        s6.g.apply_seq(&mut f, &best.moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
        if !s6.is_staged(&f) {
            fail(&format!("cube {}: the result is not staged", n));
        }
        for k in 0..3 {
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
    println!(
        "stagings per cube {:.1}; mean length of the first, second, ...: {}",
        kept as f64 / nf,
        (0..8).filter(|&k| have[k] > 0).map(|k| format!("{:.2}", at[k] as f64 / have[k] as f64)).collect::<Vec<_>>().join(" ")
    );
    println!("mean time {:.0} ms, processor time {:.0} ms", ms / nf, cpu / nf);
}

/// 6x6x6 on a scramble file: full solves, lengths per step and times.
fn bench6(args: &[String]) {
    use cubenxnxn::solver6::Solver6;
    let Some(path) = args.first() else { usage() };
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{}: {}", path, e)));
    let num = |name: &str, default: usize| opt(args, name).and_then(|s| s.parse().ok()).unwrap_or(default);
    let limit = num("--limit", usize::MAX);
    let p = params6(args);
    let t0 = Instant::now();
    let solver = Solver6::load();
    eprintln!("tables ready in {:.2} s", t0.elapsed().as_secs_f64());
    let g = &solver.s6.g;
    let (mut sums, mut times, mut n, mut cpu) = ([0usize; 4], Vec::new(), 0usize, 0.0);
    let mut hist = std::collections::BTreeMap::new();
    let mut out = opt(args, "--out").map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| fail(&format!("{}: {}", p, e))))
    });
    for scr in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(limit) {
        let scramble = g.parse_moves(scr).unwrap_or_else(|e| fail(&e));
        let mut f = g.solved();
        g.apply_seq(&mut f, &scramble);
        let (t, c) = (Instant::now(), cubenxnxn::clock::cpu_ms());
        let sol = solver.solve(&f, &p).unwrap_or_else(|e| fail(&format!("cube {}: {}", n, e)));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        cpu += cubenxnxn::clock::cpu_ms() - c;
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
        "time per cube: mean {:.0} ms, median {:.0} ms, max {:.0} ms; processor time {:.0} ms",
        times.iter().sum::<f64>() / nf,
        times[times.len() / 2],
        times[times.len() - 1],
        cpu / nf
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
        let t = Instant::now();
        let wide = cubenxnxn::stage7::Wide {
            width,
            fan: num("--fan", width),
            beam: num("--beam", 20_000),
            salt: num("--salt", 0) as u32,
            many: num("--many", 400),
            more: num("--more", 1),
            frames: num("--frames", 1) == 1,
            joint: num("--joint", 1) == 1,
            merged: num("--merged", 1) == 1,
            rest: num("--rest", 1) == 1,
            keep: num("--keep", width),
        };
        let found = s7.stage(&s5, &f, &wide);
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
    let d = match opt(args, "--mode") {
        Some("fast") => Params7::fast(),
        Some("normal") | None => Params7::normal(),
        Some(_) => usage(),
    };
    Params7 {
        width: num("--width", d.width),
        fan: num("--fan", d.fan),
        beam: num("--beam", d.beam),
        match_beam: num("--match-beam", d.match_beam),
        match_more: num("--match-more", d.match_more),
        restarts: num("--restarts", d.restarts),
        many: num("--many", d.many),
        more: num("--more", d.more),
        frames: num("--frames", d.frames as usize) == 1,
        joint: num("--joint", d.joint as usize) == 1,
        merged: num("--merged", d.merged as usize) == 1,
        rest: num("--rest", d.rest as usize) == 1,
        tries: num("--tries", d.tries),
        matchings: num("--matchings", d.matchings),
        inner: cubenxnxn::solver5::Params5 {
            labellings: num("--inner-labellings", d.inner.labellings),
            c_keep: num("--inner-c", d.inner.c_keep),
            p_keep: num("--inner-p", d.inner.p_keep),
            e_keep: num("--inner-e", d.inner.e_keep),
            ..d.inner
        },
        reductions: num("--reductions", d.reductions),
        outer: cubenxnxn::solver5::Params5 {
            labellings: num("--labellings", d.outer.labellings),
            c_keep: num("--c-keep", d.outer.c_keep),
            p_keep: num("--p-keep", d.outer.p_keep),
            e_keep: num("--e-keep", d.outer.e_keep),
            pool: num("--pool", d.outer.pool),
            probes: num("--probes", d.outer.probes as usize) as u64,
            time_ms: num("--time", d.outer.time_ms as usize) as u64,
            ..d.outer
        },
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
