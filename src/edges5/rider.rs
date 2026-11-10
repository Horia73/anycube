//! The edge phases with a rider: a second state that every move changes as
//! well and that must reach a goal of its own by the end of the phase.
//!
//! This is for larger cubes whose inner layers form a 5x5x5. The moves here
//! are those of a 7x7x7 (`face * 3 + power`, plus 18 per extra layer), in
//! the frame of the phase: a turn of three layers is the wide turn of the
//! inner 5x5x5 and a turn of two layers is a face turn to it, the same move
//! as the turn of one layer but not to the pieces around it. The rider is
//! the state of those pieces.
//!
//! The two jobs share few moves, so the larger of the two bounds is far
//! below the true length and iterative deepening takes minutes. These
//! phases are searched by beam instead: states are ranked by the sum of
//! what the edges and the rider still need.

use super::*;
use crate::search::{beam_many, BeamFound, Problem};

pub trait Rider: Sync {
    type State: Copy + Ord + std::hash::Hash + Send + Sync;
    /// The rider after move `m` of the 7x7x7, made in frame `ctx`.
    fn step(&self, ctx: usize, s: &Self::State, m: u8) -> Self::State;
    /// The moves the rider needs at least; 0 only at its goal.
    fn bound(&self, ctx: usize, s: &Self::State) -> usize;
    /// How far the rider is from its goal, in quarters of a move, for
    /// ranking states. It may be more than the bound, and it may look ahead
    /// to what the phases after this one will ask of the rider.
    fn rank(&self, ctx: usize, s: &Self::State) -> u32 {
        4 * self.bound(ctx, s) as u32
    }
}

/// The move of the inner 5x5x5 that a move of the 7x7x7 is.
#[inline(always)]
pub fn inner_move(m: u8) -> u8 {
    if m < 18 {
        m
    } else {
        m - 18
    }
}

/// How one of these searches goes.
#[derive(Clone, Copy, Debug)]
pub struct Beam {
    /// States kept per level.
    pub width: usize,
    /// Solutions wanted, and levels searched past the first one that has any.
    pub cap: usize,
    pub more: usize,
    /// Weights of the edges' bound and of the rider's bound in the ranking,
    /// and the penalty, in moves, for a state with one of them at 0 alone.
    pub mix: [u32; 2],
    pub stuck: u32,
    /// See `beam_many`.
    pub salt: u32,
}

/// How a state is ranked: what the edges need (with the moves its start was
/// behind), what the rider needs, and a penalty where one of the two is
/// done and the other is not. Such a state looks nearly solved, but the
/// moves that finish one job undo the other; without the penalty states of
/// this kind fill the beam and nothing in it can be finished.
#[inline(always)]
fn rank(b: &Beam, edges: usize, late: u8, rider_bound: usize, rider_rank: u32) -> u32 {
    let stuck = (edges == 0) != (rider_bound == 0);
    4 * b.mix[0] * (edges as u32 + late as u32) + b.mix[1] * rider_rank + 4 * b.stuck * stuck as u32
}

/// The 7x7x7 moves that are the given moves of the 5x5x5: a face turn with
/// one layer, a face half turn also with two, a wide turn with three. With
/// each, its index in `inner`.
fn joint_moves(inner: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let (mut moves, mut index) = (Vec::new(), Vec::new());
    for (k, &m) in inner.iter().enumerate() {
        moves.push(if m < 18 { m } else { m + 18 });
        index.push(k as u8);
        if m < 18 && m % 3 == 1 {
            moves.push(m + 18);
            index.push(k as u8);
        }
    }
    (moves, index)
}

/// What the three phases have in common as beam searches.
struct Joint<'a, R: Rider> {
    e5: &'a Edges5,
    rider: &'a R,
    moves: Vec<u8>,
    index: Vec<u8>,
}

impl<'a, R: Rider> Joint<'a, R> {
    fn new(e5: &'a Edges5, rider: &'a R, inner: &[u8]) -> Joint<'a, R> {
        let (moves, index) = joint_moves(inner);
        Joint { e5, rider, moves, index }
    }
}

/// Frame, moves already spent beyond the cheapest start, and rider state.
type Ride<S> = (u8, u8, S);

struct JointC<'a, R: Rider>(Joint<'a, R>);
/// Phase C: the state of `CNode`.
type NodeC<S> = (Ride<S>, u32, u16, u32, u32);

impl<R: Rider> JointC<'_, R> {
    #[inline(always)]
    fn edges(&self, n: &NodeC<R::State>) -> usize {
        self.0.e5.c_bound(&CNode { mask: n.1, flip: n.2 as usize, x: n.3, t: n.4 })
    }
}

