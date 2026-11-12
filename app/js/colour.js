// From the colours a camera saw on the six faces to the six colours of a cube.
//
// Nothing here knows what "red" looks like. The 6*N*N samples are sorted
// into six groups of N*N each by how alike they are, starting from the
// middle sticker of each face (which on a cube of odd size is that face's
// colour by definition), so the light in the room, the camera's white
// balance and the shade of a particular cube's stickers matter little:
// what counts is that a cube's reds are more like each other than like its
// oranges. A cube of even size has no middle stickers; its groups start
// from the six samples furthest apart, and a corner says which group
// belongs on which face.
//
// Red and orange are the pair that goes wrong, and glossy stickers are
// what makes them, so the comparison is built for both:
//
// - A sample is taken apart into how much white is in it and what is left,
//   its colour proper. A sticker that throws the light back shows its
//   colour with the light's white added, so its colour proper is still
//   there, as strong as without the sheen: a red under a sheen is pink,
//   and what is left of pink when the white is taken out is red.
// - A group is known by the direction of its colour proper (its hue), by
//   how much white its purest stickers have in them, and by how strong its
//   colour usually is. A sample is near a group when its colour proper lies
//   along the group's, whatever white is on top; a little is counted for
//   that white, and for being much stronger or weaker than the group is,
//   which is what tells an orange from a red of nearly the same hue. A
//   group with no colour of its own (white, or black) is known by that.
// - A shadow, or a darker exposure, makes the white and the colour smaller
//   together and moves no direction.
// - A different white balance multiplies the red, the green and the blue
//   of a whole picture, each by its own number, and a different exposure
//   all three by one. Those numbers are estimated for each of the six
//   pictures, from how its samples sit against the groups they fell into,
//   and taken out; the camera changes both between one face and the next.
// - Every colour is on exactly N*N stickers, so all samples are placed
//   together, the surest first.
//
// No DOM here, so that the tests can load it under Node.

import { CubeModel } from "./cube-model.js";

// The page's sticker colours, in face order U R F D L B.
export const STICKERS = ["#f4f4ef", "#d92b3a", "#1faa59", "#f7d038", "#f58220", "#2f6fde"];
export const BLACK_STICKER = "#24252b";

// The three opposite pairs of the standard sticker colours, in any orientation.
export function oppositeCentres(palette) {
  const slot = (colour) => colour === BLACK_STICKER ? 0 : STICKERS.indexOf(colour);
  return palette.slice(0, 3).every((colour, face) => {
    const k = slot(colour);
    return k >= 0 && slot(palette[face + 3]) === (k + 3) % 6;
  });
}

