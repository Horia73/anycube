//! A face turn and the corresponding wide turn act alike on the 3x3x3
//! through the middle of a 5x5x5. Search for short words on that cube, then
//! assign widths and, where necessary, restore turns invisible to the core.
//! On a 4x4x4 a verified reduction supplies one choice of virtual middle edges.

mod shell;
mod widths;
pub use widths::Widths;

use crate::c3::{C3, T3};
use crate::c5::{tables5, Cube5};
use crate::cube::{inv_move, perm_parity, tables, Cube, SOLVED_CT};
use crate::cuben::Geometry;
use crate::solver::{Params, Solver};
use crate::solver5::{Params5, Solver5};
use crate::two3::{Effort, Two3};
use rayon::prelude::*;
use std::sync::OnceLock;

/// Maximum number of search nodes tried per spelling of a projected word.
pub const DEFAULT_WIDTH_BUDGET: usize = 25_000;

// Ordered as wings, x centres, t centres. Each entry gives the source of
// one destination. The same representation permutes pieces or their domains.
type Orbits<T> = [[T; 24]; 3];

struct Choice {
    moves: Vec<u8>,
    src: Orbits<u8>,
    dest: Orbits<u8>,
    extra: usize,
}

impl Choice {
    fn new(moves: Vec<u8>, extra: usize) -> Self {
        let mut src: Orbits<u8> = [std::array::from_fn(|i| i as u8); 3];
        for &m in &moves {
            let m = m as usize;
            let step = [&tables().ep_src[m], &tables().ct_src[m], &tables5().t_src[m]];
            src = std::array::from_fn(|o| std::array::from_fn(|i| src[o][step[o][i] as usize]));
        }
        let mut dest = [[0; 24]; 3];
        for o in 0..3 {
            for i in 0..24 {
                dest[o][src[o][i] as usize] = i as u8;
            }
        }
        Self {
            moves,
            src,
            dest,
            extra,
        }
    }

    fn apply(&self, p: &Orbits<u8>) -> Orbits<u8> {
        std::array::from_fn(|o| std::array::from_fn(|i| p[o][self.src[o][i] as usize]))
    }
}

struct Spellings {
    face: [Vec<Choice>; 18],
    gap: Vec<Choice>,
}

fn spellings() -> &'static Spellings {
    static ALL: OnceLock<Spellings> = OnceLock::new();
    ALL.get_or_init(|| {
        let face = std::array::from_fn(|m| {
            let m = m as u8;
            let mut choices = vec![Choice::new(vec![m], 0), Choice::new(vec![m + 18], 0)];
            // All ways to divide the net turn between a face turn and a
            // wide turn of this face. The two turns commute.
            for wide in 1..=3 {
                let narrow = (m % 3 + 1 + 4 - wide) % 4;
                if narrow != 0 {
                    let base = m / 3 * 3;
                    choices.push(Choice::new(vec![base + narrow - 1, base + 18 + wide - 1], 1));
                }
            }
            choices
        });
        let mut gap = vec![Choice::new(Vec::new(), 0)];
        for m in 0..18 {
            gap.push(Choice::new(vec![inv_move(m), m + 18], 2));
        }
        Spellings { face, gap }
    })
}

fn pieces(c: &Cube5, five: bool) -> Orbits<u8> {
    [c.c4.ep, c.c4.ct, if five { c.t } else { [0; 24] }]
}

fn goal(five: bool) -> Orbits<u8> {
    [
        std::array::from_fn(|i| i as u8),
        SOLVED_CT,
        if five { SOLVED_CT } else { [0; 24] },
    ]
}

type Domains = Orbits<u32>;

fn possible(d: &Domains, p: &Orbits<u8>) -> bool {
    (0..3).all(|o| (0..24).all(|i| d[o][i] & (1 << p[o][i]) != 0))
}

struct Step {
    choices: &'static [Choice],
    allowed: Vec<Domains>,
}

