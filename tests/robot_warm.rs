//! The time of a choice is kept: after the warm-up that the program does
//! when it starts (the tables read through, a few cubes solved), a choice
//! takes what `Options` allows and no more, from the first one on.
#![cfg(feature = "robots")]

use anycube::api::{check, Checked, Input};
use anycube::robot::fastest::{self, Options};
use anycube::robot::multicuber::optimize::Space;
use anycube::robot::multicuber::planner::{Lift, Planner, RobotState};
use anycube::robot::multicuber::timing::TimeModel;
use anycube::robot::physical::PhysicalCube;
use anycube::robot::Turn;

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn tables() {
    if std::env::var_os("ANYCUBE_TABLES").is_none() {
        std::env::set_var("ANYCUBE_TABLES", std::env::temp_dir().join("anycube-cli-tests"));
    }
}

fn scrambled(n: usize, moves: usize, rng: &mut Rng) -> Checked {
    let text: Vec<String> = (0..moves)
        .map(|_| {
            let depth = if n > 3 && rng.next(3) == 0 { 2 } else { 1 };
            Turn::Layers { face: rng.next(6) as u8, depth, turns: [1, -1, 2][rng.next(3)] }.name()
        })
        .collect();
    check(n, Input::Moves(&text.join(" ")), n).unwrap()
}

const AFTER_SCAN: RobotState = RobotState { lift: Lift::Level(0), flip_up: false };

/// The choice for a cube, held to solve it: by its moves, and by its plan on the sticker model.
fn chosen(c: &Checked, space: &Space, timing: &TimeModel, opts: &Options) -> fastest::Choice {
    let cost = |m: &[Turn]| space.cost(AFTER_SCAN, m);
    let choice = fastest::choose(c, &cost, opts).unwrap_or_else(|e| panic!("{}: {} with {:?}", e.code, e.message, opts));
    assert!(choice.candidates >= 1 && choice.solutions >= 1);
    assert!((cost(&choice.moves) - choice.est_ms).abs() < 1e-6);
    // The plain answer is a solution, costed by the same planner, and the chosen one is never slower.
    assert!((cost(&choice.plain.moves) - choice.plain.est_ms).abs() < 1e-6 && choice.est_ms <= choice.plain.est_ms);
    let mut plain = PhysicalCube::new(c.size);
    plain.facelets = c.facelets.clone();
    for t in &choice.plain.moves {
        let Turn::Layers { face, depth, turns } = *t else { panic!("the solver gives layer turns") };
        let power = match turns {
            1 => 0,
            2 => 1,
            _ => 2,
        };
        plain.g.apply(&mut plain.facelets, (face as usize + 6 * (depth as usize - 1)) * 3 + power);
    }
    assert!(plain.is_solved(), "the plain answer {:?} does not solve the cube", choice.plain.moves);
    let mut p = Planner::new(c.size).with_timing(timing).from_state(AFTER_SCAN);
    space.solution(&mut p, &choice.moves);
    let mut cube = PhysicalCube::new(c.size);
    cube.facelets = c.facelets.clone();
    (cube.lift, cube.flip_up) = (AFTER_SCAN.lift, AFTER_SCAN.flip_up);
    for op in &p.ops {
        cube.apply_line(&op.line());
    }
    assert!(cube.is_solved(), "the plan of {:?} does not solve the cube", choice.names());
    assert!(cube.violations.is_empty(), "{:?}", cube.violations);
    choice
}

/// After the warm-up, the first choices of a program already keep to their time: the plain
/// search's, the search for more and the costing, and no more. (A test file of its own, so that no
/// other test takes the cores meanwhile.)
#[test]
fn a_warmed_choice_keeps_its_time_from_the_first_one_on() {
    tables();
    let timing = TimeModel::for_size(3);
    let space = Space::new(3, &timing);
    let cost = |m: &[Turn]| space.cost(AFTER_SCAN, m);
    let opts = Options { plain_ms: 60.0, search_ms: 15.0, total_ms: 25.0, ..Options::default() };
    fastest::warm(3, &cost, &opts);
    // Done once: a second call has nothing to do.
    assert!(!fastest::warm(3, &cost, &opts));
    let mut rng = Rng(0xc01d);
    let (mut compute, mut candidates) = (Vec::new(), Vec::new());
    for _ in 0..30 {
        let choice = chosen(&scrambled(3, 30, &mut rng), &space, &timing, &opts);
        compute.push(choice.compute_ms);
        candidates.push(choice.candidates);
    }
    eprintln!("first choice after the warm-up: {:.1} ms, {} candidates; all 30: {:.1} ms at most", compute[0], candidates[0], compute.iter().cloned().fold(0.0, f64::max));
    compute.sort_by(f64::total_cmp);
    candidates.sort();
    let allowed = opts.plain_ms + opts.total_ms;
    assert!(compute[15] <= allowed + 10.0, "half the choices took more than {:.0} ms of {:.0}", compute[15], allowed);
    // (Room for a machine that is busy with something else.)
    assert!(compute[29] <= 2.0 * allowed + 50.0, "a choice took {:.0} ms of {:.0}", compute[29], allowed);
    assert!(candidates[15] >= 5, "half the choices had fewer than {} candidates", candidates[15]);
}