impl<R: Rider> Problem for JointC<'_, R> {
    type Node = NodeC<R::State>;

    fn moves(&self) -> &[u8] {
        &self.0.moves
    }

    #[inline(always)]
    fn next(&self, n: &Self::Node, k: usize) -> Self::Node {
        let (e, m7) = (self.0.e5, self.0.moves[k]);
        let m = inner_move(m7);
        let ride = (n.0 .0, n.0 .1, self.0.rider.step(n.0 .0 as usize, &n.0 .2, m7));
        (ride, e.bw.apply(m, n.1), e.t3.flip_mv[n.2 as usize][m as usize % 18], e.bx.apply(m, n.3), e.bt.apply(m, n.4))
    }

    #[inline(always)]
    fn bound(&self, n: &Self::Node) -> usize {
        self.edges(n).max(self.0.rider.bound(n.0 .0 as usize, &n.0 .2))
    }

    fn done(&self, n: &Self::Node) -> bool {
        n.1 == LOW_HALF && n.2 == 0
    }
}

struct JointP<'a, R: Rider>(Joint<'a, R>);
/// Phase P: the state of `PNode`.
type NodeP<S> = (Ride<S>, u32, u32, u16, u16);

impl<R: Rider> JointP<'_, R> {
    #[inline(always)]
    fn edges(&self, n: &NodeP<R::State>) -> usize {
        self.0.e5.p_bound(&PNode { ml: n.1 as usize, mh: n.2 as usize, fb: n.3 as usize, rl: n.4 as usize }, usize::MAX)
    }
}

impl<R: Rider> Problem for JointP<'_, R> {
    type Node = NodeP<R::State>;

    fn moves(&self) -> &[u8] {
        &self.0.moves
    }

    #[inline(always)]
    fn next(&self, n: &Self::Node, k: usize) -> Self::Node {
        let (e, i) = (self.0.e5, self.0.index[k] as usize);
        let ride = (n.0 .0, n.0 .1, self.0.rider.step(n.0 .0 as usize, &n.0 .2, self.0.moves[k]));
        (ride, e.ml_mv[n.1 as usize][i], e.mh_mv[n.2 as usize][i], e.fb_mv[n.3 as usize][i], e.rl_mv[n.4 as usize][i])
    }

    #[inline(always)]
    fn bound(&self, n: &Self::Node) -> usize {
        self.edges(n).max(self.0.rider.bound(n.0 .0 as usize, &n.0 .2))
    }

    /// Whether phase E accepts the cube is checked on the cube itself.
    fn done(&self, _: &Self::Node) -> bool {
        true
    }
}

struct JointE<'a, R: Rider>(Joint<'a, R>);
/// Phase E: the state of `ENode`.
type NodeE<S> = (Ride<S>, u16, u16, u8, u16, u8);

impl<R: Rider> JointE<'_, R> {
    #[inline(always)]
    fn edges(&self, n: &NodeE<R::State>) -> usize {
        let node = ENode { lo: n.1 as usize, hi: n.2 as usize, d: n.3, ud: n.4 as usize, side: n.5 as usize };
        self.0.e5.e_bound(&node)
    }
}

impl<R: Rider> Problem for JointE<'_, R> {
    type Node = NodeE<R::State>;

    fn moves(&self) -> &[u8] {
        &self.0.moves
    }

    #[inline(always)]
    fn next(&self, n: &Self::Node, k: usize) -> Self::Node {
        let (e, i) = (self.0.e5, self.0.index[k] as usize);
        let ride = (n.0 .0, n.0 .1, self.0.rider.step(n.0 .0 as usize, &n.0 .2, self.0.moves[k]));
        let (lo, hi) = (e.out_low_mv[n.1 as usize][i], e.out_high_mv[n.2 as usize][i]);
        // The neighbour is at distance d-1, d or d+1; the table says which, modulo 3.
        let v = e.e_mod3(lo as usize, hi as usize);
        let d = n.3 + (v + 3 - (n.3 + 2) % 3) % 3 - 1;
        (ride, lo, hi, d, e.ud_mv[n.4 as usize][i], e.last_mv[n.5 as usize][i])
    }

    #[inline(always)]
    fn bound(&self, n: &Self::Node) -> usize {
        self.edges(n).max(self.0.rider.bound(n.0 .0 as usize, &n.0 .2))
    }

    fn done(&self, _: &Self::Node) -> bool {
        true
    }
}

/// Moves each start has spent beyond the cheapest one, capped.
fn handicaps(starts: &[Start]) -> Vec<u8> {
    let least = starts.iter().map(|s| s.cost).min().unwrap_or(0);
    starts.iter().map(|s| (s.cost - least).min(200) as u8).collect()
}

fn found(out: Vec<BeamFound>, start_of: &[usize]) -> Vec<Found> {
    out.into_iter().map(|f| Found { start: start_of[f.root], moves: f.moves }).collect()
}

