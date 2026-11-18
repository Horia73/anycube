//! Reduction solver: three reduction phases, then a 3x3x3 solve.
//!
//! The cube has no fixed centres, so it may be solved into any of the 24
//! orientations and along any of the three axes. Each (orientation class,
//! axis) pair is a "variant": the same physical cube re-expressed so that the
//! fixed phase definitions apply. Solutions are mapped back at the end.

use crate::big2::Big2;
use crate::ctab::{Ctab, Near};
use crate::c3::{C3, T3};
use crate::opt3::{Opt3, Outcome, Root};
use crate::cube::{skip_move, tables as cube_tables, Cube};
use crate::tables::*;
use crate::two3::Two3;
use rayon::prelude::*;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering::Relaxed};
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Params {
    /// Phase-1 solutions to enumerate.
    pub n1: usize,
    /// Phase-1 endpoints kept for phase 2.
    pub k1: usize,
    /// Phase-2 endpoints to collect.
    pub n2: usize,
    /// Phase-2 endpoints tried in phase 3.
    pub k3: usize,
    /// 3x3x3: the longest first phase of the two-phase search tried for every reduction ...
    pub first333: usize,
    /// Explore additional physical first turns, sharing the finishing search.
    pub alternate_starts: bool,
    /// ... and then, step by step, for the reductions with the shortest totals so far: a
    /// length of first phase and how many of them are searched to it.
    pub deeper333: Vec<(usize, usize)>,
    /// Reductions to collect; the one with the shortest total wins.
    pub n3: usize,
    /// How many moves past the shortest reduction to keep collecting.
    pub extra3: u8,
    /// Node budget of the exact 3x3x3 search over the collected reductions (0 = off).
    pub exact_nodes: i64,
    /// Longest 3x3x3 solution the exact search looks for.
    pub exact_cap: usize,
    /// Search phases 1 and 2 as one phase (needs the 2.37 GB centre table).
    pub joint: bool,
    /// Positions the joint phase may visit once it has found an endpoint.
    /// This, not a clock, is what ends it: the same cube then gives the
    /// same solution on every run and every machine.
    pub joint_nodes: i64,
    /// Soft time limit per cube in milliseconds, 0 for none (the default):
    /// with one, a stage that has something to show stops at its share of
    /// the limit, and what it has by then is different every run. The first
    /// results of each stage are always awaited, so a solve can run somewhat over.
    pub time_limit_ms: u64,
    /// One level past `extra3`, keep reductions whose 3x3x3 is within this many
    /// moves of a domino subgroup (0 = off) ...
    pub far_g: u8,
    /// ... from this many of the best phase-2 endpoints ...
    pub far_k: usize,
    /// ... looking at up to this many reductions per endpoint.
    pub far_n: usize,
}

impl Default for Params {
    fn default() -> Params {
        Params {
            n1: 10_000,
            k1: 500,
            n2: 10_000,
            k3: 10_000,
            first333: 11,
            alternate_starts: true,
            deeper333: vec![(13, 48)],
            n3: 2000,
            extra3: 1,
            exact_nodes: 0,
            exact_cap: 15,
            joint: true,
            joint_nodes: 30_000_000,
            time_limit_ms: 0,
            far_g: 0,
            far_k: 300,
            far_n: 20_000,
        }
    }
}

impl Params {
    /// About 0.3 s per cube on an M1 Pro.
    pub fn fast() -> Params {
        Params {
            n2: 300,
            k3: 300,
            joint_nodes: 20_000_000,
            first333: 10,
            alternate_starts: false,
            deeper333: vec![(12, 24)],
            n3: 200,
            ..Params::default()
        }
    }

