// Putting the six pictures of a scan together into a cube.
//
// A scan is six pictures, each an N by N grid of colours, asked for in a
// fixed order and each a fixed way up. People get that wrong: a face held
// a quarter turn round, two faces shown in each other's turn. The colours
// themselves do not depend on it (colour.js sorts them without knowing
// where a picture belongs), and a cube is a demanding thing: its eight
// corners must be eight different real corners, its edges real edges. So
// when the pictures do not make a cube as they were taken, every other way
// of placing and turning them is tried, and if exactly one makes a cube,
// that is the cube. In the same way, when no way makes a cube, two
// stickers the sorting was unsure about (a red and an orange, as a rule)
// are exchanged, and if that makes a cube in exactly one way, it is taken.
//
// What is checked here is what the pieces on the outside say: corners,
// the middle edges of an odd cube, the other edge pieces of larger cubes.
// It is everything for 2x2x2 and 3x3x3 and a necessary part for larger
// cubes, whose full check is the solver's (the page asks it afterwards).
//
// No DOM here, so that the tests can load it under Node.

import { CubeModel, FACES } from "./cube-model.js";
import { named, nameByCorner, oppositeCentres, sortColours, sortDistinctColours } from "./colour.js";

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const NORMAL = FACES.map((f) => f.normal);
// Two colours are of one axis when they belong on opposite faces (or are the same).
const axis = (colour) => colour % 3;

const shapes = new Map();

// The pieces on the outside of a cube of size n, as facelet indices.
function shape(n) {
  let s = shapes.get(n);
  if (s) return s;
  const model = new CubeModel(n);
  const pieces = new Map();
  for (let i = 0; i < model.count; i++) {
    const { p } = model.place(i);
    const cubie = p.map((v) => Math.max(-(n - 1), Math.min(n - 1, v)));
    const outer = cubie.filter((v) => Math.abs(v) === n - 1).length;
    if (outer < 2) continue;
    const key = cubie.join(",");
    if (!pieces.has(key)) pieces.set(key, { cubie, cells: [] });
    pieces.get(key).cells.push(i);
  }
  const face = (i) => Math.floor(i / (n * n));
  const corners = [];
  const edges = [];
  const wings = [];
  for (const { cubie, cells } of pieces.values()) {
    if (cells.length === 3) {
      // In an order that goes round the corner the same way for every corner.
      let [a, b, c] = cells;
      if (dot(cross(NORMAL[face(a)], NORMAL[face(b)]), NORMAL[face(c)]) < 0) [b, c] = [c, b];
      const ordered = [a, b, c];
      corners.push({ cells: ordered, faces: ordered.map(face), home: ordered.reduce((m, i) => m | (1 << face(i)), 0), updown: ordered.findIndex((i) => face(i) % 3 === 0) });
    } else {
      const along = cubie.find((v) => Math.abs(v) !== n - 1) ?? 0;
      const faces = cells.map(face);
      // The facelet an edge piece's flip is told by: the one on the top or bottom, else on the front or back.
      const lead = faces.findIndex((f) => f % 3 === 0) >= 0 ? faces.findIndex((f) => f % 3 === 0) : faces.findIndex((f) => f % 3 === 2);
      const piece = { cells, faces, home: (1 << faces[0]) | (1 << faces[1]), lead };
      if (n % 2 === 1 && along === 0) edges.push(piece);
      else wings.push({ ...piece, orbit: Math.abs(along) });
    }
  }
  s = { model, corners, edges, wings };
  shapes.set(n, s);
  return s;
}

function parity(perm) {
  const seen = new Array(perm.length).fill(false);
  let odd = 0;
  for (let i = 0; i < perm.length; i++) {
    if (seen[i]) continue;
    let length = 0;
    for (let j = i; !seen[j]; j = perm[j]) {
      seen[j] = true;
      length++;
    }
    odd ^= (length - 1) & 1;
  }
  return odd;
}