function toLab([r, g, b]) {
  const lin = (v) => {
    v /= 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  const [R, G, B] = [lin(r), lin(g), lin(b)];
  const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  const x = f((0.4124 * R + 0.3576 * G + 0.1805 * B) / 0.95047);
  const y = f(0.2126 * R + 0.7152 * G + 0.0722 * B);
  const z = f((0.0193 * R + 0.1192 * G + 0.9505 * B) / 1.08883);
  return [116 * y - 16, 500 * (x - y), 200 * (y - z)];
}

// A colour as three numbers that say what it looks like to the eye:
// lightness, with a small weight, and the two colour axes of CIELAB made
// nearly independent of the light. Used to find the page colour nearest a
// group of stickers, where the group's mean has little noise left.
export function feature(rgb) {
  return labFeature(toLab(rgb));
}

function labFeature([L, a, b]) {
  return [L * 0.25, (100 * a) / (L + 60), (100 * b) / (L + 60)];
}

// What the sorting itself compares, in units of what a sample of the same
// colour would differ by: a few steps of the camera's scale for a dark
// sample, a few hundredths of its own size for a bright one.
const NOISE = 4; // steps of the 8-bit scale
const UNEVEN = 0.035; // how much stickers of one colour differ in hue, as a share of their size
const PALE = 0.15; // a group with less colour than this share of its light has none of its own
const SHEEN = 2.5; // what a sample that is all sheen and no colour counts against a coloured group
const STRENGTH = 0.3; // how far the strength of a colour varies within one group, as a logarithm
const WHITE = 1 / Math.sqrt(3);

// A sample taken apart: `x` is how much white (or grey) is in it, `c`
// what is left, its colour proper, and `y` how strong that is.
function split(rgb) {
  const x = (rgb[0] + rgb[1] + rgb[2]) * WHITE;
  const c = [rgb[0] - x * WHITE, rgb[1] - x * WHITE, rgb[2] - x * WHITE];
  return { x, c, y: Math.hypot(c[0], c[1], c[2]), length: Math.hypot(rgb[0], rgb[1], rgb[2]) };
}

// Brightness says little: a shadow halves it. But a sample several times
// darker or lighter than a group is (a black sticker beside a white one)
// is not of that group. Nothing is counted up to a difference a shadow
// could make, and more beyond it.
const SHADE = 0.9; // as a logarithm: about two and a half times
function unlike(length, usual) {
  const beyond = Math.abs(Math.log((length + NOISE) / (usual + NOISE))) - SHADE;
  return beyond > 0 ? 3 * beyond : 0;
}

// What a group is like, from its samples (taken apart): `hue` the direction
// of its colour proper, or null if it has none; `ratio` the white there is
// to the colour in its purest samples; `usual` how strong its colour
// usually is (how bright it is, for a group without colour).
function describe(members, before) {
  if (!members.length) return before;
  const middle = (values, share = 0.5) => {
    const sorted = values.slice().sort((p, q) => p - q);
    return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))];
  };
  const mean = (values) => Math.exp(values.reduce((t, v) => t + Math.log(v + NOISE), 0) / values.length) - NOISE;
  if (middle(members.map((m) => Math.max(0, m.y - NOISE) / (m.length + NOISE))) < PALE) {
    return { hue: null, ratio: 0, usual: mean(members.map((m) => m.length)) };
  }
  // The directions laid end to end, a weak colour counting for little: one
  // strong sample that strayed into a group does not become its hue by itself.
  const sum = [0, 0, 0];
  for (const m of members) {
    const weight = 1 / Math.sqrt(m.y * m.y + 4 * NOISE * NOISE);
    for (let d = 0; d < 3; d++) sum[d] += m.c[d] * weight;
  }
  const hue = unit(sum);
  const along = members.map((m) => Math.max(0, m.c[0] * hue[0] + m.c[1] * hue[1] + m.c[2] * hue[2]));
  const ratios = members.map((m, i) => m.x / (along[i] + NOISE)).filter((_, i) => along[i] > 3 * NOISE);
  return { hue, ratio: ratios.length ? middle(ratios, 0.25) : 0, usual: mean(along) };
}

// How far a sample is from a group.
function apart(m, group) {
  const tolerance = Math.sqrt(NOISE * NOISE + UNEVEN * UNEVEN * m.length * m.length);
  if (!group.hue) return Math.hypot(m.y / tolerance, unlike(m.length, group.usual));
  const along = m.c[0] * group.hue[0] + m.c[1] * group.hue[1] + m.c[2] * group.hue[2];
  const a = Math.max(0, along);
  // Off the group's hue; and, counted the same, having less white than the
  // group's purest samples, which no sheen explains.
  const excess = m.x - a * group.ratio;
  const off = Math.sqrt(Math.max(0, m.y * m.y - a * a) + (excess < 0 ? excess * excess : 0));
  const sheen = excess > 0 ? (SHEEN * excess) / (m.length + NOISE) : 0;
  const strength = Math.min(2.5, Math.abs(Math.log((a + NOISE) / (group.usual + NOISE))) / STRENGTH);
  return Math.hypot(off / tolerance, sheen, strength);
}

function unit(v) {
  const length = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / length, v[1] / length, v[2] / length];
}

function distance(p, q) {
  return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]);
}

// Allow modest changes in exposure when comparing repeated camera readings.
export function similarReading(a, b) {
  return distance(toLab(a), toLab(b)) <= 12;
}