    /// About 15 s per cube: twenty times the joint phase of the default, fifteen
    /// times as many endpoints, and an exact 3x3x3 search over the reductions.
    pub fn full() -> Params {
        Params {
            n2: 150_000,
            k3: 150_000,
            joint_nodes: 400_000_000,
            exact_nodes: 60_000_000,
            exact_cap: 16,
            deeper333: vec![(12, usize::MAX), (13, 128), (14, 8)],
            ..Params::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct Solution {
    pub moves: Vec<u8>,
    /// Lengths of phase 1, 2, 3 and the 3x3x3 stage.
    pub lens: [usize; 4],
    /// The shortest reduction the search came across; the solution may use a longer one.
    pub shortest_reduction: usize,
}

pub struct Solver {
    pub tabs: Tabs,
    pub t3: T3,
    pub big2: Big2,
    pub two: Two3,
    pub ctab: Ctab,
    pub(crate) near: Near,
    /// R-vs-L patterns on the R and L faces that phase 3 can finish.
    pub(crate) rl_ok: [bool; 256],
    /// Rotation indices of the three frames and six orientation classes.
    frames: [usize; 3],
    classes: [usize; 6],
}

pub(crate) struct Variant {
    pub cube: Cube,
    pub frame: usize,
}

struct P1Cand {
    variant: usize,
    moves: Vec<u8>,
    cube: Cube,
    mask: u32,
    parity: u8,
    value: u8,
}

struct Reduction {
    prefix: Vec<u8>,
    lens: [usize; 3],
    frame: usize,
    c3: C3,
}

impl Reduction {
    fn finish(&self, tail: &[u8], shortest: usize) -> Solution {
        let map = &cube_tables().relabel[self.frame];
        let mut moves = self.prefix.clone();
        moves.extend(tail.iter().map(|&m| map[m as usize]));
        Solution { moves, lens: [self.lens[0], self.lens[1], self.lens[2], tail.len()], shortest_reduction: shortest }
    }

    /// Charge the physical first turn at its actual cost. It commutes
    /// through the initial run about its axis and can cancel a turn from
    /// any of the three reduction phases.
    fn prepend(&mut self, m: u8) {
        let same = self.prefix.iter().take_while(|&&x| x / 3 % 3 == m / 3 % 3).position(|&x| x / 3 == m / 3);
        if let Some(i) = same {
            let turns = (self.prefix[i] % 3 + m % 3 + 2) % 4;
            if turns != 0 {
                self.prefix[i] = m / 3 * 3 + turns - 1;
            } else {
                let phase = if i < self.lens[0] {
                    0
                } else if i < self.lens[0] + self.lens[1] {
                    1
                } else {
                    2
                };
                self.lens[phase] -= 1;
                self.prefix.remove(i);
            }
        } else {
            self.prefix.insert(0, m);
            self.lens[0] += 1;
        }
    }
}

/// A state from which phase 3 can start.
pub(crate) struct P2Cand {
    pub variant: usize,
    /// Length of phase 1 alone (the whole prefix when phases 1 and 2 are searched as one).
    pub len1: u8,
    /// Moves of phases 1 and 2.
    pub moves: Vec<u8>,
    pub cube: Cube,
    pub len12: u8,
    /// Lower bound on the length of phases 1 to 3.
    pub value: u8,
}

impl Solver {
    pub fn new() -> Solver {
        let ct = cube_tables();
        let (x, y, z) = ([21u8, 32], [18u8, 29], [24u8, 35]);
        let c1: Vec<u8> = x.iter().chain(y.iter()).copied().collect();
        let c2: Vec<u8> = c1.iter().chain(c1.iter()).copied().collect();
        let frames = [0, ct.rot_index(&c1), ct.rot_index(&c2)];
        let classes = [
            0,
            ct.rot_index(&x),
            ct.rot_index(&y),
            ct.rot_index(&z),
            ct.rot_index(&c1),
            ct.rot_index(&c2),
        ];
        let t3 = T3::new();
        let big2 = Big2::load(&t3);
        let two = Two3::load(&t3);
        let tabs = Tabs::load();
        let ctab = Ctab::load(&tabs.bp, &tabs.ranks);
        let mut rl_ok = [false; 256];
        for (pat, ok) in rl_ok.iter_mut().enumerate() {
            *ok = pat.count_ones() == 4 && tabs.p2[tabs.p2_index(0x00FF | (pat as u32) << 16, 0)] == 0;
        }
        let near = Near::new(&tabs.bp);
        Solver { tabs, t3, big2, two, ctab, near, rl_ok, frames, classes }
    }

    /// The 18 variants of a cube: (re-expressed cube, frame rotation).
    pub fn variant_cubes(&self, cube: &Cube) -> Vec<(Cube, usize)> {
        let ct = cube_tables();
        let mut out = Vec::new();
        for &b in &self.frames {
            for &l in &self.classes {
                let mut v = cube.after_rotation(&ct.rots[l].state);
                v.apply_seq(&ct.rots[b].seq);
                out.push((v, b));
            }
        }
        out
    }

    pub fn solve(&self, cube: &Cube, p: &Params) -> Result<Solution, String> {
        self.solve_with_alternatives(cube, p, None)
    }

    fn reductions(&self, cube: &Cube, p: &Params, t_start: std::time::Instant, revisit: Option<usize>) -> Result<(Vec<Reduction>,usize), String> {
        cube.validate()?;
        let ct = cube_tables();
        let tabs = &self.tabs;

        // Variants, grouped by (frame, phase-1 mask).
        let mut variants = Vec::new();
        let mut groups: Vec<(usize, u32, Vec<usize>)> = Vec::new();
        for &b in &self.frames {
            for &l in &self.classes {
                let mut v = cube.after_rotation(&ct.rots[l].state);
                v.apply_seq(&ct.rots[b].seq);
                let mask = p1_mask(&v);
                let id = variants.len();
                variants.push(Variant { cube: v, frame: b });
                match groups.iter_mut().find(|g| g.0 == b && g.1 == mask) {
                    Some(g) => g.2.push(id),
                    None => groups.push((b, mask, vec![id])),
                }
            }
        }

        let debug = std::env::var_os("ANYCUBE_DEBUG").is_some();
        let limit = std::time::Duration::from_millis(p.time_limit_ms);
        let mut found: Vec<P2Cand> = if p.joint {
            self.joint_endpoints(&variants, p, (p.time_limit_ms > 0).then(|| t_start + limit.mul_f64(0.4)), revisit)?
        } else {
        // Phase 1: enumerate solutions by increasing length.
        let mut cands: Vec<P1Cand> = Vec::new();
        let dmin = groups.iter().map(|g| tabs.p1[g.1 as usize]).min().unwrap();
        let mut count = 0usize;
        let mut path = [0u8; 64];
        'p1: for d in dmin..=20 {
            for g in &groups {
                if tabs.p1[g.1 as usize] > d {
                    continue;
                }
                let mut sols = Vec::new();
                self.dfs1(g.1, d as usize, 255, 0, &mut path, &mut sols, p.n1 - count);
                count += sols.len();
                for s in sols {
                    for &vi in &g.2 {
                        let mut c = variants[vi].cube;
                        c.apply_seq(&s);
                        let mask = p2_mask(&c);
                        let parity = c.wing_parity();
                        let h = tabs.p2[tabs.p2_index(mask, parity)];
                        cands.push(P1Cand {
                            variant: vi,
                            moves: s.clone(),
                            cube: c,
                            mask,
                            parity,
                            value: s.len() as u8 + h,
                        });
                    }
                }
                if count >= p.n1 {
                    break 'p1;
                }
            }
        }
        cands.sort_by_key(|c| c.value);
        cands.truncate(p.k1);
        if cands.is_empty() {
            return Err("phase 1 found nothing".into());
        }

        // Phase 2: collect endpoints by increasing length of phases 1 + 2.
        let mut found: Vec<P2Cand> = Vec::new();
        let vmin = cands[0].value;
        let max_len1 = cands.iter().map(|c| c.moves.len() as u8).max().unwrap();
        let mut cap = 9u8;
        'p2: loop {
            for len12 in vmin..=max_len1 + cap {
                // Every eligible phase-1 endpoint is searched at this length, in parallel.
                let jobs: Vec<(&P1Cand, u8)> = cands
                    .iter()
                    .take_while(|c| c.value <= len12)
                    .map(|c| (c, len12 - c.moves.len() as u8))
                    // After the first round only the newly allowed depth is searched.
                    .filter(|&(_, d2)| d2 <= cap && (cap == 9 || d2 == cap))
                    .collect();
                let parts: Vec<Vec<P2Cand>> = jobs
                    .par_iter()
                    .map(|&(c, d2)| {
                        let mut path = [0u8; 64];
                        let mut out = Vec::new();
                        let last = c.moves.last().copied().unwrap_or(255);
                        self.dfs2(c.mask, c.parity, d2 as usize, last, 0, &mut path, c, &mut out, p.n2);
                        out
                    })
                    .collect();
                found.extend(parts.into_iter().flatten());
                if found.len() >= p.n2 {
                    break 'p2;
                }
            }
            if cap >= 20 {
                break;
            }
            cap += 1;
        }
        found
        };
        if found.is_empty() {
            return Err("phases 1 and 2 found nothing".into());
        }
        found.sort_by_key(|c| c.value);
        let t_12 = t_start.elapsed();
        if std::env::var_os("ANYCUBE_DEBUG").is_some() {
            let mut by_len = std::collections::BTreeMap::new();
            for c in &found {
                *by_len.entry(c.len12).or_insert(0usize) += 1;
            }
            eprintln!("debug: phase-2 endpoints by length of phases 1+2: {:?}", by_len);
        }

        // Phase 3: collect reductions by increasing length of phases 1 + 2 + 3.
        let k = found.len().min(p.k3);
        let mut cap = 13u8;
        let max_len12 = found[..k].iter().map(|c| c.len12).max().unwrap();
        let mut reds: Vec<(usize, Vec<u8>)> = Vec::new();
        let mut first_len: Option<u8> = None;
        'p3: loop {
            for len123 in found[0].value..=max_len12 + cap {
                if first_len.is_some_and(|f| len123 > f + p.extra3)
                    || (!reds.is_empty() && p.time_limit_ms > 0 && t_start.elapsed() > limit.mul_f64(0.6))
                {
                    break 'p3;
                }
                let jobs: Vec<(usize, &P2Cand, u8)> = found[..k]
                    .iter()
                    .enumerate()
                    .take_while(|(_, c)| c.value <= len123)
                    .map(|(i, c)| (i, c, len123 - c.len12))
                    .filter(|&(_, _, d3)| d3 <= cap && (cap == 13 || d3 == cap))
                    .collect();
                let parts: Vec<Vec<(usize, Vec<u8>)>> = jobs
                    .par_iter()
                    .map(|&(i, c, d3)| {
                        let mut path = [0u8; 64];
                        let mut out = Vec::new();
                        let sg = sigma_of(&c.cube);
                        let mask = ct3_mask(&c.cube);
                        let par = ct3_parity(&c.cube);
                        let last = c.moves.last().copied().unwrap_or(255);
                        self.dfs3(&sg, mask, par, d3 as usize, last, 0, &mut path, i, &mut out, p.n3);
                        out
                    })
                    .collect();
                reds.extend(parts.into_iter().flatten());
                if reds.len() >= p.n3 {
                    break 'p3;
                }
                if first_len.is_none() && !reds.is_empty() {
                    first_len = Some(len123);
                }
            }
            if first_len.is_some() || cap >= 24 {
                break;
            }
            cap += 1;
        }
        if reds.is_empty() {
            return Err("phase 3 found nothing".into());
        }
        // One level further out, reductions are ten times as many and one move longer.
        // Keep only those from which the 3x3x3 is close to a domino subgroup: its
        // two-phase search then starts with a short first phase.
        if let (Some(first), true) = (first_len, p.far_g > 0 && (p.time_limit_ms == 0 || t_start.elapsed() < limit.mul_f64(0.5))) {
            let level = first + p.extra3 + 1;
            let jobs: Vec<(usize, &P2Cand, u8)> = found[..k.min(p.far_k)]
                .iter()
                .enumerate()
                .filter(|(_, c)| c.value <= level && level - c.len12 <= cap)
                .map(|(i, c)| (i, c, level - c.len12))
                .collect();
            let far: Vec<(usize, Vec<u8>)> = jobs
                .par_iter()
                .flat_map_iter(|&(i, c, d3)| {
                    let mut path = [0u8; 64];
                    let mut out = Vec::new();
                    let last = c.moves.last().copied().unwrap_or(255);
                    self.dfs3(&sigma_of(&c.cube), ct3_mask(&c.cube), ct3_parity(&c.cube), d3 as usize, last, 0, &mut path, i, &mut out, p.far_n);
                    out.retain(|(_, moves3)| {
                        let mut reduced = c.cube;
                        reduced.apply_seq(moves3);
                        reduced
                            .with_home_centres()
                            .and_then(|h| C3::from_cube(&h))
                            .is_some_and(|c3| self.two.domino_distance(&self.t3, &c3, p.far_g).is_some())
                    });
                    out
                })
                .collect();
            if debug {
                eprintln!("debug: {} far reductions kept at length {}", far.len(), level);
            }
            reds.extend(far);
        }

        let mut seen = std::collections::HashSet::new();
        let mut items = Vec::new();
        for (i, moves3) in &reds {
            let c2 = &found[*i];
            let mut reduced = c2.cube;
            reduced.apply_seq(moves3);
            let home = reduced.with_home_centres().ok_or("centres are not solved after phase 3")?;
            let c3 = C3::from_cube(&home).ok_or("dedges are not paired after phase 3")?;
            if seen.insert(c3) {
                let frame = variants[c2.variant].frame;
                let prefix = c2.moves.iter().chain(moves3).map(|&m| ct.relabel[frame][m as usize]).collect();
                let first = c2.len1 as usize;
                items.push(Reduction { prefix, lens: [first, c2.moves.len() - first, moves3.len()], frame, c3 });
            }
        }
        if debug {
            let mut by_len = std::collections::BTreeMap::new();
            for it in &items {
                *by_len.entry(it.prefix.len()).or_insert(0usize) += 1;
            }
            let ends: std::collections::HashSet<_> = reds.iter().map(|r| r.0).collect();
            eprintln!(
                "debug: {} reductions, {} distinct reduced cubes by length {:?}, from {} phase-2 endpoints",
                reds.len(),
                items.len(),
                by_len,
                ends.len()
            );
            eprintln!(
                "debug: time phases 1+2 {:.0} ms, phase 3 {:.0} ms",
                t_12.as_secs_f64() * 1e3,
                (t_start.elapsed() - t_12).as_secs_f64() * 1e3
            );
        }
        Ok((items, found.iter().map(|c| c.len12 as usize).max().unwrap_or(0)))

    }

