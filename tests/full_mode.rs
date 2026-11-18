//! Wider reduction searches can displace a short finish found at the standard width.
use anycube::c5::tables5;
use anycube::cube::Cube;
use anycube::solver::{Params, Solver};
use anycube::solver5::{Params5, Solver5};

#[test]
#[ignore = "runs two full solves with the native lookup tables"]
fn full_keeps_shorter_solutions_from_compact_reductions() {
    // The wider searches alone return 41 and 66 moves on these sticker states.
    let mut cube = Cube::from_facelets(
        "BULBBFRBFBDLUDBFLUULRBUFRBFUDRRRRRDULLFFDUDFBFLRLDDFFRRDLBLBFBBDRUUFURLFBDFBLFUDUDRUDLULRUDLBRLD",
    ).unwrap();
    let sol = Solver::new().solve(&cube, &Params::full()).unwrap();
    assert!(sol.moves.len() <= 39);
    assert_eq!(sol.lens.iter().sum::<usize>(), sol.moves.len());
    cube.apply_seq(&sol.moves);
    assert!(cube.is_solved());

    let g = &tables5().geometry;
    let mut stickers = g.from_string(
        "DDLFLBUDDBURUBRRDDUBLRLBRDRBRFLBBRBRFRFRLFFBUUFLLLDUFUFUUUDDURFLFUBBUFRFBFBURDRLDDDRUDDDUBDRLLDFLDLDRLFDBBLLFFURLLBBFBLLRBFFBUDDRBDBFLUURBULRRUFLFURFU",
    ).unwrap();
    let sol = Solver5::load().solve(&stickers, &Params5::full()).unwrap();
    assert!(sol.moves.len() <= 65);
    assert_eq!(sol.lens.iter().sum::<usize>(), sol.moves.len());
    g.apply_seq(
        &mut stickers,
        &sol.moves.into_iter().map(usize::from).collect::<Vec<_>>(),
    );
    assert!(g.is_solved(&stickers));
}