// Work backwards from solved. For each remaining move allowance, a bit in
// a slot's mask means that this piece could finish there under some width
// choices. Different pieces may require incompatible choices; the forward
// search commits to one physical operation shared by all three orbits.
fn steps(word: &[u8], five: bool, extra: usize) -> Vec<Step> {
    let spells = spellings();
    let mut positions = Vec::new();
    for &m in word {
        if extra >= 2 {
            positions.push(spells.gap.as_slice());
        }
        positions.push(spells.face[m as usize].as_slice());
    }
    if extra >= 2 {
        positions.push(spells.gap.as_slice());
    }
    let solved = goal(five).map(|orbit| orbit.map(|v| 1 << v));
    let mut steps = vec![Step {
        choices: &[],
        allowed: vec![solved; extra + 1],
    }];
    for options in positions.into_iter().rev() {
        let next = &steps.last().unwrap().allowed;
        let mut allowed = vec![[[0; 24]; 3]; extra + 1];
        for (slack, mask) in allowed.iter_mut().enumerate() {
            for choice in options.iter().take_while(|c| c.extra <= slack) {
                let after = &next[slack - choice.extra];
                for o in 0..3 {
                    for i in 0..24 {
                        mask[o][i] |= after[o][choice.dest[o][i] as usize];
                    }
                }
            }
        }
        steps.push(Step {
            choices: options,
            allowed,
        });
    }
    steps.reverse();
    steps
}

fn assign(steps: &[Step], p: &Orbits<u8>, slack: usize, budget: &mut usize, path: &mut Vec<u8>) -> bool {
    let step = &steps[0];
    if !possible(&step.allowed[slack], p) {
        return false;
    }
    if step.choices.is_empty() {
        return true;
    }
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let saved = path.len();
    for choice in step.choices.iter().take_while(|c| c.extra <= slack) {
        path.extend_from_slice(&choice.moves);
        if assign(&steps[1..], &choice.apply(p), slack - choice.extra, budget, path) {
            return true;
        }
        path.truncate(saved);
    }
    false
}

fn lift(start: &Orbits<u8>, five: bool, word: &[u8], extra: usize) -> Option<Vec<u8>> {
    if extra == 0 {
        return match widths::recover(start, five, word, DEFAULT_WIDTH_BUDGET) {
            Widths::Found(path) => Some(path),
            _ => None,
        };
    }
    let mut out = Vec::new();
    let mut budget = DEFAULT_WIDTH_BUDGET;
    assign(&steps(word, five, extra), start, extra, &mut budget, &mut out).then_some(out)
}

fn core(c: &Cube5) -> C3 {
    C3 {
        ep: c.mid.ep,
        eo: c.mid.eo,
        cp: c.c4.cp,
        co: c.c4.co,
    }
}

// Each slot of a 4x4x4 has two possible witnesses for the missing middle
// edge. Choose a bijection of the twelve edge names, with the flip and
// permutation parities of a legal 3x3x3. Some true virtual middles have
// moved away from both wings, so keep the reduction's own projection too.
fn projections(c: &Cube5, five: bool) -> Vec<C3> {
    fn match_slot(c: &Cube5, slot: usize, used: u16, mid: &mut C3, out: &mut Vec<C3>) {
        if slot == 12 {
            if mid.eo.iter().sum::<u8>() % 2 == 0
                && perm_parity(&mid.ep) == perm_parity(&mid.cp)
                && !out.contains(mid)
            {
                out.push(*mid);
            }
            return;
        }
        let lo = c.c4.ep[slot];
        let hi = (c.c4.ep[slot + 12] + 12) % 24;
        for (i, wing) in [lo, hi].into_iter().enumerate() {
            let id = wing % 12;
            if (i == 1 && hi == lo) || used & (1 << id) != 0 {
                continue;
            }
            mid.ep[slot] = id;
            mid.eo[slot] = wing / 12;
            match_slot(c, slot + 1, used | (1 << id), mid, out);
        }
    }
    let mut mid = core(c);
    let mut out = vec![mid];
    if !five {
        match_slot(c, 0, 0, &mut mid, &mut out);
    }
    out
}

fn projected_words(t: &T3, two: &Two3, c: &C3, cap: usize) -> Vec<Vec<u8>> {
    let mut pairs = vec![(None, None)];
    for m in 0..18u8 {
        pairs.extend([(Some(m), None), (None, Some(m))]);
    }
    pairs
        .par_iter()
        .filter_map(|&(first, last)| {
            let mut state = *c;
            if let Some(f) = first {
                state = state.mul(&t.moves[f as usize]);
            }
            if let Some(l) = last {
                state = t.moves[l as usize].mul(&state);
            }
            let room = cap.checked_sub(usize::from(first.is_some()) + usize::from(last.is_some()))?;
            let depth = if first.is_none() && last.is_none() { 13 } else { 11 };
            let sol = two.solve_within(
                t,
                &state,
                Effort {
                    depth: room.min(depth),
                    variants: 6,
                },
                room,
            )?;
            Some(first.into_iter().chain(sol).chain(last).collect())
        })
        .collect()
}