function distantSeeds(total, first, between) {
  const seeds = [first];
  const near = Array.from({ length: total }, (_, i) => between(i, first));
  while (seeds.length < 6) {
    let far = 0;
    near.forEach((d, i) => {
      if (d > near[far]) far = i;
    });
    seeds.push(far);
    near.forEach((d, i) => (near[i] = Math.min(d, between(i, far))));
  }
  return seeds;
}

function groupMeans(samples, labels, each) {
  const sums = Array.from({ length: 6 }, () => [0, 0, 0]);
  samples.forEach((sample, i) => sample.forEach((value, d) => (sums[labels[i]][d] += value)));
  return sums.map((sum) => sum.map((value) => value / each));
}

function hexToRgb(hex) {
  const v = parseInt(hex.slice(1), 16);
  return [v >> 16, (v >> 8) & 255, v & 255];
}

// The assignment of six things to six others that costs least.
function bestMatch(cost) {
  let best = null;
  let bestCost = Infinity;
  const perm = [];
  const used = new Array(6).fill(false);
  const go = (i, total) => {
    if (total >= bestCost) return;
    if (i === 6) {
      bestCost = total;
      best = perm.slice();
      return;
    }
    for (let k = 0; k < 6; k++) {
      if (used[k]) continue;
      used[k] = true;
      perm[i] = k;
      go(i + 1, total + cost[i][k]);
      used[k] = false;
    }
  };
  go(0, 0);
  return best;
}

// Sorts the samples into six groups of equal size. `seeds` are six sample
// indices to start the groups from; `pinned` says whether a seed stays in
// its own group whatever happens (the middle stickers of an odd cube).
// `cast` is what to divide the samples of each picture by before comparing
// (see `sortColours`).
function group(samples, seeds, pinned, each, cast, start, allow = 0) {
  const total = samples.length;
  const at = samples.map((p, i) => {
    const c = cast[Math.floor(i / each)];
    return split([p[0] / c[0], p[1] / c[1], p[2] / c[2]]);
  });
  // A group begun from one sample cannot tell how little white its colour
  // has: that sample may be one under a sheen. `allow` is the share of the
  // sample's own white that others of the group must have at least.
  let groups = start?.groups ?? seeds.map((i) => ((g) => ({ ...g, ratio: g.ratio * allow }))(describe([at[i]], null)));
  let labels = new Int8Array(total).fill(-1);
  const fixed = new Int8Array(total).fill(-1);
  if (pinned) seeds.forEach((i, k) => (fixed[i] = k));
  let cost = [];
  for (let round = 0; round < 16; round++) {
    cost = at.map((m) => groups.map((g) => apart(m, g)));
    const next = new Int8Array(total).fill(-1);
    const room = new Array(6).fill(each);
    for (let i = 0; i < total; i++) {
      if (fixed[i] >= 0) {
        next[i] = fixed[i];
        room[fixed[i]]--;
      }
    }
    // Those with most to lose by not getting their first choice choose first.
    const order = [];
    for (let i = 0; i < total; i++) {
      if (next[i] >= 0) continue;
      const sorted = cost[i].slice().sort((a, b) => a - b);
      order.push([sorted[1] - sorted[0], i]);
    }
    order.sort((a, b) => b[0] - a[0]);
    for (const [, i] of order) {
      let k = -1;
      for (let c = 0; c < 6; c++) if (room[c] > 0 && (k < 0 || cost[i][c] < cost[i][k])) k = c;
      next[i] = k;
      room[k]--;
    }
    // Then any two that would both be better off, or better off together, change places.
    for (let improved = true, pass = 0; improved && pass < 20; pass++) {
      improved = false;
      for (let i = 0; i < total; i++) {
        if (fixed[i] >= 0) continue;
        for (let j = i + 1; j < total; j++) {
          if (fixed[j] >= 0 || next[i] === next[j]) continue;
          if (cost[i][next[j]] + cost[j][next[i]] < cost[i][next[i]] + cost[j][next[j]] - 1e-9) {
            [next[i], next[j]] = [next[j], next[i]];
            improved = true;
          }
        }
      }
    }
    const same = next.every((v, i) => v === labels[i]);
    labels = next;
    if (same) break;
    groups = groups.map((old, k) => describe(at.filter((_, i) => labels[i] === k), old));
  }
  // How unsure each sample is: 0 when its group is far nearer than any other, 1 when two are
  // as near; and which group that other is.
  const doubt = new Float32Array(total);
  const second = new Int8Array(total);
  for (let i = 0; i < total; i++) {
    const own = cost[i][labels[i]];
    let other = -1;
    for (let c = 0; c < 6; c++) if (c !== labels[i] && (other < 0 || cost[i][c] < cost[i][other])) other = c;
    second[i] = other;
    doubt[i] = Math.max(0, Math.min(1, own / (cost[i][other] + 1e-6)));
  }
  let spread = 0;
  for (let i = 0; i < total; i++) spread += cost[i][labels[i]];
  return { labels, doubt, second, spread, groups, at };
}