// Whether three colours, read round a corner, are a corner of a cube.
function cornerFits(a, b, c) {
  if (axis(a) === axis(b) || axis(a) === axis(c) || axis(b) === axis(c)) return false;
  return dot(cross(NORMAL[a], NORMAL[b]), NORMAL[c]) > 0;
}

/**
 * What the outside of a cube says is wrong with it, or null: `facelets`
 * are colours 0 to 5 (URFDLB) in facelet order.
 */
export function structure(n, facelets) {
  const { corners, edges, wings } = shape(n);
  const where = new Map(corners.map((c, k) => [c.home, k]));
  const perm = [];
  let twist = 0;
  for (const corner of corners) {
    const [a, b, c] = corner.cells.map((i) => facelets[i]);
    if (!cornerFits(a, b, c)) return "corner";
    const home = where.get((1 << a) | (1 << b) | (1 << c));
    if (home === undefined || perm.includes(home)) return "corner";
    perm.push(home);
    twist += ([a, b, c].findIndex((v) => v % 3 === 0) - corner.updown + 3) % 3;
  }
  if (twist % 3 !== 0) return "twist";
  if (edges.length) {
    const edgeHome = new Map(edges.map((e, k) => [e.home, k]));
    const edgePerm = [];
    let flip = 0;
    for (const edge of edges) {
      const [a, b] = edge.cells.map((i) => facelets[i]);
      if (axis(a) === axis(b)) return "edge";
      const home = edgeHome.get((1 << a) | (1 << b));
      if (home === undefined || edgePerm.includes(home)) return "edge";
      edgePerm.push(home);
      const leading = a % 3 === 0 || b % 3 === 0 ? (a % 3 === 0 ? 0 : 1) : a % 3 === 2 ? 0 : 1;
      flip ^= leading === edge.lead ? 0 : 1;
    }
    if (flip) return "flip";
    if (parity(perm) !== parity(edgePerm)) return "parity";
  }
  if (wings.length) {
    const seen = new Map();
    for (const wing of wings) {
      const [a, b] = wing.cells.map((i) => facelets[i]);
      if (axis(a) === axis(b)) return "wing";
      const key = wing.orbit * 64 + ((1 << a) | (1 << b));
      seen.set(key, (seen.get(key) ?? 0) + 1);
    }
    for (const count of seen.values()) if (count !== 2) return "wing";
  }
  return null;
}

// A grid of n by n turned by quarters, clockwise: where each cell of the result comes from.
const turned = new Map();
function turnMap(n, quarters) {
  const key = n * 4 + quarters;
  let map = turned.get(key);
  if (map) return map;
  map = new Int16Array(n * n);
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      let [sr, sc] = [r, c];
      for (let q = 0; q < quarters; q++) [sr, sc] = [n - 1 - sc, sr];
      map[r * n + c] = sr * n + sc;
    }
  }
  turned.set(key, map);
  return map;
}

/** The samples (or labels, or anything per sample) laid out as an arrangement puts them. */
export function arrange(n, values, { from, rot }) {
  const nn = n * n;
  const out = new (values.constructor === Array ? Array : values.constructor)(6 * nn);
  for (let s = 0; s < 6; s++) {
    const map = turnMap(n, rot[s]);
    for (let k = 0; k < nn; k++) out[s * nn + k] = values[from[s] * nn + map[k]];
  }
  return out;
}

const SAME = { from: [0, 1, 2, 3, 4, 5], rot: [0, 0, 0, 0, 0, 0] };
// The faces are placed in an order that closes corners early.
const ORDER = [2, 0, 1, 3, 4, 5];