fn try_from(t: &T3, two: &Two3, cube: &Cube5, five: bool, cap: usize) -> Option<Vec<u8>> {
    let start = pieces(cube, five);
    let cap = cap.min(22);
    let mut words: Vec<_> = projections(cube, five)
        .iter()
        .take(8)
        .flat_map(|c| projected_words(t, two, c, cap))
        .collect();
    words.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    words.dedup();
    let mut jobs: Vec<_> = words
        .iter()
        .flat_map(|w| (0..=2.min(cap - w.len())).map(move |extra| (w, extra)))
        .collect();
    jobs.sort_by_key(|(word, extra)| word.len() + extra);
    jobs.par_iter()
        .find_map_first(|(w, extra)| lift(&start, five, w, *extra))
}

/// Search the input and checkpoints near the last wide turn. The colours of
/// the solved endpoint define the frame, including on an even cube. Going
/// back along a verified solution also provides a legal virtual 3x3x3.
fn improve_tail(t: &T3, two: &Two3, five: bool, path: &[u8], bound: usize) -> Option<Vec<u8>> {
    let last = path.iter().rposition(|m| *m >= 18).map_or(0, |k| k + 1);
    let mut at = Cube5::solved();
    let mut best = None;
    let mut limit = bound;
    for cut in (0..path.len()).rev() {
        at.apply(inv_move(path[cut]));
        if cut > last || (cut > 0 && cut + 10 < last) {
            continue;
        }
        if let Some(cap) = limit.checked_sub(cut + 1) {
            if let Some(sol) = try_from(t, two, &at, five, cap) {
                let mut found = path[..cut].to_vec();
                found.extend(sol);
                limit = found.len();
                best = Some(found);
            }
        }
    }
    best
}

/// Assign a width to each turn of a supplied face word, with no phase
/// restrictions or large pruning tables. The word includes directions and
/// powers. It must already solve the fixed 3x3x3 core on a 5x5x5, or the
/// corners in some orientation on a 4x4x4. Check the result on the input.
pub fn recover_widths(g: &Geometry, facelets: &[u8], word: &[u8], budget: usize) -> Result<Widths, String> {
    if !matches!(g.n, 4 | 5) || facelets.len() != 6 * g.n * g.n || facelets.iter().any(|&c| c >= 6) {
        return Err("expected the facelets of a 4x4x4 or 5x5x5".into());
    }
    crate::api::check::validate(g.n, facelets).map_err(|e| e.message)?;
    if word.iter().any(|&m| m >= 18) {
        return Err("the projected word must contain only face turns".into());
    }
    let five = g.n == 5;
    let start = if five {
        let c = Cube5::from_facelets(facelets)?;
        let mut narrow = c;
        narrow.apply_seq(word);
        if core(&narrow) != C3::identity() {
            return Ok(Widths::Impossible);
        }
        pieces(&c, true)
    } else {
        let c = Cube::from_facelets(&g.to_string(facelets))?;
        let mut narrow = c;
        narrow.apply_seq(word);
        let t = tables();
        let Some(r) = t
            .rots
            .iter()
            .position(|r| r.state.cp == narrow.cp && r.state.co == narrow.co)
        else {
            return Ok(Widths::Impossible);
        };
        let home = c.after_rotation(&t.rots[t.rot_inv[r]].state);
        [home.ep, home.ct, [0; 24]]
    };
    let result = widths::recover(&start, five, word, budget);
    if let Widths::Found(moves) = &result {
        verify(g, facelets, moves)?;
    }
    Ok(result)
}

fn verify(g: &Geometry, facelets: &[u8], moves: &[u8]) -> Result<(), String> {
    let mut check = facelets.to_vec();
    g.apply_seq(&mut check, &moves.iter().map(|&m| m as usize).collect::<Vec<_>>());
    if !g.is_solved(&check) {
        return Err("width assignment did not solve the original stickers".into());
    }
    Ok(())
}

// A face turn only permutes centres within each face. For a cube G and a
// face turn F we can therefore solve F G, then append F in the returned
// solved frame. The hidden ordering of same-colour centres cannot matter.
fn append_last_face(start: &Cube, mut moves: Vec<u8>, face: u8) -> Option<Vec<u8>> {
    let t = tables();
    let mut end = *start;
    end.apply_seq(&moves);
    let frame = t.rots.iter().position(|r| r.state == end)?;
    let last = t.relabel[t.rot_inv[frame]][face as usize];
    for i in (0..moves.len()).rev() {
        if moves[i] / 3 % 3 != last / 3 % 3 {
            break;
        }
        if moves[i] / 3 == last / 3 {
            let power = (moves[i] % 3 + last % 3 + 2) % 4;
            if power == 0 {
                moves.remove(i);
            } else {
                moves[i] = last / 3 * 3 + power - 1;
            }
            return Some(moves);
        }
    }
    moves.push(last);
    Some(moves)
}