// What each picture's red, green and blue are multiplied by, next to the
// others: for each channel, the number that best turns what the groups
// say the picture's samples should be into what they are; and with it how
// much brighter or darker the whole picture is. What a sample should be is
// its group's colour proper at the group's usual strength, with the white
// the sample itself has. A face with one or two colours on it says little
// (the numbers and the colours cannot be told apart there), so its numbers
// are taken in part only, and left at one where a single colour fills it.
function casts(sorted, each) {
  const { labels, groups, at } = sorted;
  const out = [];
  for (let f = 0; f < 6; f++) {
    const top = [0, 0, 0];
    const bottom = [0, 0, 0];
    const seen = new Set();
    let brighter = 0;
    let counted = 0;
    for (let i = f * each; i < (f + 1) * each; i++) {
      const g = groups[labels[i]];
      const m = at[i];
      seen.add(labels[i]);
      let should;
      if (g.hue) {
        const along = Math.max(0, m.c[0] * g.hue[0] + m.c[1] * g.hue[1] + m.c[2] * g.hue[2]);
        should = [0, 1, 2].map((d) => along * g.hue[d] + m.x * WHITE);
        if (along > 3 * NOISE) {
          brighter += Math.log((along + NOISE) / (g.usual + NOISE));
          counted++;
        }
      } else {
        should = [m.x * WHITE, m.x * WHITE, m.x * WHITE];
        brighter += Math.log((m.length + NOISE) / (g.usual + NOISE));
        counted++;
      }
      const is = [m.c[0] + m.x * WHITE, m.c[1] + m.x * WHITE, m.c[2] + m.x * WHITE];
      for (let d = 0; d < 3; d++) {
        top[d] += is[d] * should[d];
        bottom[d] += should[d] * should[d];
      }
    }
    const share = Math.max(0, Math.min(1, (seen.size - 1) / 3));
    const raw = top.map((t, d) => (bottom[d] > 1 ? Math.max(0.5, Math.min(2, t / bottom[d])) : 1));
    // How the three differ from each other is the cast; how bright the picture is, by itself.
    const middle = Math.cbrt(raw[0] * raw[1] * raw[2]);
    const gain = counted ? Math.exp(Math.max(-0.7, Math.min(0.7, brighter / counted))) : 1;
    out.push(raw.map((v) => ((v / middle) * gain) ** share));
  }
  return out;
}

