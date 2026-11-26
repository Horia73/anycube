//! MultiCuber's planner: the quickest commands, under the time model, that
//! make a solution's moves or show the six faces to the camera, searched
//! over the robot's states (lift position, flipper side, which face is
//! where).
//!
//! Every step of the search is a command that `planner::allowed` lets
//! through in the state it is sent in: the cases in which the 2018 planner
//! sends it (lift between levels, to the scanning height only from 0 and
//! back; the flipper tipping the cube with the lift at 0 and moving alone
//! when the cube is off it; the rotator turning the whole cube at level n
//! and k layers at level k). The commands come out of the builder's
//! primitives, so the bookkeeping of frames is the one held against
//! robot.py, and the tests run every plan on the sticker model.
//!
//! A `Space` is made once per time model: the quickest way between every
//! two states (a few hundred of them), by Dijkstra from each lift and
//! flipper position (the way from one orientation to another is the way
//! from the first orientation of all to their difference). A solution's
//! time is then a short dynamic programme over its moves (before each move
//! its face must be at the top or the bottom, which sixteen states offer),
//! a few microseconds a solution: that is how thousands of candidate
//! solutions are compared by their true robot time in a few milliseconds.
//!
//! A solution ends with the lift down (the cube resting in the flipper,
//! whichever side that is on). The scan is a search of its own over the
//! states and the faces already seen: a picture is taken with the lift
//! down and the flipper DOWN, of the face in front of the camera, and
//! (unless told otherwise) right after the flipper tipped the cube there,
//! which is how every picture has been taken since 2018: the flipper's
//! walls square the cube up.

use super::planner::{allowed, renamed, turn_perm, Cmd, Lift, Op, Planner, RobotState, CAMERA, PERM_X, PERM_Y};
use super::timing::TimeModel;
use crate::robot::physical::PhysicalCube;
use crate::robot::Turn;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const U: u8 = 0;
const D: u8 = 3;
const FAR: u32 = u32::MAX / 4;
const NONE: u16 = u16::MAX;
const ORIENTATIONS: usize = 24;
const TURNS: [i8; 3] = [1, -1, 2];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    Lift(Lift),
    Flip(bool),
    /// The whole cube at level n.
    Whole(i8),
}

fn turn_index(turns: i8) -> usize {
    match turns {
        1 => 0,
        -1 => 1,
        _ => 2,
    }
}

/// Cost as an integer (0.01 ms) so that the search is the same on every machine.
fn cost_of(ms: f64) -> u32 {
    (ms * 100.0).round().max(0.0) as u32
}

/// A state before a move's rotator turn, and the state after it.
#[derive(Clone, Copy)]
struct Target {
    pre: u16,
    post_state: u16,
    /// `pre` and `post` taken apart for the inner loop of `cost`.
    pre_column: u32,
    pre_o: u8,
    post: Place,
}

/// A state as `cost` keeps it: the start of its row of `dist`, its orientation.
#[derive(Clone, Copy, Default)]
struct Place {
    row: u32,
    o: u8,
}

pub struct Space {
    n: u8,
    /// States: (lift position, flipper, orientation).
    count: usize,
    /// Orientation -> where each face is (the solver's face name -> robot position).
    frames: Vec<[u8; 6]>,
    /// (where U is, where F is) -> orientation.
    index: [[u8; 6]; 6],
    /// `rel[a][b]`: the orientation reached from the first one by the turns that lead from a to b.
    rel: Vec<[u8; ORIENTATIONS]>,
    /// `rename[axis][turn][o]`: the orientation after the solver's whole-cube turn.
    rename: [[[u8; ORIENTATIONS]; 3]; 3],
    edges: Vec<Vec<(Act, u16, u32)>>,
    /// The quickest ways from the states of the first orientation (one row for
    /// each lift and flipper position) to every state, `dist[row * count + t]`.
    /// The way from orientation a to b is the way from the first to `rel[a][b]`:
    /// kept so, the table is some ten kilobytes and stays in the nearest cache.
    dist: Vec<u32>,
    /// From the states of the first orientation: the state before each state on the quickest way.
    via: Vec<Vec<u16>>,
    /// Per (face, layers - 1, turn): the states the move can be made from.
    targets: Vec<Vec<Target>>,
    rot: [u32; 2],
}

