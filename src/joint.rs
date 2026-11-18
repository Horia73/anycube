//! Phases 1 and 2 as one search: from any state, stage all centres, bring
//! the R/L centres into a finishable pattern, make the wing permutation even
//! and the dedges pairable, using every move.
//!
//! The exact centre table makes the centre part of the search perfect: only
//! sequences that really stage the centres in the given number of moves are
//! followed. The other conditions are tested at the end.

use crate::ctab::NEAR;
use crate::cube::{skip_move, tables as cube_tables, Cube};
use crate::solver::{P2Cand, Params, Solver, Variant};
use crate::tables::*;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;

#[derive(Clone, Copy)]
struct Node {
    /// Positions of the R/L-, U/D- and R-coloured centres.
    rl: u32,
    ud: u32,
    r: u32,
    /// Parity of the wing permutation.
    parity: u8,
    /// Exact number of moves to stage the centres.
    d: u8,
    /// Moves that bring the centres one step closer, and moves that keep the
    /// distance, when known.
    closer: u32,
    same: u32,
    /// Positions of the centres the two partner variants call R-coloured.
    pr: [u32; 2],
}

const UNKNOWN: u32 = u32::MAX;

/// The centres one move on, and where the exact table has them (`Solver::joint_ahead`).
#[derive(Clone, Copy)]
struct Ahead {
    rl: u32,
    ud: u32,
    at: usize,
}

struct Shared<'a> {
    /// The searches of one length are jobs with numbers: those from this
    /// number on need not go on (see `joint_endpoints`).
    stop_at: AtomicUsize,
    /// With a clock (`--time`): past this moment the search ends as soon as
    /// it has an endpoint, wherever it has got to.
    deadline: Option<std::time::Instant>,
    have_endpoint: &'a AtomicBool,
    timed_out: AtomicBool,
    /// Statistics for the debug output.
    staged: AtomicUsize,
    tried: AtomicUsize,
    by_rem: [AtomicUsize; 9],
}

/// The jobs of one length that are done, and what they add up to in
/// their order: positions visited and endpoints found by all the jobs
/// before `at`, which are the ones that count.
struct Front {
    done: Vec<Option<(u64, usize)>>,
    at: usize,
    spent: u64,
    found: usize,
    decided: bool,
}

/// A variant in another frame whose centres are staged by the same sequences
/// (relabelled), so one search serves all three frames.
struct Partner {
    variant: usize,
    /// Where this variant's R-coloured centres are in the searched variant.
    r: u32,
    /// Its R and L faces are this byte of the searched variant's masks ...
    shift: u32,
    /// ... and these patterns on them are the ones phase 3 can finish.
    ok: [bool; 256],
    /// Move names in this variant's frame.
    relabel: [u8; 36],
}

/// What the last move of a sequence does to the wings, per frame.
struct LastMove {
    /// Per frame and wing position, the moves sending it to the low half.
    to_low: [[u32; 24]; 3],
    /// Per move, destination of each wing position.
    dest: [[u8; 24]; 27],
    followers: [u32; 28],
    /// Moves with a live frame that leave the wing permutation even.
    terminal: [u32; 2],
    /// Per frame, moves outside phase 3.
    live: [u32; 3],
    frames: usize,
}

struct Ctx<'a> {
    solver: &'a Solver,
    shared: &'a Shared<'a>,
    variants: &'a [Variant],
    partners: &'a [Partner],
    last_move: &'a LastMove,
    variant: usize,
    /// Its number among the searches of this length.
    job: usize,
    path: [u8; 32],
    out: Vec<P2Cand>,
    nodes: u64,
    /// Frames in which the sequence that just ended can be an endpoint (bit mask).
    frames_ok: u8,
    by_rem: [usize; 9],
    /// Where the wings are after the first moves of the path, for as many
    /// of them as `wings_known`: worked out when a sequence is one move
    /// from its end, and kept for the sequences that begin the same way.
    wings: [[u8; 24]; 33],
    wings_known: usize,
}

const BATCH: u64 = 1 << 12;