/** Groups clearly separated camera colours by their full CIELAB distance. */
export function sortDistinctColours(samples, n) {
  const each = n * n;
  if (samples.length !== 6 * each) return null;
  const points = samples.map(toLab);
  const seeds = n % 2 === 1
    ? Array.from({ length: 6 }, (_, face) => face * each + Math.floor(each / 2))
    : distantSeeds(points.length, 0, (a, b) => distance(points[a], points[b]));
  let references = seeds.map((i) => points[i]);
  const ranked = (point) => references
    .map((reference, label) => ({ label, cost: distance(point, reference) }))
    .sort((a, b) => a.cost - b.cost);
  const balanced = (labels) => {
    const counts = new Array(6).fill(0);
    for (const label of labels) counts[label]++;
    return counts.every((count) => count === each);
  };
  if (n % 2 === 0) {
    // The farthest stickers may sit at the edge of each colour group.
    const initial = points.map((point) => ranked(point)[0].label);
    if (!balanced(initial)) return null;
    references = groupMeans(points, initial, each);
  }

  const labels = new Int8Array(points.length);
  const second = new Int8Array(points.length);
  const doubt = new Float32Array(points.length);
  for (const [i, point] of points.entries()) {
    const [closest, next] = ranked(point);
    // Leave close readings to the sorter that models glare and exposure.
    if (closest.cost * 2 >= next.cost) return null;
    labels[i] = closest.label;
    second[i] = next.label;
    doubt[i] = closest.cost / next.cost;
  }
  // Accept only when every sticker's first choice gives the right counts.
  if (!balanced(labels)) return null;
  return { labels, second, doubt, colours: groupMeans(samples, labels, each) };
}

/**
 * Sorts the camera's samples into six groups of n*n, without naming them.
 * The samples are in six runs of n*n, one run per picture (in any order of
 * faces, each picture turned any way: the sorting does not depend on it).
 *
 * @returns `labels` (0 to 5 per sample; on a cube of odd size group k is
 *   the one the middle sample of run k is in), `doubt` (0 to 1 per sample:
 *   how nearly it belonged to another group), `second` (that other group)
 *   and `colours` (the mean colour the camera saw for each group).
 */
export function sortColours(samples, n) {
  const nn = n * n;
  if (samples.length !== 6 * nn) throw new Error("expected 6 faces of n by n samples");
  const none = Array.from({ length: 6 }, () => [1, 1, 1]);
  const run = (seeds, pinned, allow) => {
    let cast = none;
    let best = group(samples, seeds, pinned, nn, cast, null, allow);
    // The pictures' own casts, taken out a few times over: each sorting says them better.
    for (let round = 0; round < 3; round++) {
      const more = casts(best, nn);
      if (more.every((c) => c.every((v) => Math.abs(v - 1) < 0.004))) break;
      cast = cast.map((c, f) => c.map((v, d) => v * more[f][d]));
      best = group(samples, seeds, pinned, nn, cast, best);
    }
    return best;
  };
  let best;
  if (n % 2 === 1) {
    const mid = Math.floor(nn / 2);
    // The middle stickers begin the groups. One of them may lie under a
    // sheen, and then says its colour has more white than it has; or two of
    // them may be one hue (a white and a yellow under a yellow lamp) and
    // differ in their white alone. So the sorting is done holding the groups
    // to their middle sticker's white not at all, in part and wholly, and
    // the one whose groups are tightest is kept.
    for (const allow of [0, 0.5, 1]) {
      const tried = run([0, 1, 2, 3, 4, 5].map((f) => f * nn + mid), true, allow);
      if (!best || tried.spread < best.spread) best = tried;
    }
  } else {
    // Six samples far apart: one to begin with, then each time the one
    // furthest from all chosen so far. A glint or a shadow can be one of
    // them and spoil the start, so several beginnings are tried and the
    // sorting whose groups are tightest is kept.
    const parts = samples.map(split);
    const alone = parts.map((m) => describe([m], null));
    const between = (i, j) => Math.min(apart(parts[i], alone[j]), apart(parts[j], alone[i]));
    const colourful = (i) => parts[i].y / Math.sqrt(NOISE * NOISE + UNEVEN * UNEVEN * parts[i].length * parts[i].length);
    const byColour = samples.map((_, i) => i).sort((i, j) => colourful(j) - colourful(i));
    const starts = [0, 0.2, 0.4, 0.6, 0.8, 1].map((q) => byColour[Math.round(q * (byColour.length - 1))]);
    for (const first of starts) {
      const seeds = distantSeeds(samples.length, first, between);
      const tried = run(seeds, false, 0);
      if (!best || tried.spread < best.spread) best = tried;
    }
  }
  const { labels, doubt, second } = best;
  return { labels, doubt, second, colours: groupMeans(samples, labels, nn) };
}

