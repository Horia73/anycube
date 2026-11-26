//! The optimizing planner (off by default): the quickest way, under the
//! time model, to make a solution's moves, searched over the robot's states
//! (lift position, flipper side, which face is where) instead of following
//! the 2018 rules.
//!
//! It uses only the steps the legacy planner itself makes, each under the
//! same condition: the lift between levels, between 0 and the scanning
//! height (never from the scanning height to a turning level); the flipper
//! carrying the cube with the lift at 0, alone with it anywhere else; the
//! rotator turning the whole cube at level n and k layers at level k. The
//! commands come out of the legacy planner's own primitives, so their
//! bookkeeping (frames, names) is the same, and the tests run every plan on
//! the sticker model and check that it solves the cube.

use super::planner::{turn_perm, Lift, Planner, PERM_X, PERM_Y};
use super::timing::TimeModel;
use crate::robot::Turn;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const U: u8 = 0;
const D: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    Lift(Lift),
    Flip(bool),
    /// The whole cube at level n.
    Whole(i8),
}

#[derive(Clone, Copy)]
struct Node {
    lift: Lift,
    flip_up: bool,
    frame: [u8; 6],
}

struct Space {
    n: u8,
}

impl Space {
    fn lift_index(&self, l: Lift) -> usize {
        match l {
            Lift::Level(k) => k as usize,
            Lift::Scan => self.n as usize + 1,
        }
    }

    /// Lift, flipper and the frame (given by where U and F are) as one number.
    fn key(&self, s: &Node) -> usize {
        ((self.lift_index(s.lift) * 2 + s.flip_up as usize) * 6 + s.frame[0] as usize) * 6 + s.frame[2] as usize
    }

    fn size(&self) -> usize {
        (self.n as usize + 2) * 2 * 36
    }

}

fn turned(frame: [u8; 6], perm: &[u8; 6], turns: i8) -> [u8; 6] {
    let p = turn_perm(perm, turns);
    std::array::from_fn(|f| p[frame[f] as usize])
}

/// The steps allowed from a state, with where they lead and what they cost (ms).
fn steps(sp: &Space, s: &Node, t: &TimeModel, out: &mut Vec<(Act, Node, f64)>) {
    out.clear();
    for to in (0..=sp.n).map(Lift::Level).chain([Lift::Scan]) {
        if to == s.lift {
            continue;
        }
        let scan_hop = (s.lift == Lift::Scan && to != Lift::Level(0)) || (to == Lift::Scan && s.lift != Lift::Level(0));
        if scan_hop {
            continue;
        }
        let ms = t.lift_pair_ms(s.lift, to) + t.overhead_ms;
        out.push((Act::Lift(to), Node { lift: to, ..*s }, ms));
    }
    let up = !s.flip_up;
    let carry = s.lift == Lift::Level(0);
    let frame = if carry { turned(s.frame, &PERM_X, if up { 1 } else { -1 }) } else { s.frame };
    let ms = t.flip_move_ms(carry) + t.overhead_ms;
    out.push((Act::Flip(up), Node { flip_up: up, frame, ..*s }, ms));
    if s.lift == Lift::Level(sp.n) {
        for turns in [1, -1, 2] {
            let ms = t.rot_turn_ms(turns == 2) + t.overhead_ms;
            out.push((Act::Whole(turns), Node { frame: turned(s.frame, &PERM_Y, turns), ..*s }, ms));
        }
    }
}

/// Cost as an integer (0.01 ms) so that the search is the same on every machine.
fn cost(ms: f64) -> u64 {
    (ms * 100.0).round() as u64
}