    pub(crate) fn solve_with_alternatives(
        &self, cube: &Cube, p: &Params, alternatives: Option<&mut Vec<Vec<u8>>>,
    ) -> Result<Solution, String> {
        let t_start = std::time::Instant::now();
        let limit = std::time::Duration::from_millis(p.time_limit_ms);
        let (items,last_level) = self.reductions(cube,p,t_start,None)?;
        // Pass 1: two-phase search on every reduction, then deeper on the ones that came out
        // best. Past its share of the time limit each takes the first solution it finds.
        let reduced: Vec<(usize, C3)> = items.iter().map(|it| (it.prefix.len(), it.c3)).collect();
        let two = self.two.finish(&self.t3, &reduced, p.first333, &p.deeper333, (p.time_limit_ms > 0).then(|| t_start + limit.mul_f64(0.8)));
        let mut best = (usize::MAX, 0usize, Vec::new());
        for (k, sol) in two.iter().enumerate() {
            let total = items[k].prefix.len() + sol.len();
            if total < best.0 {
                best = (total, k, sol.clone());
            }
        }
        // Pass 2: exact search. Scan every reduction for very short 3x3x3 solutions first
        // (cheap), then raise the limit one move at a time while the node budget lasts.
        if p.exact_nodes > 0 {
            let opt = Opt3::new(&self.t3, &self.big2);
            let roots: Vec<Root> = items.par_iter().map(|it| opt.prepare(&it.c3)).collect();
            let mut order: Vec<usize> = (0..items.len()).collect();
            order.sort_by_key(|&k| (items[k].prefix.len(), two[k].len()));
            let best_total = AtomicUsize::new(best.0);
            let budget = AtomicI64::new(p.exact_nodes);
            // Per reduction: the longest length already shown to have no solution.
            let proven: Vec<AtomicUsize> = roots.iter().map(|r| AtomicUsize::new(r.lower_bound().saturating_sub(1))).collect();
            let hits: Mutex<Vec<(usize, usize, Vec<u8>)>> = Mutex::new(Vec::new());
            for cap in 12.min(p.exact_cap)..=p.exact_cap {
                let next = AtomicUsize::new(0);
                (0..rayon::current_num_threads()).into_par_iter().for_each(|_| loop {
                    let j = next.fetch_add(1, Relaxed);
                    if j >= order.len() || budget.load(Relaxed) < 0 {
                        break;
                    }
                    let k = order[j];
                    let Some(room) = best_total.load(Relaxed).checked_sub(items[k].prefix.len() + 1) else {
                        continue;
                    };
                    let bound = room.min(cap);
                    let known = proven[k].load(Relaxed);
                    if bound <= known {
                        continue;
                    }
                    match opt.solve(&roots[k], known + 1, bound, &budget) {
                        Outcome::Solved(sol) => {
                            let total = items[k].prefix.len() + sol.len();
                            best_total.fetch_min(total, Relaxed);
                            // The solution is optimal: nothing shorter is left to find here.
                            proven[k].store(usize::MAX, Relaxed);
                            hits.lock().unwrap().push((total, k, sol));
                        }
                        Outcome::None => proven[k].store(bound, Relaxed),
                        Outcome::Aborted => {}
                    }
                });
                if budget.load(Relaxed) < 0 {
                    break;
                }
            }
            if std::env::var_os("ANYCUBE_DEBUG").is_some() {
                let mut opt_hist = std::collections::BTreeMap::new();
                for h in hits.lock().unwrap().iter() {
                    *opt_hist.entry(h.2.len()).or_insert(0usize) += 1;
                }
                let mut two_hist = std::collections::BTreeMap::new();
                for t in &two {
                    *two_hist.entry(t.len()).or_insert(0usize) += 1;
                }
                eprintln!(
                    "debug: two-phase lengths {:?}, exact hits by length {:?}, {} M nodes used",
                    two_hist,
                    opt_hist,
                    (p.exact_nodes - budget.load(Relaxed)) / 1_000_000
                );
            }
            for h in hits.into_inner().unwrap() {
                if (h.0, h.1) < (best.0, best.1) {
                    best = h;
                }
            }
        }
        let shortest = items.iter().map(|it| it.prefix.len()).min().unwrap_or(0);
        let (_, k, tail) = best;
        let mut result = items[k].finish(&tail, shortest);
        if let Some(alts) = alternatives {
            let mut order: Vec<_> = (0..items.len()).collect();
            order.sort_by_key(|&k| (items[k].prefix.len() + two[k].len(), items[k].prefix.len()));
            alts.extend(order.into_iter().take(8).map(|k| items[k].finish(&two[k], shortest).moves));
        }
        if p.alternate_starts && p.joint && p.time_limit_ms == 0 && result.moves.len() > 37 {
            let settings = Params {
                joint_nodes: if result.moves.len() > 40 { 12_000_000 } else { 4_000_000 },
                k3: 3000,
                n3: 400,
                alternate_starts: false,
                ..Params::default()
            };
            let mut extra = if result.moves.len() > 40 {
                self.reductions(cube, &settings, std::time::Instant::now(), Some(last_level)).map(|r| r.0).unwrap_or_default()
            } else {
                Vec::new()
            };
            let mut next = *cube;
            let m = 21; // Rw
            next.apply(m);
            if let Ok((mut batch, _)) = self.reductions(&next, &settings, std::time::Instant::now(), None) {
                for r in &mut batch {
                    r.prepend(m);
                }
                extra.extend(batch);
            }
            extra.sort_by_key(|r| r.prefix.len());
            let mut seen: std::collections::HashMap<C3, usize> = items.iter().map(|r| (r.c3, r.prefix.len())).collect();
            extra.retain(|r| {
                if seen.get(&r.c3).is_some_and(|&cost| cost <= r.prefix.len()) {
                    return false;
                }
                seen.insert(r.c3, r.prefix.len());
                true
            });
            extra.truncate(600);
            if let Some(r) = extra.first() {
                result.shortest_reduction = result.shortest_reduction.min(r.prefix.len());
            }
            let prices: Vec<_> = extra.iter().map(|r| (r.prefix.len(), r.c3)).collect();
            if !prices.is_empty() {
                let tails = self.two.finish(&self.t3, &prices, 10, &[(11, 64), (12, 24), (13, 6)], None);
                if let Some((r, tail)) = extra.iter().zip(&tails).min_by_key(|(r, t)| r.prefix.len() + t.len()) {
                    if r.prefix.len() + tail.len() < result.moves.len() {
                        result = r.finish(tail, result.shortest_reduction);
                    }
                }
            }
        }
        let mut check = *cube;
        check.apply_seq(&result.moves);
        if !check.is_solved() {
            return Err("internal error: solution does not solve the cube".into());
        }
        Ok(result)

    }