impl Solver {
    fn joint_root(&self, cube: &Cube) -> Node {
        let m = axis_masks(cube);
        let mut r = 0u32;
        for i in 0..24 {
            if cube.ct[i] == 1 {
                r |= 1 << i;
            }
        }
        let d = self.ctab.exact(&self.tabs.bp, &self.tabs.ranks, m[2], m[0]);
        Node { rl: m[2], ud: m[0], r, parity: cube.wing_parity(), d, closer: UNKNOWN, same: UNKNOWN, pr: [0; 2] }
    }

    /// The partners of searched variant `i` in the other two frames.
    fn joint_partners(&self, variants: &[Variant], i: usize) -> Vec<Partner> {
        let ct = cube_tables();
        const GOALS: [u32; 3] = [0x0000FF, 0x00FF00, 0xFF0000];
        let mut out = Vec::new();
        for f in 1..3 {
            let frame = variants[f * 6].frame;
            let rot = ct.rots[frame].seq.clone();
            // The searched variant seen from the other frame: same colours, rotated positions.
            let mut seen = variants[i].cube;
            seen.apply_seq(&rot);
            let seen_masks = axis_masks(&seen);
            // Where the rotation takes each face pair.
            let mut turned = Cube::solved();
            turned.apply_seq(&rot);
            let turned_masks = axis_masks(&turned);
            let to_axis = |a: usize| GOALS.iter().position(|&g| g == turned_masks[a]).unwrap();
            let j = (f * 6..f * 6 + 6)
                .find(|&j| {
                    let m = axis_masks(&variants[j].cube);
                    (0..3).all(|a| m[to_axis(a)] == seen_masks[a])
                })
                .expect("every variant has a partner in each frame");
            // The colour of the searched variant that the partner calls R.
            let p = (0..24).find(|&p| variants[j].cube.ct[p] == 1).unwrap();
            let colour = seen.ct[p];
            let mut r = 0u32;
            for q in 0..24 {
                if variants[i].cube.ct[q] == colour {
                    r |= 1 << q;
                }
            }
            // The face pair of the searched variant that the rotation takes to R/L.
            let bp = &self.tabs.bp;
            let turn = |mask: u32| rot.iter().fold(mask, |x, &m| bp.apply(m, x));
            let shift = [0u32, 8, 16].into_iter().find(|&s| turn(0xFF << s) == 0xFF_0000).unwrap();
            let mut ok = [false; 256];
            for (b, o) in ok.iter_mut().enumerate() {
                *o = self.rl_ok[(turn((b as u32) << shift) >> 16) as usize];
            }
            out.push(Partner { variant: j, r, shift, ok, relabel: ct.relabel[ct.rot_inv[frame]] });
        }
        out
    }

    /// The node after move `m`, if the centres can still be staged in `rem` more moves.
    #[inline(always)]
    fn joint_step(&self, n: &Node, m: u8, rem: usize) -> Option<Node> {
        let ahead = self.joint_ahead(n, m, rem)?;
        self.joint_arrive(n, m, rem, &ahead)
    }

    /// The first half of a step: where the centres are after move `m`, and
    /// the entry that says how far that is asked for ahead (`search::fetch`).
    /// Both tables are read at random and the exact one is a hundred times
    /// the cache: a search that asks for the entries of all its next moves
    /// before it reads the first waits for them together, not one after
    /// the other. `None` if the move cannot be part of an endpoint.
    #[inline(always)]
    fn joint_ahead(&self, n: &Node, m: u8, rem: usize) -> Option<Ahead> {
        // A sequence that leaves the wing permutation odd is no endpoint in any frame.
        if rem == 0 && n.parity ^ p2_flips_parity(m) as u8 != 0 {
            return None;
        }
        let bp = &self.tabs.bp;
        let (rl, ud) = (bp.apply(m, n.rl), bp.apply(m, n.ud));
        let at = if rem <= NEAR {
            if !self.near.ahead(rl, ud) {
                return None;
            }
            0
        } else {
            let at = self.ctab.index(&self.tabs.ranks, rl, ud);
            self.ctab.fetch(at);
            at
        };
        Some(Ahead { rl, ud, at })
    }