fn try_last_faces(solver: &Solver, path: &[u8]) -> Option<Vec<u8>> {
    let inverse = crate::cube::inv_seq(path);
    let mut best = None;
    let mut limit = path.len();
    let params = Params::fast();
    for face in 0..18 {
        let mut start = Cube::from_moves(&[face]);
        start.apply_seq(&inverse);
        let mut proposals = Vec::new();
        if let Ok(winner) = solver.solve_with_alternatives(&start, &params, Some(&mut proposals)) {
            proposals.push(winner.moves);
        }
        for word in proposals {
            // Appending one face can remove at most one existing move.
            if word.len() <= limit {
                if let Some(found) = append_last_face(&start, word, face) {
                    if found.len() < limit {
                        limit = found.len();
                        best = Some(found);
                    }
                }
            }
        }
    }
    best
}

/// An experimental quality pass for a standalone 4x4x4 or 5x5x5.
pub enum Search<'a> {
    Four {
        solver: &'a Solver,
        params: &'a Params,
        last_face: bool,
    },
    Five {
        solver: &'a Solver5,
        params: &'a Params5,
        shell: Option<&'a Solver>,
    },
}

/// Lengths after the ordinary solve, the requested orientations and the
/// projected 3x3x3 search. The final moves include any subsequent improvement.
pub struct ResultLengths {
    pub normal: usize,
    pub held: usize,
    pub projected: usize,
    pub moves: Vec<u8>,
}