impl Space {
    pub fn new(n: usize, t: &TimeModel) -> Space {
        let n8 = n as u8;
        let lifts = n + 2;
        let count = lifts * 2 * ORIENTATIONS;
        // The 24 orientations, from the first by the flipper's and the rotator's turns.
        let mut frames: Vec<[u8; 6]> = vec![[0, 1, 2, 3, 4, 5]];
        let mut at = 0;
        while at < frames.len() {
            for perm in [&PERM_X, &PERM_Y] {
                let next = turned(frames[at], perm, 1);
                if !frames.contains(&next) {
                    frames.push(next);
                }
            }
            at += 1;
        }
        assert_eq!(frames.len(), ORIENTATIONS);
        let mut index = [[u8::MAX; 6]; 6];
        for (o, f) in frames.iter().enumerate() {
            index[f[0] as usize][f[2] as usize] = o as u8;
        }
        let find = |f: &[u8; 6]| index[f[0] as usize][f[2] as usize];
        let rel: Vec<[u8; ORIENTATIONS]> = frames
            .iter()
            .map(|a| {
                let mut inv = [0u8; 6];
                for (x, &y) in a.iter().enumerate() {
                    inv[y as usize] = x as u8;
                }
                std::array::from_fn(|b| find(&std::array::from_fn(|x| frames[b][inv[x] as usize])))
            })
            .collect();
        let rename = std::array::from_fn(|axis| std::array::from_fn(|ti| std::array::from_fn(|o| find(&renamed(frames[o], axis as u8, TURNS[ti])))));
        let mut sp = Space { n: n8, count, frames, index, rel, rename, edges: Vec::new(), dist: Vec::new(), via: Vec::new(), targets: Vec::new(), rot: [0; 2] };
        sp.rot = [cost_of(t.rot_turn_ms(false) + t.overhead_ms), cost_of(t.rot_turn_ms(true) + t.overhead_ms)];
        // The steps: every command allowed in each state, but for the layer turns (the moves).
        sp.edges = (0..count)
            .map(|s| {
                let (lift, flip_up, o) = sp.parts(s as u16);
                let frame = sp.frames[o];
                let mut out = Vec::new();
                for to in (0..=n8).map(Lift::Level).chain([Lift::Scan]) {
                    if allowed(n8, lift, flip_up, Cmd::Lift(to)) {
                        out.push((Act::Lift(to), sp.state(to, flip_up, o), cost_of(t.lift_pair_ms(lift, to) + t.overhead_ms)));
                    }
                }
                let up = !flip_up;
                if allowed(n8, lift, flip_up, Cmd::Flip { up, normal: false }) {
                    let carry = lift == Lift::Level(0);
                    let o2 = if carry { sp.find(&turned(frame, &PERM_X, if up { 1 } else { -1 })) } else { o };
                    out.push((Act::Flip(up), sp.state(lift, up, o2), cost_of(t.flip_move_ms(carry) + t.overhead_ms)));
                }
                if lift == Lift::Level(n8) {
                    for turns in TURNS {
                        if allowed(n8, lift, flip_up, Cmd::Rot(turns)) {
                            out.push((Act::Whole(turns), sp.state(lift, flip_up, sp.find(&turned(frame, &PERM_Y, turns))), sp.rot[(turns == 2) as usize]));
                        }
                    }
                }
                out
            })
            .collect();
        // The quickest ways from the states of the first orientation: by `rel`, from every state.
        for row in 0..lifts * 2 {
            let (d, via) = sp.dijkstra((row * ORIENTATIONS) as u16);
            sp.dist.extend(d);
            sp.via.push(via);
        }
        // Where each move can be made: its face at the top (lift at its layers) or at the bottom.
        for face in 0..6u8 {
            for depth in 1..n8.max(2) {
                for turns in TURNS {
                    let mut list = Vec::with_capacity(16);
                    for o in 0..ORIENTATIONS {
                        let pos = sp.frames[o][face as usize];
                        if pos != U && pos != D {
                            continue;
                        }
                        let level = if pos == U { depth } else { n8 - depth };
                        let o2 = if pos == D || level == n8 { sp.find(&turned(sp.frames[o], &PERM_Y, turns)) } else { o };
                        for flip_up in [false, true] {
                            if allowed(n8, Lift::Level(level), flip_up, Cmd::Rot(turns)) {
                                let (pre, post) = (sp.state(Lift::Level(level), flip_up, o), sp.state(Lift::Level(level), flip_up, o2));
                                list.push(Target { pre, post_state: post, pre_column: (pre as usize / ORIENTATIONS * ORIENTATIONS) as u32, pre_o: o as u8, post: sp.place(post) });
                            }
                        }
                    }
                    sp.targets.push(list);
                }
            }
        }
        sp
    }