// The eight corners as triples of facelet indices, by their place: bit 0
// set for the right half, bit 1 for the top, bit 2 for the front.
function corners(n) {
  const model = new CubeModel(n);
  const out = Array.from({ length: 8 }, () => []);
  for (let i = 0; i < model.count; i++) {
    const { p } = model.place(i);
    if (p.filter((v) => Math.abs(v) >= n - 1).length === 3) {
      out[(p[0] > 0 ? 1 : 0) | (p[1] > 0 ? 2 : 0) | (p[2] > 0 ? 4 : 0)].push(i);
    }
  }
  return out;
}

// Which face each group of an even cube belongs on, from its corners: two
// colours that never meet at a corner are opposite, and the corner at the
// bottom, back and left is taken to be at home, which fixes the rest.
export function nameByCorner(n, labels) {
  const meet = Array.from({ length: 6 }, () => new Array(6).fill(false));
  const cs = corners(n);
  for (const c of cs) {
    const [x, y, z] = c.map((i) => labels[i]);
    if (x === y || y === z || x === z) return null;
    meet[x][y] = meet[y][x] = meet[x][z] = meet[z][x] = meet[y][z] = meet[z][y] = true;
  }
  const opposite = [];
  for (let a = 0; a < 6; a++) {
    const never = [0, 1, 2, 3, 4, 5].filter((b) => b !== a && !meet[a][b]);
    if (never.length !== 1) return null;
    opposite[a] = never[0];
  }
  const model = new CubeModel(n);
  const name = new Array(6).fill(-1);
  for (const i of cs[0]) {
    const face = model.place(i).face;
    name[labels[i]] = face;
    name[opposite[labels[i]]] = (face + 3) % 6;
  }
  return name.includes(-1) || new Set(name).size !== 6 ? null : name;
}

/**
 * Sorts the camera's samples into the six colours of a cube.
 *
 * @param {number[][]} samples 6*n*n colours as [r, g, b], in facelet order
 * @param {number} n the size of the cube
 * @returns facelets (0 to 5 for URFDLB, per sample), `doubt` (0 to 1 per
 *   sample: how nearly it belonged to another colour), `second` (that other
 *   colour, as a face), `palette` (the page colour to show each face's
 *   stickers in) and `seen` (the mean colour the camera saw for each face's
 *   stickers).
 */
export function classify(samples, n) {
  const sorted = sortColours(samples, n);
  return named(n, sorted, sorted.labels);
}

/**
 * Names the groups of `sortColours` by the faces they belong on, for
 * samples laid out as a cube: `labels` are the groups in facelet order
 * (the same groups, moved to where an arrangement of the pictures puts
 * them), `doubt` and `second` likewise if given.
 */
export function named(n, sorted, labels, doubt = sorted.doubt, second = sorted.second) {
  const nn = n * n;
  const groupColour = sorted.colours;
  // A dark neutral group can be the black side of a cube. Compare its
  // lightness to the other colours so dim lighting alone keeps white white.
  const lab = groupColour.map(toLab);
  const black = lab.map(([L, a, b], i) => {
    if (L > 35 || Math.hypot(a, b) > 18) return false;
    const other = lab.filter((_, k) => k !== i).map(([light]) => light).sort((p, q) => p - q);
    return L <= other[2] - 8;
  });
  // The page colour nearest each group, each used once.
  const reference = STICKERS.map((hex) => feature(hexToRgb(hex)));
  const blackReference = feature(hexToRgb(BLACK_STICKER));
  const look = bestMatch(lab.map((c, i) => {
    const measured = labFeature(c);
    return reference.map((r, k) => distance(measured, k === 0 && black[i] ? blackReference : r));
  }));
  // Red and orange are told apart by each other, not by the page's two: of
  // the two groups taken for them, the orange is the one nearer yellow in
  // hue and the stronger. (A camera can show a cube's orange as a bright
  // red and its red as a dull one, each then nearer the page's other colour.)
  const [red, orange] = [look.indexOf(1), look.indexOf(4)];
  const proper = (k) => {
    const m = split(groupColour[k]);
    const hue = unit(m.c);
    return { yellow: hue.reduce((t, v, d) => t + v * STANDARD[3].hue[d], 0), strong: Math.log(m.y + NOISE) };
  };
  const [r, o] = [proper(red), proper(orange)];
  if ((o.yellow - r.yellow) / 0.05 + (o.strong - r.strong) / STRENGTH < 0) [look[red], look[orange]] = [4, 1];
  // Odd: a group is named by the face its middle sticker is on. Even: ask a
  // corner, and if the corners make no sense yet, go by what the colours look like.
  let name;
  if (n % 2 === 1) {
    const mid = Math.floor(nn / 2);
    name = new Array(6).fill(-1);
    for (let f = 0; f < 6; f++) name[labels[f * nn + mid]] = f;
    if (name.includes(-1)) name = look;
  } else {
    name = nameByCorner(n, labels) || look;
  }
  const facelets = Uint8Array.from(labels, (k) => name[k]);
  const palette = new Array(6);
  const seen = new Array(6);
  for (let k = 0; k < 6; k++) {
    palette[name[k]] = look[k] === 0 && black[k] ? BLACK_STICKER : STICKERS[look[k]];
    seen[name[k]] = groupColour[k].map(Math.round);
  }
  return { facelets, doubt, second: Int8Array.from(second, (k) => name[k]), palette, seen };
}