    /// The length of the shortest phase-3 solution of a cube phase 3 can start from.
    pub fn phase3_length(&self, cube: &Cube, max: usize) -> Option<usize> {
        let (sg, mask, par) = (sigma_of(cube), ct3_mask(cube), ct3_parity(cube));
        (0..=max).find(|&d| {
            let (mut path, mut out) = ([0u8; 64], Vec::new());
            self.dfs3(&sg, mask, par, d, 255, 0, &mut path, 0, &mut out, 1);
            !out.is_empty()
        })
    }

    /// Enumerates phase-1 solutions of exactly `rem` more moves. Returns true when `limit` is reached.
    fn dfs1(
        &self,
        mask: u32,
        rem: usize,
        last: u8,
        depth: usize,
        path: &mut [u8; 64],
        out: &mut Vec<Vec<u8>>,
        limit: usize,
    ) -> bool {
        if mask == P1_GOAL {
            if rem == 0 {
                out.push(path[..depth].to_vec());
                return out.len() >= limit;
            }
            // Leaving the goal and coming back is only worth it for longer detours.
            if rem < 5 {
                return false;
            }
        }
        if rem == 0 {
            return false;
        }
        for &m in &P1_MOVES {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let x = self.tabs.bp.apply(m, mask);
            if self.tabs.p1[x as usize] as usize > rem - 1 {
                continue;
            }
            path[depth] = m;
            if self.dfs1(x, rem - 1, m, depth + 1, path, out, limit) {
                return true;
            }
        }
        false
    }