    fn find(&self, frame: &[u8; 6]) -> usize {
        self.index[frame[0] as usize][frame[2] as usize] as usize
    }

    fn lift_index(&self, l: Lift) -> usize {
        match l {
            Lift::Level(k) => k as usize,
            Lift::Scan => self.n as usize + 1,
        }
    }

    fn state(&self, lift: Lift, flip_up: bool, o: usize) -> u16 {
        ((self.lift_index(lift) * 2 + flip_up as usize) * ORIENTATIONS + o) as u16
    }

    fn parts(&self, s: u16) -> (Lift, bool, usize) {
        let s = s as usize;
        let l = s / (2 * ORIENTATIONS);
        (if l == self.n as usize + 1 { Lift::Scan } else { Lift::Level(l as u8) }, s / ORIENTATIONS % 2 == 1, s % ORIENTATIONS)
    }

    fn place(&self, s: u16) -> Place {
        Place { row: (s as usize / ORIENTATIONS * self.count) as u32, o: (s as usize % ORIENTATIONS) as u8 }
    }

    fn start(&self, s: RobotState) -> u16 {
        self.state(s.lift, s.flip_up, 0)
    }

    fn dijkstra(&self, from: u16) -> (Vec<u32>, Vec<u16>) {
        let mut dist = vec![FAR; self.count];
        let mut via = vec![NONE; self.count];
        let mut heap = BinaryHeap::new();
        dist[from as usize] = 0;
        heap.push(Reverse((0u32, from)));
        while let Some(Reverse((c, s))) = heap.pop() {
            if c > dist[s as usize] {
                continue;
            }
            for &(_, to, ms) in &self.edges[s as usize] {
                let nc = c + ms;
                if nc < dist[to as usize] {
                    dist[to as usize] = nc;
                    via[to as usize] = s;
                    heap.push(Reverse((nc, to)));
                }
            }
        }
        (dist, via)
    }

    /// The time of the quickest way from s to t.
    #[inline]
    fn d(&self, s: u16, t: u16) -> u32 {
        let (s, t) = (s as usize, t as usize);
        let turn = self.rel[s % ORIENTATIONS][t % ORIENTATIONS] as usize;
        self.dist[s / ORIENTATIONS * self.count + t / ORIENTATIONS * ORIENTATIONS + turn]
    }