impl Search<'_> {
    pub fn solve(&self, g: &Geometry, f: &[u8], holds: usize) -> Result<ResultLengths, String> {
        let five = matches!(self, Self::Five { .. });
        let size = if five { 5 } else { 4 };
        if g.n != size || f.len() != 6 * g.n * g.n || f.iter().any(|&v| v >= 6) {
            return Err("invalid facelets for the selected cube size".into());
        }
        if !(1..=6).contains(&holds) {
            return Err("holds must be between 1 and 6".into());
        }
        let (t, two) = match self {
            Self::Four { solver, .. } => (&solver.t3, &solver.two),
            Self::Five { solver, .. } => (&solver.edges.t3, &solver.two),
        };
        crate::api::check::validate(size, f).map_err(|e| e.message)?;
        if g.is_solved(f) {
            return Ok(ResultLengths {
                normal: 0,
                held: 0,
                projected: 0,
                moves: Vec::new(),
            });
        }
        let mut seeds = Vec::new();
        let mut winners = Vec::new();
        for turns in g.holds().iter().take(holds) {
            let framed = g.held(f, turns);
            let mut alts = Vec::new();
            let solution = match self {
                Self::Four { solver, params, .. } => {
                    solver
                        .solve_with_alternatives(
                            &Cube::from_facelets(&g.to_string(&framed))?,
                            params,
                            Some(&mut alts),
                        )?
                        .moves
                }
                Self::Five { solver, params, .. } => {
                    solver.solve_with_alternatives(&framed, params, &mut alts)?.moves
                }
            };
            let map = g.frame_moves(turns);
            let original = |word: &[u8]| word.iter().map(|&m| map[m as usize] as u8).collect::<Vec<u8>>();
            winners.push(original(&solution));
            seeds.extend(alts.iter().map(|w| original(w)));
        }
        let normal = winners[0].len();
        winners.sort_by_key(Vec::len);
        let mut moves = winners[0].clone();
        let held = moves.len();
        winners.extend(seeds);
        winners.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        winners.dedup();
        for seed in winners.iter().take(4) {
            if let Some(found) = improve_tail(t, two, five, seed, moves.len()) {
                moves = found;
            }
        }
        let projected = moves.len();
        let improved = match self {
            Self::Four {
                solver,
                last_face: true,
                ..
            } => try_last_faces(solver, &moves),
            Self::Five {
                shell: Some(solver), ..
            } => shell::improve(solver, &moves, projected),
            _ => None,
        };
        if let Some(found) = improved {
            moves = found;
        }
        verify(g, f, &moves)?;
        Ok(ResultLengths {
            normal,
            held,
            projected,
            moves,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::{inv_seq, parse_moves};

    fn start(solution: &[u8]) -> Cube5 {
        let mut c = Cube5::solved();
        c.apply_seq(&inv_seq(solution));
        c
    }

    fn verify(size: usize, scramble: &[u8], answer: &[u8]) {
        let g = Geometry::new(size);
        let mut f = g.solved();
        for &m in scramble.iter().chain(answer) {
            g.apply(&mut f, m as usize);
        }
        assert!(g.is_solved(&f));
    }

    #[test]
    fn matches_real_centres_and_wings_with_mixed_widths() {
        let original = parse_moves("Rw U' Bw2 L Dw2 F' Uw").unwrap();
        let word: Vec<_> = original.iter().map(|m| m % 18).collect();
        for (size, five) in [(4, false), (5, true)] {
            let p = pieces(&start(&original), five);
            let found = lift(&p, five, &word, 0).unwrap();
            assert_eq!(found.len(), word.len());
            verify(size, &inv_seq(&original), &found);
            assert!(lift(&p, five, &word[1..], 0).is_none());
        }
    }

    #[test]
    fn final_face_uses_the_auxiliary_solved_frame() {
        let scramble = parse_moves("Rw U Fw2 L'").unwrap();
        let answer = inv_seq(&scramble);
        for face in 0..18u8 {
            let mut aux = Cube::from_moves(&[face]);
            aux.apply_seq(&scramble);
            for r in &tables().rots {
                let mut word = answer.clone();
                word.push(inv_move(face));
                word.extend_from_slice(&r.seq);
                let found = append_last_face(&aux, word, face).unwrap();
                verify(4, &scramble, &found);
            }
        }
    }

    #[test]
    fn restores_a_split_turn_and_a_turn_invisible_to_the_core() {
        for (physical, projected, extra) in [("R' Rw2", "R", 1), ("R' Rw", "", 2), ("F U' Uw B", "F B", 2)] {
            let path = parse_moves(physical).unwrap();
            let word = parse_moves(projected).unwrap();
            for (size, five) in [(4, false), (5, true)] {
                let p = pieces(&start(&path), five);
                assert!(lift(&p, five, &word, extra - 1).is_none());
                let found = lift(&p, five, &word, extra).unwrap();
                assert_eq!(found.len(), word.len() + extra);
                verify(size, &inv_seq(&path), &found);
            }
        }
    }

    #[test]
    fn reconstructs_a_virtual_middle_from_either_wing() {
        let mut c = start(&parse_moves("U Fw2 R'").unwrap());
        let actual = core(&c);
        // Use a different legal projection of the same corner parity as
        // the supplied virtual middle. The wing witnesses recover the real one.
        c.mid = C3::identity();
        assert_ne!(core(&c), actual);
        assert!(projections(&c, false).contains(&actual));
        assert_eq!(projections(&c, true), vec![core(&c)]);
    }

    #[test]
    fn supplied_words_recover_every_small_width_assignment_in_any_hold() {
        let word = parse_moves("F2 U' R B2").unwrap();
        for size in [4, 5] {
            let g = Geometry::new(size);
            let target = if size == 4 {
                g.turned(&g.solved(), &[1, 0])
            } else {
                g.solved()
            };
            for mask in 0..1 << word.len() {
                let path: Vec<_> = word
                    .iter()
                    .enumerate()
                    .map(|(i, &m)| m + 18 * ((mask >> i) & 1) as u8)
                    .collect();
                let mut f = target.clone();
                for m in inv_seq(&path) {
                    g.apply(&mut f, m as usize);
                }
                let Widths::Found(found) = recover_widths(&g, &f, &word, 100).unwrap() else {
                    panic!("missed size {size}, mask {mask}")
                };
                assert_eq!(found.iter().map(|m| m % 18).collect::<Vec<_>>(), word);
                for m in found {
                    g.apply(&mut f, m as usize);
                }
                assert!(g.is_solved(&f));
            }
        }
    }

    #[test]
    fn impossible_widths_and_an_unfinished_search_are_distinct() {
        for size in [4, 5] {
            let g = Geometry::new(size);
            let mut f = g.solved();
            let word = parse_moves("U").unwrap();
            for m in inv_seq(&word) {
                g.apply(&mut f, m as usize);
            }
            assert_eq!(recover_widths(&g, &f, &word, 0).unwrap(), Widths::BudgetExhausted);
            // A pure inner turn does nothing to the corners or middle edges.
            for m in parse_moves("F' Fw").unwrap() {
                g.apply(&mut f, m as usize);
            }
            assert_eq!(recover_widths(&g, &f, &word, 100).unwrap(), Widths::Impossible);
            assert!(recover_widths(&g, &f, &[18], 100).is_err());
        }
    }
}