    /// Enumerates phase-2 centre solutions and keeps those whose edges can be paired.
    #[allow(clippy::too_many_arguments)]
    fn dfs2(
        &self,
        mask: u32,
        parity: u8,
        rem: usize,
        last: u8,
        depth: usize,
        path: &mut [u8; 64],
        cand: &P1Cand,
        out: &mut Vec<P2Cand>,
        limit: usize,
    ) -> bool {
        let tabs = &self.tabs;
        if rem == 0 {
            if tabs.p2[tabs.p2_index(mask, parity)] != 0 {
                return false;
            }
            let mut cube = cand.cube;
            cube.apply_seq(&path[..depth]);
            if !edges_pairable(&cube) {
                return false;
            }
            let dc = tabs.ct3[tabs.ct3_index(ct3_mask(&cube), ct3_parity(&cube))];
            if dc == UNREACHED {
                return false;
            }
            let ds = tabs.sigma[sigma_rank(&sigma_of(&cube))];
            let len12 = (cand.moves.len() + depth) as u8;
            let mut moves = cand.moves.clone();
            moves.extend_from_slice(&path[..depth]);
            out.push(P2Cand {
                variant: cand.variant,
                len1: cand.moves.len() as u8,
                moves,
                cube,
                len12,
                value: len12 + dc.max(ds),
            });
            return out.len() >= limit;
        }
        for &m in &P2_MOVES {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            let x = tabs.bp.apply(m, mask);
            let q = parity ^ p2_flips_parity(m) as u8;
            if tabs.p2[tabs.p2_index(x, q)] as usize > rem - 1 {
                continue;
            }
            path[depth] = m;
            if self.dfs2(x, q, rem - 1, m, depth + 1, path, cand, out, limit) {
                return true;
            }
        }
        false
    }