// The page's own colours as groups, for a first guess at one sample.
const STANDARD = STICKERS.map((hex) => describe([split(hexToRgb(hex))], null));

/**
 * What one sample looks like by itself, before a whole cube has been seen:
 * the nearest of six colours, and how nearly it was the next nearest (0
 * to 1). The six are the page's own to begin with, or `seen`, what
 * `learn` has made of them. Only for showing while the camera is held up:
 * the cube is sorted afterwards, all its stickers together, without this.
 */
export function guess(rgb, seen = STANDARD) {
  const far = away(split(rgb), seen);
  let first = 0;
  let next = -1;
  for (let k = 1; k < 6; k++) if (far[k] < far[first]) first = k;
  for (let k = 0; k < 6; k++) if (k !== first && (next < 0 || far[k] < far[next])) next = k;
  return { colour: first, doubt: Math.max(0, Math.min(1, far[first] / (far[next] + 1e-6))) };
}

// How far a sample is from each of six colours. One of the page's own has
// no picture to be bright or dark in, so how strong the sample's colour is
// does not count against it; one made over from this cube's stickers does
// know how strong it is here, and that is what tells an orange from a red
// of nearly the same hue.
function away(m, seen) {
  return seen.map((g) => {
    if (g.known) return apart(m, g);
    return apart(m, g.hue ? { ...g, usual: Math.max(0, m.c[0] * g.hue[0] + m.c[1] * g.hue[1] + m.c[2] * g.hue[2]) } : { ...g, usual: m.length });
  });
}

// Whether the samples taken for one colour are two colours of one hue, a
// weaker and a stronger: they are when, put in order of strength, they
// fall into two runs with a clear step between. `whiter` says how much
// more white for its colour the weaker run has than the stronger.
const STEP = 0.17; // as a logarithm: a step of about a fifth
function twoColours(members, hue) {
  if (members.length < 2 || !hue) return null;
  const along = (m) => Math.max(0, m.c[0] * hue[0] + m.c[1] * hue[1] + m.c[2] * hue[2]);
  const order = members.map((m) => ({ m, s: Math.log(along(m) + NOISE), white: m.x / (along(m) + NOISE) })).sort((p, q) => p.s - q.s);
  let at = 0;
  for (let i = 1; i < order.length; i++) if (order[i].s - order[i - 1].s > order[at + 1].s - order[at].s) at = i - 1;
  if (order[at + 1].s - order[at].s < STEP) return null;
  const [weak, strong] = [order.slice(0, at + 1), order.slice(at + 1)];
  const purest = (part) => part.map((p) => p.white).sort((p, q) => p - q)[Math.floor(part.length / 4)];
  return { weak: weak.map((p) => p.m), strong: strong.map((p) => p.m), whiter: purest(weak) / purest(strong) };
}