    /// The second half: the node, if the centres can still be staged in `rem` more moves.
    #[inline(always)]
    fn joint_arrive(&self, n: &Node, m: u8, rem: usize, ahead: &Ahead) -> Option<Node> {
        let (rl, ud) = (ahead.rl, ahead.ud);
        let (d, closer, same) = if rem <= NEAR {
            // Close to the goal the small in-cache table answers exactly.
            match self.near.get(rl, ud) {
                Some((d, closer, same)) if d as usize <= rem => (d, closer, same),
                _ => return None,
            }
        } else {
            // The neighbour is at distance d-1, d or d+1; the table says which, modulo 3.
            let v = self.ctab.get(ahead.at);
            let d = n.d + (v + 3 - (n.d + 2) % 3) % 3 - 1;
            if d as usize > rem {
                return None;
            }
            (d, UNKNOWN, UNKNOWN)
        };
        let bp = &self.tabs.bp;
        Some(Node {
            rl,
            ud,
            r: bp.apply(m, n.r),
            parity: n.parity ^ p2_flips_parity(m) as u8,
            d,
            closer,
            same,
            pr: [bp.apply(m, n.pr[0]), bp.apply(m, n.pr[1])],
        })
    }

    /// Endpoints of the joint phase, by increasing length, until `p.n2` are found
    /// or the budget of positions is spent (and there is an endpoint).
    ///
    /// The same cube gives the same endpoints on every run and every
    /// machine. A length is searched as a few thousand jobs, one for every
    /// two first moves, in a mixed order; the jobs are started in that
    /// order and every one runs to its end. The search stops after the
    /// first job with which the jobs up to it, in their order, have spent
    /// the budget or found `p.n2` endpoints, and only the endpoints of the
    /// jobs up to that one are returned: a job that happened to be
    /// finished early further on does not count. (Stopping every thread
    /// when a counter they share runs out is simpler and returns what the
    /// threads had got to that time.)
    ///
    /// With a `deadline` the search also ends there if it has an endpoint,
    /// with whatever it has found: that result is not repeatable.
    pub(crate) fn joint_endpoints(
        &self,
        variants: &[Variant],
        p: &Params,
        deadline: Option<std::time::Instant>,
        revisit: Option<usize>,
    ) -> Result<Vec<P2Cand>, String> {
        // Variants come in groups of six per frame. Only the first frame is searched;
        // every staged sequence is then also tried in its partner of the other two frames.
        let share = std::env::var_os("ANYCUBE_NO_SHARE").is_none();
        let searched = if share { 6 } else { variants.len() };
        let partners: Vec<Vec<Partner>> = (0..searched)
            .map(|i| if share { self.joint_partners(variants, i) } else { Vec::new() })
            .collect();
        let roots: Vec<Node> = (0..searched)
            .map(|i| {
                let mut n = self.joint_root(&variants[i].cube);
                for (k, part) in partners[i].iter().enumerate() {
                    n.pr[k] = part.r;
                }
                n
            })
            .collect();
        let lower = roots.iter().map(|n| n.d as usize).min().unwrap();
        // For the test on the last move: which wings each move sends to the low half, per frame.
        let ct = cube_tables();
        let mut last_move = LastMove {
            to_low: [[0; 24]; 3], dest: [[0; 24]; 27], followers: [(1 << 27) - 1; 28], terminal: [0; 2],
            live: [0; 3], frames: if share { 3 } else { 1 },
        };
        for &last in &P1_MOVES {
            last_move.followers[last as usize] = P1_MOVES.iter().copied()
                .filter(|&m| !skip_move(last, m)).fold(0, |mask, m| mask | (1 << m));
        }
        let wing_positions: Vec<[u8; 24]> = variants.iter().take(searched).map(|v| {
            let mut pos = [0; 24];
            for (i, &piece) in v.cube.ep.iter().enumerate() {
                pos[piece as usize] = i as u8;
            }
            pos
        }).collect();
        for &m in &P1_MOVES {
            for (q, &p) in ct.ep_src[m as usize].iter().enumerate() {
                last_move.dest[m as usize][p as usize] = q as u8;
            }
        }
        for f in 0..last_move.frames {
            // Positions that end in the low half when the cube is turned into frame `f`.
            let mut at = [0u8; 24];
            for (i, a) in at.iter_mut().enumerate() {
                *a = i as u8;
            }
            let frame = variants[f * 6].frame;
            for &m in &ct.rots[frame].seq {
                let old = at;
                for i in 0..24 {
                    at[i] = old[ct.ep_src[m as usize][i] as usize];
                }
            }
            let low = at[..12].iter().fold(0u32, |a, &p| a | 1 << p);
            let relabel = &ct.relabel[ct.rot_inv[frame]];
            for &m in &P1_MOVES {
                let src = &ct.ep_src[m as usize];
                for q in 0..24 {
                    if (low >> q) & 1 == 1 {
                        last_move.to_low[f][src[q] as usize] |= 1 << m;
                    }
                }
                if !is_p3_move(relabel[m as usize]) {
                    last_move.live[f] |= 1 << m;
                }
            }
        }
        for &m in &P1_MOVES {
            if last_move.live.iter().any(|bits| bits >> m & 1 != 0) {
                last_move.terminal[p2_flips_parity(m) as usize] |= 1 << m;
            }
        }
        // Development aid: stop after a given depth.
        let max_depth: Option<usize> =
            std::env::var("ANYCUBE_JOINT_MAXDEPTH").ok().and_then(|s| s.parse().ok());
        let have_endpoint = AtomicBool::new(false);
        let shared = Shared {
            stop_at: AtomicUsize::new(usize::MAX),
            deadline,
            have_endpoint: &have_endpoint,
            timed_out: AtomicBool::new(false),
            staged: AtomicUsize::new(0),
            tried: AtomicUsize::new(0),
            by_rem: Default::default(),
        };
        let budget = p.joint_nodes.max(0) as u64;
        let mut found: Vec<P2Cand> = Vec::new();
        let mut spent = 0u64;
        for depth in revisit.unwrap_or(lower).max(lower)..=revisit.unwrap_or(24) {
            // One job for every two first moves that can still lead to staged centres.
            let mut jobs: Vec<(usize, Vec<u8>, Node)> = Vec::new();
            for (vi, root) in roots.iter().enumerate().filter(|(_, root)| root.d as usize <= depth) {
                let mut level: Vec<(Vec<u8>, Node)> = vec![(Vec::new(), *root)];
                for _ in 0..depth.min(2) {
                    let mut next = Vec::new();
                    for (path, n) in &level {
                        let last = path.last().copied().unwrap_or(255);
                        for &m in P1_MOVES.iter().filter(|&&m| last == 255 || !skip_move(last, m)) {
                            if let Some(to) = self.joint_step(n, m, depth - path.len() - 1) {
                                next.push((path.iter().copied().chain([m]).collect(), to));
                            }
                        }
                    }
                    level = next;
                }
                jobs.extend(level.into_iter().map(|(path, n)| (vi, path, n)));
            }
            let mut order = crate::search::mixed_order(jobs.len());
            if revisit.is_some() {
                // Start at the far end of the previous level. Among jobs
                // with the same remaining centre distance, prefer the
                // first turns which already put more wings opposite their
                // partners. Stable ties retain the spread of prefix moves.
                order.reverse();
                let quality: Vec<_> = jobs.iter().map(|(v,path,n)| {
                    let mut at = variants[*v].cube;
                    at.apply_seq(path);
                    let mut low = 0u32;
                    for &wing in &at.ep[..12] {low |= 1 << (wing % 12);}
                    (n.d, 12-low.count_ones())
                }).collect();
                order.sort_by_key(|&i| quality[i]);
            }
            shared.stop_at.store(usize::MAX, Relaxed);
            let next = AtomicUsize::new(0);
            let front = Mutex::new(Front { done: vec![None; jobs.len()], at: 0, spent, found: found.len(), decided: false });
            let mut parts: Vec<(usize, Vec<P2Cand>)> = (0..rayon::current_num_threads())
                .into_par_iter()
                .flat_map_iter(|_| {
                    let mut mine = Vec::new();
                    loop {
                        let job = next.fetch_add(1, Relaxed);
                        if job >= jobs.len() || job >= shared.stop_at.load(Relaxed) {
                            break;
                        }
                        let (vi, path, node) = &jobs[order[job]];
                        let mut ctx = Ctx {
                            solver: self,
                            shared: &shared,
                            variants,
                            partners: &partners[*vi],
                            last_move: &last_move,
                            variant: *vi,
                            job,
                            path: [0; 32],
                            out: Vec::new(),
                            nodes: 0,
                            frames_ok: 7,
                            by_rem: [0; 9],
                            wings: [wing_positions[*vi]; 33],
                            wings_known: 0,
                        };
                        ctx.path[..path.len()].copy_from_slice(path);
                        ctx.dfs(node, depth - path.len(), path.last().copied().unwrap_or(255), path.len());
                        for (a, &n) in shared.by_rem.iter().zip(ctx.by_rem.iter()) {
                            a.fetch_add(n, Relaxed);
                        }
                        // The jobs done so far, as far as they are all done in their order.
                        let mut front = front.lock().unwrap();
                        front.done[job] = Some((ctx.nodes, ctx.out.len()));
                        while !front.decided && front.at < jobs.len() {
                            let Some((nodes, ends)) = front.done[front.at] else { break };
                            front.spent += nodes;
                            front.found += ends;
                            front.at += 1;
                            if front.found >= p.n2 || (front.found > 0 && front.spent >= budget) {
                                front.decided = true;
                                shared.stop_at.fetch_min(front.at, Relaxed);
                            }
                        }
                        drop(front);
                        mine.push((job, ctx.out));
                    }
                    mine
                })
                .collect();
            parts.sort_by_key(|part| part.0);
            let front = front.into_inner().unwrap();
            let timed_out = shared.timed_out.load(Relaxed);
            // The jobs that count; past a deadline, whatever there is.
            let upto = if timed_out { usize::MAX } else if front.decided { front.at } else { jobs.len() };
            found.extend(parts.into_iter().filter(|part| part.0 < upto).flat_map(|part| part.1));
            spent = front.spent;
            if found.len() >= p.n2 || (!found.is_empty() && (spent >= budget || timed_out)) {
                break;
            }
            if max_depth.is_some_and(|m| depth >= m) {
                break;
            }
        }
        if std::env::var_os("ANYCUBE_DEBUG").is_some() {
            eprintln!(
                "debug: joint phase: nodes by moves left {:?}, {} staged sequences, {} edge tests, {} endpoints",
                shared.by_rem.iter().map(|a| a.load(Relaxed)).collect::<Vec<_>>(),
                shared.staged.load(Relaxed),
                shared.tried.load(Relaxed),
                found.len()
            );
        }
        Ok(found)
    }
}