    /// Enumerates phase-3 solutions of exactly `rem` more moves. Returns true when `limit` is reached.
    #[allow(clippy::too_many_arguments)]
    fn dfs3(
        &self,
        sg: &[u8; 12],
        mask: u32,
        parity: u8,
        rem: usize,
        last: u8,
        depth: usize,
        path: &mut [u8; 64],
        cand: usize,
        out: &mut Vec<(usize, Vec<u8>)>,
        limit: usize,
    ) -> bool {
        let tabs = &self.tabs;
        if rem == 0 {
            if tabs.sigma[sigma_rank(sg)] == 0 && tabs.ct3[tabs.ct3_index(mask, parity)] == 0 {
                out.push((cand, path[..depth].to_vec()));
                return out.len() >= limit;
            }
            return false;
        }
        for (k, &m) in P3_MOVES.iter().enumerate() {
            if last != 255 && skip_move(last, m) {
                continue;
            }
            // A reduction that ends with a face turn is a shorter reduction plus a move
            // the 3x3x3 stage could make itself.
            if rem == 1 && m < 18 {
                continue;
            }
            let x = tabs.bp.apply(m, mask);
            let q = parity ^ p3_flips_parity(m) as u8;
            if tabs.ct3[tabs.ct3_index(x, q)] as usize > rem - 1 {
                continue;
            }
            let s = sigma_move(&tabs.pl[k], &tabs.phinv[k], sg);
            if tabs.sigma[sigma_rank(&s)] as usize > rem - 1 {
                continue;
            }
            path[depth] = m;
            if self.dfs3(&s, x, q, rem - 1, m, depth + 1, path, cand, out, limit) {
                return true;
            }
        }
        false
    }
}

impl Default for Solver {
    fn default() -> Solver {
        Solver::new()
    }
}

#[cfg(test)]
mod reduction_tests {
    use super::*;

    #[test]
    fn setup_cost_and_phase_counts_agree_after_commuting() {
        for prefix in [vec![], vec![0], vec![0, 27, 11, 3], vec![4, 13, 22, 7], vec![6, 34, 8]] {
            for m in 0..36 {
                for split in 0..=prefix.len() {
                    let mut r = Reduction { prefix: prefix.clone(), lens: [split, 0, prefix.len() - split], frame: 0, c3: C3::identity() };
                    r.prepend(m);
                    let mut expected = Cube::from_moves(&[m]);
                    expected.apply_seq(&prefix);
                    assert_eq!(Cube::from_moves(&r.prefix), expected);
                    assert_eq!(r.lens.iter().sum::<usize>(), r.prefix.len());
                }
            }
        }
    }
}