/**
 * The six colours as this camera sees them on this cube, for `guess`:
 * the page's own, each made over from the samples (stickers already
 * captured, or in the picture now) that are nearest it. The camera's white
 * balance and the cube's own shades are in the samples; the names stay the
 * page's. Where one of the page's colours has taken the samples of two,
 * its own and those of another that got none, they are parted: a red and
 * an orange that the camera shows as one hue, the orange stronger; or a
 * yellow and a white under a yellow light, the white with far more white
 * in it. (A sheen makes a sample weaker and whiter too, but not by so much.)
 */
export function learn(samples) {
  const parts = samples.map(split);
  const nearest = (m, seen) => away(m, seen).reduce((best, d, k, all) => (d < all[best] ? k : best), 0);
  const pale = STANDARD.findIndex((g) => !g.hue);
  const alike = (j, k) => STANDARD[j].hue.reduce((t, v, d) => t + v * STANDARD[k].hue[d], 0);
  let seen = STANDARD;
  for (let round = 0; round < 4; round++) {
    const mine = seen.map(() => []);
    parts.forEach((m) => mine[nearest(m, seen)].push(m));
    for (let k = 0; k < 6; k++) {
      if (k === pale) continue;
      const two = twoColours(mine[k], describe(mine[k], STANDARD[k]).hue);
      if (!two) continue;
      if (two.whiter > 2.5) {
        if (!mine[pale].length) [mine[pale], mine[k]] = [two.weak, two.strong];
        continue;
      }
      if (two.whiter > 1.25) continue;
      // The neighbour in hue that has no samples of its own, if there is one.
      let other = -1;
      for (let j = 0; j < 6; j++) {
        if (j === k || j === pale || mine[j].length || alike(j, k) < 0.8) continue;
        if (other < 0 || alike(j, k) > alike(other, k)) other = j;
      }
      if (other < 0) continue;
      // The stronger run is the stronger of the two colours.
      const [low, high] = STANDARD[k].usual < STANDARD[other].usual ? [k, other] : [other, k];
      mine[low] = two.weak;
      mine[high] = two.strong;
    }
    seen = seen.map((g, k) => (mine[k].length ? { ...describe(mine[k], g), known: true } : STANDARD[k]));
  }
  return seen;
}

/**
 * The colour of one cell of the camera's picture, as the sticker's own
 * colour and not what lies over it. A glossy sticker throws the light
 * back from part of itself: there the picture shows the sticker's colour
 * with the light's white mixed in, more or less of it, and that part can
 * be most of the cell. The gap between stickers and a finger's shadow are
 * dark. So the darkest pixels are left out, of the rest the third with the
 * most colour in them is kept (the sheen is what has least), and of those
 * the middle value of each of red, green and blue is taken: a corner of the
 * next sticker, or what is left of the sheen, does not pull a middle value
 * the way it pulls a mean. A white sticker has no part with more colour;
 * its middle is white either way.
 *
 * @param {Uint8ClampedArray} data RGBA pixels of the cell
 */
export function cellColour(data) {
  const px = [];
  for (let i = 0; i < data.length; i += 4) {
    const [r, g, b] = [data[i], data[i + 1], data[i + 2]];
    const most = Math.max(r, g, b);
    // How much colour: the spread of the three against the largest, steadied for dark pixels.
    px.push({ r, g, b, light: r + g + b, colour: (most - Math.min(r, g, b)) / (most + 24) });
  }
  if (!px.length) return [0, 0, 0];
  px.sort((p, q) => p.light - q.light);
  const lit = px.slice(Math.floor(px.length * 0.18));
  lit.sort((p, q) => q.colour - p.colour);
  const kept = lit.slice(0, Math.max(1, Math.ceil(lit.length * 0.36)));
  const middle = (key) => {
    const values = kept.map((p) => p[key]).sort((x, y) => x - y);
    const at = (values.length - 1) / 2;
    return (values[Math.floor(at)] + values[Math.ceil(at)]) / 2;
  };
  return [middle("r"), middle("g"), middle("b")];
}