    /// The commands of the quickest way from s to t.
    fn way(&self, s: u16, t: u16) -> Vec<Act> {
        let (row, a) = (s as usize / ORIENTATIONS, s as usize % ORIENTATIONS);
        let (trow, b) = (t as usize / ORIENTATIONS, t as usize % ORIENTATIONS);
        let first = (row * ORIENTATIONS) as u16;
        let mut at = (trow * ORIENTATIONS + self.rel[a][b] as usize) as u16;
        let mut acts = Vec::new();
        while at != first {
            let before = self.via[row][at as usize];
            assert!(before != NONE, "every state is reached from every other");
            acts.push(self.edges[before as usize].iter().find(|e| e.1 == at).expect("the step exists").0);
            at = before;
        }
        acts.reverse();
        acts
    }

    fn targets(&self, face: u8, depth: u8, turns: i8) -> &[Target] {
        let per_face = (self.n.max(2) - 1) as usize * 3;
        &self.targets[face as usize * per_face + (depth as usize - 1) * 3 + turn_index(turns)]
    }

    /// With the lift down, flipper and cube as they are.
    fn down(&self, s: u16) -> u16 {
        let (_, flip_up, o) = self.parts(s);
        self.state(Lift::Level(0), flip_up, o)
    }

    /// The robot time (ms) of the quickest commands for `moves` from `start`,
    /// the lift brought down at the end.
    pub fn cost(&self, start: RobotState, moves: &[Turn]) -> f64 {
        let mut now = [(Place::default(), 0u32); 16];
        let mut next = now;
        now[0] = (self.place(self.start(start)), 0);
        let mut len = 1;
        for &mv in moves {
            match mv {
                Turn::Whole { axis, turns } => {
                    for e in &mut now[..len] {
                        e.0.o = self.rename[axis as usize][turn_index(turns)][e.0.o as usize];
                    }
                }
                Turn::Layers { face, depth, turns } => {
                    let targets = self.targets(face, depth, turns);
                    let rot = self.rot[(turns == 2) as usize];
                    for (j, t) in targets.iter().enumerate() {
                        let mut best = FAR;
                        for &(s, c) in &now[..len] {
                            let turn = self.rel[s.o as usize][t.pre_o as usize];
                            best = best.min(c + self.dist[(s.row + t.pre_column) as usize + turn as usize]);
                        }
                        next[j] = (t.post, best + rot);
                    }
                    len = targets.len();
                    std::mem::swap(&mut now, &mut next);
                }
            }
        }
        // The lift down at the end: the same flipper side and orientation, level 0.
        let down = |s: Place| self.dist[s.row as usize + (s.row as usize / self.count % 2) * ORIENTATIONS];
        let best = now[..len].iter().map(|&(s, c)| c + down(s)).min().unwrap_or(0);
        best as f64 / 100.0
    }

    /// Appends to `p` (from its state) the quickest commands for `moves`,
    /// then the lift down.
    pub fn solution(&self, p: &mut Planner, moves: &[Turn]) {
        p.start_run();
        // Forward, as `cost`, keeping for each state where it came from.
        let mut layers: Vec<Vec<(u16, u32, usize)>> = vec![vec![(self.start(p.state()), 0, 0)]];
        for &mv in moves {
            let now = layers.last().unwrap();
            let next: Vec<(u16, u32, usize)> = match mv {
                Turn::Whole { axis, turns } => now
                    .iter()
                    .enumerate()
                    .map(|(i, &(s, c, _))| {
                        let o = s as usize % ORIENTATIONS;
                        (s - o as u16 + self.rename[axis as usize][turn_index(turns)][o] as u16, c, i)
                    })
                    .collect(),
                Turn::Layers { face, depth, turns } => self
                    .targets(face, depth, turns)
                    .iter()
                    .map(|t| {
                        let (i, c) = now.iter().enumerate().map(|(i, &(s, c, _))| (i, c + self.d(s, t.pre))).min_by_key(|&(i, c)| (c, i)).unwrap();
                        (t.post_state, c + self.rot[(turns == 2) as usize], i)
                    })
                    .collect(),
            };
            layers.push(next);
        }
        let last = layers.last().unwrap();
        let mut at = (0..last.len()).min_by_key(|&i| (last[i].1 + self.d(last[i].0, self.down(last[i].0)), i)).unwrap();
        // Back: which state of each layer the chosen way goes through.
        let mut chosen = vec![0usize; layers.len()];
        for k in (0..layers.len()).rev() {
            chosen[k] = at;
            at = layers[k][at].2;
        }
        for (k, &mv) in moves.iter().enumerate() {
            let from = layers[k][chosen[k]].0;
            match mv {
                Turn::Whole { axis, turns } => p.rename(axis, turns),
                Turn::Layers { face, depth, turns } => {
                    let target = self.targets(face, depth, turns)[chosen[k + 1]];
                    self.emit(p, from, target.pre);
                    let pos = p.frame[face as usize];
                    let level = if pos == U { depth } else { p.n - depth };
                    assert!((pos == U || pos == D) && p.lift == Lift::Level(level), "the move's face is at the rotator");
                    p.rot(turns, pos == D || level == p.n, Some(k as u16));
                    debug_assert_eq!(self.state(p.lift, p.flip_up, self.find(&p.frame)), target.post_state);
                }
            }
        }
        let end = layers.last().unwrap()[*chosen.last().unwrap()].0;
        self.emit(p, end, self.down(end));
    }