impl Ctx<'_> {
    /// Whether this search need not go on (`Shared::stop_at`).
    #[inline(always)]
    fn over(&self) -> bool {
        self.job >= self.shared.stop_at.load(Relaxed)
    }

    fn dfs(&mut self, n: &Node, rem: usize, last: u8, depth: usize) {
        if rem == 0 {
            self.leaf(n, depth);
            return;
        }
        self.nodes += 1;
        self.by_rem[rem.min(8)] += 1;
        if self.nodes % BATCH == 0 {
            let late = self.shared.deadline.is_some_and(|d| std::time::Instant::now() > d);
            if late && self.shared.have_endpoint.load(Relaxed) {
                self.shared.timed_out.store(true, Relaxed);
                self.shared.stop_at.store(0, Relaxed);
            }
            if self.over() {
                return;
            }
        }
        // With no move to spare, only the moves that bring the centres closer can work;
        // with one to spare, also those that keep the distance.
        let mut allowed = self.last_move.followers[last.min(27) as usize] & match rem - n.d as usize {
            0 => n.closer,
            1 => n.closer | n.same,
            _ => UNKNOWN,
        };
        if rem == 1 {
            allowed &= self.last_move.terminal[n.parity as usize];
        }
        if allowed == 0 {
            return;
        }
        // Before the last move: where the two wings of each dedge are. Almost every
        // last move leaves some dedge with both wings in the same half, in every
        // frame, and can be skipped without looking at the centres.
        let mut frames = [u32::MAX; 3];
        if rem == 1 {
            let lm = self.last_move;
            for k in self.wings_known..depth {
                self.wings[k + 1] = crate::cube::compose_src(&lm.dest[self.path[k] as usize], &self.wings[k]);
            }
            self.wings_known = depth;
            let pos = &self.wings[depth];
            frames = [0; 3];
            for (f, mask) in frames.iter_mut().enumerate().take(lm.frames) {
                *mask = allowed & lm.live[f];
                let low = &lm.to_low[f];
                for i in 0..12 {
                    *mask &= low[pos[i] as usize] ^ low[pos[i + 12] as usize];
                    if *mask == 0 {
                        break;
                    }
                }
            }
            allowed = frames[0] | frames[1] | frames[2];
        }
        // The moves worth trying, each with the frames it may end an endpoint
        // in and its table entry asked for; then the steps themselves.
        let mut tries = [(0u8, 0u8, Ahead { rl: 0, ud: 0, at: 0 }); 27];
        let mut count = 0;
        while allowed != 0 {
            let m = allowed.trailing_zeros() as u8;
            allowed &= allowed - 1;
            let ok = ((frames[0] >> m & 1) | (frames[1] >> m & 1) << 1 | (frames[2] >> m & 1) << 2) as u8;
            if let Some(ahead) = self.solver.joint_ahead(n, m, rem - 1) {
                tries[count] = (m, ok, ahead);
                count += 1;
            }
        }
        for &(m, ok, ahead) in &tries[..count] {
            if let Some(next) = self.solver.joint_arrive(n, m, rem - 1, &ahead) {
                if rem == 1 {
                    self.frames_ok = ok;
                }
                self.path[depth] = m;
                self.wings_known = self.wings_known.min(depth);
                self.dfs(&next, rem - 1, m, depth + 1);
                if self.over() {
                    return;
                }
            }
        }
    }

    /// The centres are staged; test everything else phase 3 needs, in all three frames.
    fn leaf(&mut self, n: &Node, depth: usize) {
        self.shared.staged.fetch_add(1, Relaxed);
        // The wing parity is the same in every frame.
        if n.parity != 0 {
            return;
        }
        let solver = self.solver;
        let last = if depth > 0 { Some(self.path[depth - 1]) } else { None };
        if self.frames_ok & 1 == 1 && solver.rl_ok[(n.r >> 16) as usize] && !last.is_some_and(is_p3_move) {
            self.endpoint(self.variant, depth, None);
        }
        let partners = self.partners;
        for (k, part) in partners.iter().enumerate() {
            if (self.frames_ok >> (k + 1)) & 1 == 1
                && part.ok[((n.pr[k] >> part.shift) & 255) as usize]
                && !last.is_some_and(|m| is_p3_move(part.relabel[m as usize]))
            {
                self.endpoint(part.variant, depth, Some(&part.relabel));
            }
        }
    }

    /// Tests the current sequence, in the frame of `variant`, for pairable dedges
    /// and records it as an endpoint.
    fn endpoint(&mut self, variant: usize, depth: usize, relabel: Option<&[u8; 36]>) {
        self.shared.tried.fetch_add(1, Relaxed);
        let tabs = &self.solver.tabs;
        let mut moves = [0u8; 32];
        for (i, &m) in self.path[..depth].iter().enumerate() {
            moves[i] = relabel.map_or(m, |r| r[m as usize]);
        }
        let moves = &moves[..depth];
        // Almost every candidate fails on the wings, so look at those alone first.
        let ct = cube_tables();
        let mut ep = self.variants[variant].cube.ep;
        for &m in moves {
            let src = &ct.ep_src[m as usize];
            ep = crate::cube::compose_src(&ep, src);
        }
        if !wings_pairable(&ep) {
            return;
        }
        let mut cube = self.variants[variant].cube;
        cube.apply_seq(moves);
        debug_assert_eq!(tabs.p2[tabs.p2_index(p2_mask(&cube), cube.wing_parity())], 0);
        let dc = tabs.ct3[tabs.ct3_index(ct3_mask(&cube), ct3_parity(&cube))];
        if dc == UNREACHED {
            return;
        }
        let ds = tabs.sigma[sigma_rank(&sigma_of(&cube))];
        let len = moves.len() as u8;
        self.out.push(P2Cand { variant, len1: len, moves: moves.to_vec(), cube, len12: len, value: len + dc.max(ds) });
        self.shared.have_endpoint.store(true, Relaxed);
    }
}
