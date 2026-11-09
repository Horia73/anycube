//! Reduction solver: three reduction phases, then a 3x3x3 solve.
//!
//! The cube has no fixed centres, so it may be solved into any of the 24
//! orientations and along any of the three axes. Each (orientation class,
//! axis) pair is a "variant": the same physical cube re-expressed so that the
//! fixed phase definitions apply. Solutions are mapped back at the end.

use crate::c3::{C3, T3};
use crate::cube::{skip_move, tables as cube_tables, Cube};
use crate::tables::*;

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
    /// Minimum phase-2 probes of the 3x3x3 solver.
    pub probes333: u64,
}

impl Default for Params {
    fn default() -> Params {
        Params { n1: 10_000, k1: 500, n2: 100, k3: 100, probes333: 500 }
    }
}

#[derive(Clone, Debug)]
pub struct Solution {
    pub moves: Vec<u8>,
    /// Lengths of phase 1, 2, 3 and the 3x3x3 stage.
    pub lens: [usize; 4],
}

pub struct Solver {
    pub tabs: Tabs,
    pub t3: T3,
    /// Rotation indices of the three frames and six orientation classes.
    frames: [usize; 3],
    classes: [usize; 6],
}

struct Variant {
    cube: Cube,
    frame: usize,
}

struct P1Cand {
    variant: usize,
    moves: Vec<u8>,
    cube: Cube,
    mask: u32,
    parity: u8,
    value: u8,
}

struct P2Cand {
    c1: usize,
    moves: Vec<u8>,
    cube: Cube,
    len12: u8,
    value: u8,
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
        Solver { tabs: Tabs::load(), t3: T3::new(), frames, classes }
    }

    pub fn solve(&self, cube: &Cube, p: &Params) -> Result<Solution, String> {
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
                for (i, c) in cands.iter().enumerate() {
                    if c.value > len12 {
                        break;
                    }
                    let d2 = len12 - c.moves.len() as u8;
                    // After the first round only the newly allowed depth is searched.
                    if d2 > cap || (cap > 9 && d2 != cap) {
                        continue;
                    }
                    let last = c.moves.last().copied().unwrap_or(255);
                    self.dfs2(c.mask, c.parity, d2 as usize, last, 0, &mut path, i, c, &mut found, p.n2);
                    if found.len() >= p.n2 {
                        break 'p2;
                    }
                }
            }
            if cap >= 20 {
                break;
            }
            cap += 1;
        }
        if found.is_empty() {
            return Err("phase 2 found nothing".into());
        }
        found.sort_by_key(|c| c.value);

        // Phase 3: first solution by increasing total length.
        let k = found.len().min(p.k3);
        let mut cap = 13u8;
        let max_len12 = found[..k].iter().map(|c| c.len12).max().unwrap();
        let mut hit: Option<(usize, Vec<u8>)> = None;
        'p3: loop {
            for len123 in found[0].value..=max_len12 + cap {
                for (i, c) in found[..k].iter().enumerate() {
                    if c.value > len123 {
                        break;
                    }
                    let d3 = len123 - c.len12;
                    if d3 > cap || (cap > 13 && d3 != cap) {
                        continue;
                    }
                    let sg = sigma_of(&c.cube);
                    let mask = ct3_mask(&c.cube);
                    let par = ct3_parity(&c.cube);
                    let last = c.moves.last().or(cands[c.c1].moves.last()).copied().unwrap_or(255);
                    if self.dfs3(&sg, mask, par, d3 as usize, last, 0, &mut path) {
                        hit = Some((i, path[..d3 as usize].to_vec()));
                        break 'p3;
                    }
                }
            }
            if cap >= 24 {
                break;
            }
            cap += 1;
        }
        let (i, moves3) = hit.ok_or("phase 3 found nothing")?;
        let c2 = &found[i];
        let c1 = &cands[c2.c1];

        // 3x3x3 stage.
        let mut reduced = c2.cube;
        reduced.apply_seq(&moves3);
        let home = reduced.with_home_centres().ok_or("centres are not solved after phase 3")?;
        let c3 = C3::from_cube(&home).ok_or("dedges are not paired after phase 3")?;
        let moves333 = self.t3.solve(&c3, p.probes333);

        let frame = variants[c1.variant].frame;
        let moves: Vec<u8> = c1
            .moves
            .iter()
            .chain(c2.moves.iter())
            .chain(moves3.iter())
            .chain(moves333.iter())
            .map(|&m| ct.relabel[frame][m as usize])
            .collect();
        let mut check = *cube;
        check.apply_seq(&moves);
        if !check.is_solved() {
            return Err("internal error: solution does not solve the cube".into());
        }
        Ok(Solution { moves, lens: [c1.moves.len(), c2.moves.len(), moves3.len(), moves333.len()] })
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
        c1: usize,
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
            out.push(P2Cand { c1, moves: path[..depth].to_vec(), cube, len12, value: len12 + dc.max(ds) });
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
            if self.dfs2(x, q, rem - 1, m, depth + 1, path, c1, cand, out, limit) {
                return true;
            }
        }
        false
    }

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
    ) -> bool {
        let tabs = &self.tabs;
        if rem == 0 {
            return tabs.sigma[sigma_rank(sg)] == 0 && tabs.ct3[tabs.ct3_index(mask, parity)] == 0;
        }
        for (k, &m) in P3_MOVES.iter().enumerate() {
            if last != 255 && skip_move(last, m) {
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
            if self.dfs3(&s, x, q, rem - 1, m, depth + 1, path) {
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