/**
 * Every way of placing and turning the six pictures that makes the outside
 * of a cube, as different cubes: `labels` are the groups of `sortColours`,
 * picture by picture as taken.
 *
 * @returns a list, the one nearest to how the pictures were taken first,
 *   of `{ from, rot, moved, turned, facelets, key }`: `from[s]` is the
 *   picture that belongs on face s, `rot[s]` the quarter turns it was held
 *   round by, `moved` and `turned` the faces that were not where or how
 *   they were asked for.
 */
export function fit(n, sorted, labels = sorted.labels, cap = 400) {
  const nn = n * n;
  const { corners } = shape(n);
  const odd = n % 2 === 1;
  const mid = (nn - 1) / 2;
  // Per picture and turn, its labels as they would lie on a face.
  const lie = [];
  for (let g = 0; g < 6; g++) {
    lie.push([0, 1, 2, 3].map((q) => {
      const map = turnMap(n, q);
      return Array.from(map, (k) => labels[g * nn + k]);
    }));
  }
  // The corners that close when each face of ORDER is placed.
  const depth = new Array(6);
  ORDER.forEach((s, d) => (depth[s] = d));
  const closing = ORDER.map((_, d) => corners.filter((c) => Math.max(...c.faces.map((f) => depth[f])) === d));
  const found = new Map();
  const from = new Array(6).fill(-1);
  const rot = new Array(6).fill(0);
  const used = new Array(6).fill(false);
  const name = new Array(6).fill(-1);
  let leaves = 0;

  const colourAt = (i) => {
    const s = Math.floor(i / nn);
    return lie[from[s]][rot[s]][i - s * nn];
  };
  const cornersFit = (d) => {
    for (const corner of closing[d]) {
      const [a, b, c] = corner.cells.map(colourAt);
      if (a === b || a === c || b === c) return false;
      if (odd && !cornerFits(name[a], name[b], name[c])) return false;
    }
    return true;
  };
  const consider = (arrangement) => {
    const lying = arrange(n, labels, arrangement);
    const { facelets, palette } = named(n, sorted, lying);
    if (odd && !oppositeCentres(palette)) return;
    if (structure(n, facelets)) return;
    const key = cubeKey(n, facelets);
    // The same cube turned as a whole is 24 arrangements: the one nearest to how the pictures were taken.
    let best = null;
    for (const same of wholeTurned(n, arrangement)) {
      const moved = [];
      const turnedFaces = [];
      for (let s = 0; s < 6; s++) {
        if (same.from[s] !== s) moved.push(s);
        else if (same.rot[s]) turnedFaces.push(s);
      }
      const changes = moved.length + turnedFaces.length;
      if (!best || changes < best.changes) best = { ...same, moved, turned: turnedFaces, changes };
    }
    const was = found.get(key);
    if (!was || best.changes < was.changes) {
      const shown = arrange(n, labels, best);
      found.set(key, { ...best, facelets: named(n, sorted, shown).facelets, key });
    }
  };
  const leaf = () => {
    leaves++;
    consider({ from: from.slice(), rot: rot.slice() });
  };
  // Even cubes have no middles to name colours by while the pictures are
  // being placed, but a colour meets only four others at corners (never
  // itself, never its opposite): more than four, and this is not a cube.
  const meets = (d) => {
    if (odd) return true;
    const met = new Array(6).fill(0);
    for (let k = 0; k <= d; k++) {
      for (const corner of closing[k]) {
        const [a, b, c] = corner.cells.map(colourAt);
        met[a] |= (1 << b) | (1 << c);
        met[b] |= (1 << a) | (1 << c);
        met[c] |= (1 << a) | (1 << b);
      }
    }
    return met.every((m) => (m & 1) + ((m >> 1) & 1) + ((m >> 2) & 1) + ((m >> 3) & 1) + ((m >> 4) & 1) + ((m >> 5) & 1) <= 4);
  };
  const turnFaces = (d) => {
    // A cube that fits together in very many ways says nothing by it.
    if (found.size > cap || leaves > cap * 20) return;
    if (d === 6) return leaf();
    const s = ORDER[d];
    for (let q = 0; q < 4; q++) {
      rot[s] = q;
      if (cornersFit(d) && meets(d)) turnFaces(d + 1);
    }
    rot[s] = 0;
  };
  const place = (d) => {
    if (d === 6) {
      // On an odd cube a colour is named by where the picture with it in the middle lies.
      if (odd) {
        name.fill(-1);
        for (let s = 0; s < 6; s++) name[labels[from[s] * nn + mid]] = s;
        if (name.includes(-1)) return;
      }
      // The first face of ORDER is not turned: the cube turned as a whole is the same cube.
      return turnFaces(1);
    }
    const s = ORDER[d];
    for (let g = 0; g < 6; g++) {
      if (used[g]) continue;
      used[g] = true;
      from[s] = g;
      place(d + 1);
      used[g] = false;
    }
    from[s] = -1;
  };
  // As the pictures were taken, first: if they make a cube that way, that is the cube.
  consider(SAME);
  if (found.size) return [...found.values()];
  // Then every other way, with the first picture kept on the first face of ORDER.
  from[ORDER[0]] = 0;
  used[0] = true;
  place(1);
  return [...found.values()].sort((a, b) => a.changes - b.changes);
}