    /// The commands of the quickest way from s to t, on the builder.
    fn emit(&self, p: &mut Planner, s: u16, t: u16) {
        debug_assert_eq!(self.state(p.lift, p.flip_up, self.find(&p.frame)), s);
        for act in self.way(s, t) {
            self.step(p, act);
        }
        debug_assert_eq!(self.state(p.lift, p.flip_up, self.find(&p.frame)), t);
    }

    fn step(&self, p: &mut Planner, act: Act) {
        match act {
            Act::Lift(l) => p.lift_to(l),
            Act::Flip(up) => p.flip(up, false),
            Act::Whole(turns) => p.rot(turns, true, None),
        }
    }

    /// The quickest commands to where the robot can be switched off: lift down, flipper up.
    pub fn park(&self, p: &mut Planner) {
        p.start_run();
        let s = self.start(p.state());
        let t = (0..ORIENTATIONS).map(|o| self.state(Lift::Level(0), true, o)).min_by_key(|&t| (self.d(s, t), t)).unwrap();
        self.emit(p, s, t);
    }

    /// The quickest commands that show the six faces to the camera, a
    /// picture after each (their faces and turns are named afterwards, by
    /// `name_photos`). `seated`: a picture only right after the flipper
    /// tipped the cube in front of the camera.
    pub fn scan(&self, p: &mut Planner, seated: bool) {
        p.start_run();
        let at_camera: Vec<u8> = self.frames.iter().map(|f| f.iter().position(|&pos| pos == CAMERA).unwrap() as u8).collect();
        // A node: a state and the faces seen. How it was reached: a step (0), a picture (1), FLIP DOWN and a picture (2).
        let nodes = self.count * 64;
        let mut dist = vec![FAR; nodes];
        let mut via: Vec<(u32, u8)> = vec![(u32::MAX, 0); nodes];
        let mut heap = BinaryHeap::new();
        let first = self.start(p.state()) as usize * 64;
        dist[first] = 0;
        heap.push(Reverse((0u32, first as u32)));
        let mut end = None;
        while let Some(Reverse((c, node))) = heap.pop() {
            let node = node as usize;
            if c > dist[node] {
                continue;
            }
            let (s, seen) = ((node / 64) as u16, node % 64);
            if seen == 63 {
                end = Some(node);
                break;
            }
            let mut offer = |to: usize, nc: u32, how: u8, heap: &mut BinaryHeap<Reverse<(u32, u32)>>| {
                if nc < dist[to] {
                    dist[to] = nc;
                    via[to] = (node as u32, how);
                    heap.push(Reverse((nc, to as u32)));
                }
            };
            let (lift, flip_up, o) = self.parts(s);
            for &(act, to, ms) in &self.edges[s as usize] {
                offer(to as usize * 64 + seen, c + ms, 0, &mut heap);
                if seated && lift == Lift::Level(0) && act == Act::Flip(false) {
                    let bit = 1usize << at_camera[to as usize % ORIENTATIONS];
                    if seen & bit == 0 {
                        offer(to as usize * 64 + (seen | bit), c + ms, 2, &mut heap);
                    }
                }
            }
            if !seated && lift == Lift::Level(0) && !flip_up {
                let bit = 1usize << at_camera[o];
                if seen & bit == 0 {
                    offer(s as usize * 64 + (seen | bit), c, 1, &mut heap);
                }
            }
        }
        let mut at = end.expect("the six faces can always be shown");
        let mut steps: Vec<(Option<Act>, bool)> = Vec::new();
        while at != first {
            let (before, how) = via[at];
            let (s, t) = ((before as usize / 64) as u16, (at / 64) as u16);
            let act = (how != 1).then(|| self.edges[s as usize].iter().find(|e| e.1 == t).expect("the step exists").0);
            steps.push((act, how != 0));
            at = before as usize;
        }
        steps.reverse();
        let mut index = 0;
        for (act, photo) in steps {
            if let Some(act) = act {
                self.step(p, act);
            }
            if photo {
                p.photo(index, '?', 0);
                index += 1;
            }
        }
    }
}