/// Appends to `p` (from its state) the quickest commands for `moves` and the
/// 2018 finish (lift down, flipper down and up), as `Planner::solution` does.
pub fn solution(p: &mut Planner, moves: &[Turn], t: &TimeModel) {
    p.start_run();
    let sp = Space { n: p.n };
    let size = sp.size();
    let start = Node { lift: p.lift, flip_up: p.flip_up, frame: p.frame };
    // Per layer (before each move): best cost, where it came from within the layer,
    // and for the sources, the state of the previous layer that the move came from.
    let mut layers: Vec<Vec<(u64, Option<(usize, Act)>, Option<usize>, Node)>> = Vec::new();
    let mut sources: Vec<(usize, u64, Option<usize>, Node)> = vec![(sp.key(&start), 0, None, start)];
    let mut buf = Vec::new();
    for (i, &mv) in moves.iter().enumerate() {
        let mut layer: Vec<(u64, Option<(usize, Act)>, Option<usize>, Node)> = vec![(u64::MAX, None, None, start); size];
        let mut heap = BinaryHeap::new();
        for &(k, c, from, node) in &sources {
            if c < layer[k].0 {
                layer[k] = (c, None, from, node);
                heap.push(Reverse((c, k)));
            }
        }
        if let Turn::Whole { .. } = mv {
            // A rename: nothing moves, the frames are renamed in every state.
            let mut next = Vec::new();
            for &(_, c, _, node) in &sources {
                let mut q = Planner::new(sp.n as usize, p.b_rule);
                q.frame = node.frame;
                q.turn(mv, i);
                let renamed = Node { frame: q.frame, ..node };
                next.push((sp.key(&renamed), c, Some(sp.key(&node)), renamed));
            }
            layers.push(layer);
            sources = next;
            continue;
        }
        let Turn::Layers { face, depth, turns } = mv else { unreachable!() };
        // Dijkstra over the steps that bring the face to the top or the bottom.
        while let Some(Reverse((c, k))) = heap.pop() {
            if c > layer[k].0 {
                continue;
            }
            let node = layer[k].3;
            steps(&sp, &node, t, &mut buf);
            for &(act, next, ms) in &buf {
                let nk = sp.key(&next);
                let nc = c + cost(ms);
                if nc < layer[nk].0 {
                    layer[nk] = (nc, Some((k, act)), None, next);
                    heap.push(Reverse((nc, nk)));
                }
            }
        }
        // The move itself, from every state where the face is at U or D.
        let mut best: Vec<Option<(u64, usize, Node)>> = vec![None; size];
        for (k, &(c, _, _, node)) in layer.iter().enumerate() {
            if c == u64::MAX {
                continue;
            }
            let pos = node.frame[face as usize];
            if pos != U && pos != D {
                continue;
            }
            let level = if pos == U { depth } else { sp.n - depth };
            let mut ms = 0.0;
            if node.lift != Lift::Level(level) {
                // From the scanning height the lift goes to 0 first, as the legacy planner's always does.
                if node.lift == Lift::Scan {
                    continue;
                }
                ms += t.lift_pair_ms(node.lift, Lift::Level(level)) + t.overhead_ms;
            }
            ms += t.rot_turn_ms(turns == 2) + t.overhead_ms;
            let frame = if pos == D || level == sp.n { turned(node.frame, &PERM_Y, turns) } else { node.frame };
            let next = Node { lift: Lift::Level(level), flip_up: node.flip_up, frame };
            let nk = sp.key(&next);
            let nc = c + cost(ms);
            if best[nk].is_none_or(|b| nc < b.0) {
                best[nk] = Some((nc, k, next));
            }
        }
        layers.push(layer);
        sources = best.iter().enumerate().filter_map(|(k, b)| b.map(|(c, from, node)| (k, c, Some(from), node))).collect();
    }
    // The finish, from each end state.
    let finish = |node: &Node| -> f64 {
        let mut q = Planner::costing(sp.n as usize, p.b_rule, t);
        q.lift = node.lift;
        q.flip_up = node.flip_up;
        q.lift_to(Lift::Level(0));
        q.flip(false, false);
        q.flip(true, false);
        q.ms
    };
    let &(_, _, mut from, _) = sources.iter().min_by_key(|(_, c, _, node)| (c + cost(finish(node)), *c)).expect("a way always exists");
    // Back through the layers: the steps of each, then the move that left it.
    let mut plan: Vec<(Vec<Act>, Option<usize>)> = Vec::new();
    for li in (0..layers.len()).rev() {
        let layer = &layers[li];
        let mut at = from.expect("each layer was left by a move");
        let mut acts = Vec::new();
        while let Some((prev, act)) = layer[at].1 {
            acts.push(act);
            at = prev;
        }
        acts.reverse();
        plan.push((acts, Some(li)));
        from = layer[at].2;
    }
    plan.reverse();
    for (acts, mi) in plan {
        for act in acts {
            match act {
                Act::Lift(l) => p.lift_to(l),
                Act::Flip(up) => p.flip(up, false),
                Act::Whole(turns) => p.rot(turns, true, None),
            }
        }
        let i = mi.expect("one move per layer");
        match moves[i] {
            Turn::Whole { .. } => p.turn(moves[i], i),
            Turn::Layers { face, depth, turns } => {
                let pos = p.frame[face as usize];
                debug_assert!(pos == U || pos == D);
                let level = if pos == U { depth } else { p.n - depth };
                p.lift_to(Lift::Level(level));
                p.rot(turns, pos == D || level == p.n, Some(i as u16));
            }
        }
    }
    p.lift_to(Lift::Level(0));
    p.flip(false, false);
    p.flip(true, false);
}