// The 24 ways of turning a whole cube, as where each facelet goes.
const wholeTurns = new Map();
function turnsOfCube(n) {
  let all = wholeTurns.get(n);
  if (all) return all;
  const { model } = shape(n);
  const matrices = [];
  const axes = [[1, 0, 0], [0, 1, 0], [0, 0, 1], [-1, 0, 0], [0, -1, 0], [0, 0, -1]];
  for (const x of axes) {
    for (const y of axes) {
      if (dot(x, y) !== 0) continue;
      matrices.push([x, y, cross(x, y)]);
    }
  }
  all = matrices.map((m) => {
    const to = new Int32Array(model.count);
    for (let i = 0; i < model.count; i++) {
      const { p } = model.place(i);
      const q = [0, 1, 2].map((k) => m[0][k] * p[0] + m[1][k] * p[1] + m[2][k] * p[2]);
      to[i] = model.indexAt(q[0], q[1], q[2]);
    }
    return to;
  });
  wholeTurns.set(n, all);
  return all;
}

// An arrangement of the pictures, turned as a whole each of the 24 ways.
function wholeTurned(n, { from, rot }) {
  const nn = n * n;
  // Which sample lies at each facelet.
  const lying = arrange(n, Int32Array.from({ length: 6 * nn }, (_, i) => i), { from, rot });
  const first = [0, 1, 2, 3].map((q) => turnMap(n, q)[0]);
  return turnsOfCube(n).map((to) => {
    const moved = new Int32Array(lying.length);
    for (let i = 0; i < lying.length; i++) moved[to[i]] = lying[i];
    const out = { from: [], rot: [] };
    for (let s = 0; s < 6; s++) {
      const sample = moved[s * nn];
      out.from.push(Math.floor(sample / nn));
      out.rot.push(first.indexOf(sample % nn));
    }
    return out;
  });
}

/**
 * A cube turned as a whole is the same cube, though its stickers are then
 * in other places and, named by where their colours belong, have other
 * names. Of its 24 descriptions, the first in order: equal for two cubes
 * exactly when one is the other turned.
 */
export function cubeKey(n, facelets) {
  const nn = n * n;
  const mid = (nn - 1) / 2;
  let best = null;
  for (const to of turnsOfCube(n)) {
    const moved = new Uint8Array(facelets.length);
    for (let i = 0; i < facelets.length; i++) moved[to[i]] = facelets[i];
    let name;
    if (n % 2 === 1) {
      name = new Array(6).fill(-1);
      for (let s = 0; s < 6; s++) name[moved[s * nn + mid]] = s;
    } else {
      name = nameByCorner(n, moved);
    }
    if (!name || name.includes(-1)) continue;
    const text = Array.from(moved, (c) => name[c]).join("");
    if (best === null || text < best) best = text;
  }
  return best ?? facelets.join("");
}