fn turned(frame: [u8; 6], perm: &[u8; 6], turns: i8) -> [u8; 6] {
    let p = turn_perm(perm, turns);
    std::array::from_fn(|f| p[frame[f] as usize])
}

/// Names the pictures of a scan: each after the place its face has when the
/// scan has ended (the frame the facelets are then given in), with the
/// clockwise turn that brings the camera's picture upright for that face,
/// as anycube reads a face (/api/check's order of facelets). Found by
/// following every sticker through the commands on the sticker model: the
/// camera looks at the front of the cube as it lies in the flipper, the
/// right way up.
///
/// For the 2018 scan this gives the 2018 names, and the 2018 turns for R,
/// L, B and F; for U and D it gives 90 and 270 where robot.py has 270 and
/// 90. robot.py's two are half a turn off anycube's reading: MultiCuber's
/// FAST_SCAN.md records that its real photos make a cube only with U and D
/// turned by 180 degrees (`--rotations '{"U":2,"D":2}'`), which is exactly
/// what the sticker model says (`tests/robot_plans.rs`).
pub fn name_photos(n: usize, start: RobotState, ops: &mut [Op]) {
    assert!(6 * n * n <= 256, "stickers are followed as bytes");
    let nn = n * n;
    let mut cube = PhysicalCube::new(n);
    cube.facelets = (0..6 * nn).map(|i| i as u8).collect();
    cube.lift = start.lift;
    cube.flip_up = start.flip_up;
    let camera = CAMERA as usize;
    let mut seen: Vec<(usize, Vec<u8>)> = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        cube.apply_line(&op.line());
        if op.photo.is_some() {
            seen.push((i, cube.facelets[camera * nn..(camera + 1) * nn].to_vec()));
        }
    }
    // A face's stickers after a quarter turn clockwise, row by row.
    let quarter = |a: &[u8]| -> Vec<u8> { (0..nn).map(|i| a[(n - 1 - i % n) * n + i / n]).collect() };
    for (i, mut shown) in seen {
        let face = (0..6).find(|&q| cube.facelets[q * nn..(q + 1) * nn].contains(&shown[0])).expect("the sticker is on a face");
        let end = &cube.facelets[face * nn..(face + 1) * nn];
        let mut quarters: u16 = 0;
        while shown != end {
            shown = quarter(&shown);
            quarters += 1;
            assert!(quarters < 4, "a face stays a face");
        }
        let photo = ops[i].photo.as_mut().unwrap();
        photo.face = crate::robot::FACES[face] as char;
        photo.rotate = 90 * quarters;
    }
}