impl Edges5 {
    /// Phase C from many starts, each with the frame and state of its
    /// rider. The moves of the results are moves of the 7x7x7.
    pub fn phase_c_ridden<R: Rider>(
        &self,
        starts: &[Start],
        ride: &[(usize, R::State)],
        rider: &R,
        labellings: usize,
        b: &Beam,
    ) -> Vec<Found> {
        let p = JointC(Joint::new(self, rider, &H_MOVES));
        let late = handicaps(starts);
        let (mut roots, mut start_of) = (Vec::new(), Vec::new());
        for (i, s) in starts.iter().enumerate() {
            for (_, lab) in self.c_labellings(&s.cube, labellings) {
                let n = self.c_node(&s.cube, lab);
                roots.push(((ride[i].0 as u8, late[i], ride[i].1), n.mask, n.flip as u16, n.x, n.t));
                start_of.push(i);
            }
        }
        let score = |n: &NodeC<R::State>| {
            rank(b, p.edges(n), n.0 .1, rider.bound(n.0 .0 as usize, &n.0 .2), rider.rank(n.0 .0 as usize, &n.0 .2))
        };
        found(beam_many(&p, &roots, score, |_, _| true, b.width, b.cap, b.more, 40, b.salt), &start_of)
    }

    /// Phase P from many starts: every choice of four classes with no piece
    /// in the slice between F and B is a root of its own.
    pub fn phase_p_ridden<R: Rider>(&self, starts: &[Start], ride: &[(usize, R::State)], rider: &R, b: &Beam) -> Vec<Found> {
        let p = JointP(Joint::new(self, rider, &P_MOVES));
        let late = handicaps(starts);
        let (mut roots, mut start_of) = (Vec::new(), Vec::new());
        for (i, s) in starts.iter().enumerate() {
            let slots = slots_of(&s.cube);
            let free: Vec<u8> = (0..12u8)
                .filter(|&c| [slots.0, slots.1, slots.2].iter().all(|k| self.outer_index[k[c as usize] as usize] != 255))
                .collect();
            for a in 0..free.len() {
                for b in a + 1..free.len() {
                    for c in b + 1..free.len() {
                        for d in c + 1..free.len() {
                            let n = self.p_node(&s.cube, &[free[a], free[b], free[c], free[d]], &slots);
                            if self.p_bound(&n, usize::MAX) != FAR as usize {
                                roots.push(((ride[i].0 as u8, late[i], ride[i].1), n.ml as u32, n.mh as u32, n.fb as u16, n.rl as u16));
                                start_of.push(i);
                            }
                        }
                    }
                }
            }
        }
        let score = |n: &NodeP<R::State>| {
            rank(b, p.edges(n), n.0 .1, rider.bound(n.0 .0 as usize, &n.0 .2), rider.rank(n.0 .0 as usize, &n.0 .2))
        };
        // Four whole edges are in the middle slice; phase E must also accept the rest.
        let accept = |root: usize, moves: &[u8]| {
            let mut c = starts[start_of[root]].cube;
            for &m in moves {
                c.apply(inner_move(m));
            }
            self.e_node(&c).is_some()
        };
        found(beam_many(&p, &roots, score, accept, b.width, b.cap, b.more, 40, b.salt), &start_of)
    }

    /// Phase E from many starts; the results are reduced cubes.
    pub fn phase_e_ridden<R: Rider>(&self, starts: &[Start], ride: &[(usize, R::State)], rider: &R, b: &Beam) -> Vec<Found> {
        let p = JointE(Joint::new(self, rider, &E_MOVES[..14]));
        let late = handicaps(starts);
        let (mut roots, mut start_of) = (Vec::new(), Vec::new());
        for (i, s) in starts.iter().enumerate() {
            if let Some(n) = self.e_node(&s.cube) {
                roots.push(((ride[i].0 as u8, late[i], ride[i].1), n.lo as u16, n.hi as u16, n.d, n.ud as u16, n.side as u8));
                start_of.push(i);
            }
        }
        let score = |n: &NodeE<R::State>| {
            rank(b, p.edges(n), n.0 .1, rider.bound(n.0 .0 as usize, &n.0 .2), rider.rank(n.0 .0 as usize, &n.0 .2))
        };
        if let Some(n) = std::env::var("CUBE7_E_IDA").ok().and_then(|s| s.parse::<usize>().ok()) {
            // Development aid: exact lengths from the first few roots, to compare with the beam.
            let mut order: Vec<usize> = (0..roots.len()).collect();
            order.sort_by_key(|&i| roots[i].0 .1 as usize + p.bound(&roots[i]));
            for &i in order.iter().take(n) {
                let t = std::time::Instant::now();
                let sols = crate::search::ida(&p, &roots[i], 0, 1, 2_000_000_000);
                let one = beam_many(&p, &roots[i..i + 1], score, |_, _| true, b.width, 1, 0, 40, b.salt);
                eprintln!(
                    "      root late {} bound {} (edges {}): exact {:?} in {:.1} s, beam {:?}",
                    roots[i].0 .1,
                    p.bound(&roots[i]),
                    p.edges(&roots[i]),
                    sols.first().map(|s| s.len()),
                    t.elapsed().as_secs_f64(),
                    one.first().map(|s| s.moves.len())
                );
            }
        }
        found(beam_many(&p, &roots, score, |_, _| true, b.width, b.cap, b.more, 40, b.salt), &start_of)
    }
}