function fitWithoutRecolouring(n, sorted) {
  const ways = fit(n, sorted);
  if (ways.length && ways[0].changes === 0) return { kind: "taken", ...ways[0], labels: sorted.labels };
  if (ways.length === 1) return { kind: "placed", ...ways[0], labels: sorted.labels };
  if (ways.length > 1) return { kind: "several", ...SAME, moved: [], turned: [], ways: ways.length, labels: sorted.labels };
  return null;
}

/** Uses the distinct observed colours when they fit together as a cube. */
export function assembleScan(n, samples) {
  const distinct = sortDistinctColours(samples, n);
  if (distinct) {
    const made = fitWithoutRecolouring(n, distinct);
    if (made?.kind === "taken" || made?.kind === "placed") return { sorted: distinct, made };
  }
  const sorted = sortColours(samples, n);
  return { sorted, made: assemble(n, sorted) };
}

/**
 * The cube the six pictures make.
 *
 * @param sorted what `sortColours` gave for the samples as taken
 * @returns `{ kind, from, rot, moved, turned, swapped, ways }`:
 *   kind "taken": the pictures make a cube as they were taken;
 *   "placed": only when placed or turned another way, the one way there is;
 *   "recoloured": only with two doubtful stickers exchanged (`swapped`, as sample indices as taken);
 *   "several": more than one cube can be made of them (`ways` of them), none as taken;
 *   "none": no cube can be made of them.
 *   For "taken", "placed" and "recoloured", `labels` are the groups to go
 *   on with (those of `sorted`, two exchanged for "recoloured").
 */
export function assemble(n, sorted) {
  const fitted = fitWithoutRecolouring(n, sorted);
  if (fitted) return fitted;
  // Two stickers each of which was nearly sorted where the other is: the likeliest mistake.
  const { labels, doubt, second } = sorted;
  const nn = n * n;
  const mid = n % 2 === 1 ? (nn - 1) / 2 : -1;
  const pairs = [];
  for (let i = 0; i < labels.length; i++) {
    if (i % nn === mid) continue;
    for (let j = i + 1; j < labels.length; j++) {
      if (j % nn === mid || labels[i] === labels[j]) continue;
      if (second[i] !== labels[j] || second[j] !== labels[i]) continue;
      pairs.push([doubt[i] + doubt[j], i, j]);
    }
  }
  pairs.sort((a, b) => b[0] - a[0]);
  const made = new Map();
  const began = performance.now();
  for (const [unsure, i, j] of pairs.slice(0, 30)) {
    // A third of a second at most: it runs between the last capture and the cube appearing.
    if (performance.now() - began > 350) break;
    const tried = Int8Array.from(labels);
    [tried[i], tried[j]] = [tried[j], tried[i]];
    for (const way of fit(n, sorted, tried, 8)) {
      const was = made.get(way.key);
      if (!was || way.changes < was.changes) made.set(way.key, { ...way, swapped: [i, j], unsure, labels: tried });
    }
    if (made.size > 3) break;
  }
  // As taken with one exchange is believed before another placing with another
  // exchange; of two exchanges as taken, the one of the stickers the sorting was least sure of.
  const best = [...made.values()].sort((a, b) => a.changes - b.changes || b.unsure - a.unsure);
  const clear =
    best.length === 1 ||
    (best.length > 1 && best[0].changes === 0 && (best[1].changes > 0 || best[0].unsure - best[1].unsure > 0.15));
  if (clear) return { kind: "recoloured", ...best[0] };
  if (best.length > 1) return { kind: "several", ...SAME, moved: [], turned: [], ways: best.length, labels: sorted.labels };
  return { kind: "none", ...SAME, moved: [], turned: [], labels: sorted.labels };
}
