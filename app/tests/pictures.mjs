// Made-up pictures of a cube's face for the tests of finding one: the face
// drawn through a perspective onto a background, with noise, uneven light
// and clutter, where the truth is known.

import { invertHomography, solveHomography, warpPoint } from "../js/detect.js";

export const COLOURS = [
  [244, 244, 239],
  [217, 43, 58],
  [31, 170, 89],
  [247, 208, 56],
  [245, 130, 32],
  [47, 111, 222],
];
const BODY = [18, 19, 22];
export const UNIT = [
  [0, 0],
  [1, 0],
  [1, 1],
  [0, 1],
];

export function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

// A square of `side` pixels round (cx, cy), its top narrowed by `lean` and
// its right side shortened by `skew` (as a face tipped away from the
// camera looks), then turned by `tilt` degrees: the four corners of the
// face's stickers, top left first, clockwise.
export function quad(cx, cy, side, tilt = 0, lean = 0, skew = 0) {
  const h = side / 2;
  const base = [
    [-h * (1 - lean), -h],
    [h * (1 - lean), -h * (1 - skew)],
    [h, h * (1 - skew)],
    [-h, h],
  ];
  const [c, s] = [Math.cos((tilt * Math.PI) / 180), Math.sin((tilt * Math.PI) / 180)];
  return base.map(([x, y]) => [cx + x * c - y * s, cy + x * s + y * c]);
}

/**
 * A picture of a face. `stickers` are indices into COLOURS, row by row.
 * `fill` is the share of a cell its sticker takes, `margin` the body round
 * the stickers in cells, `light(x, y)` what the light does at a pixel,
 * `body` the colour of the cube itself, `others` further faces of the
 * cube seen beside this one (`{ corners, stickers }`, drawn under it).
 */
export function picture({ W = 320, H = 240, n, corners, stickers, background = [96, 104, 112], clutter = [], fill = 0.84, margin = 0.12, noise = 4, light = () => 1, seed = 1, body = BODY, others = [] }) {
  const random = rng(seed);
  const gauss = () => {
    let u = 0;
    for (let k = 0; k < 4; k++) u += random();
    return (u - 2) * 1.73; // about a unit normal
  };
  const data = new Uint8ClampedArray(W * H * 4);
  const m = margin / n;
  const faces = (corners ? [{ corners, stickers }, ...others] : []).map((f) => {
    // The box the face and the body round it lie in: outside it there is nothing to work out.
    const xs = f.corners.map((c) => c[0]);
    const ys = f.corners.map((c) => c[1]);
    const pad = m * Math.max(Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys)) * 1.6 + 2;
    return { ...f, back: invertHomography(solveHomography(UNIT, f.corners)), x0: Math.min(...xs) - pad, x1: Math.max(...xs) + pad, y0: Math.min(...ys) - pad, y1: Math.max(...ys) + pad };
  });
  // What is behind the cube, painted once: the rectangles one over another.
  let behind = null;
  if (clutter.length) {
    behind = new Float32Array(W * H * 3);
    for (let i = 0; i < W * H; i++) behind.set(background, i * 3);
    for (const q of clutter) {
      for (let y = Math.max(0, Math.ceil(q.y)); y < Math.min(H, q.y + q.h); y++) {
        for (let x = Math.max(0, Math.ceil(q.x)); x < Math.min(W, q.x + q.w); x++) behind.set(q.colour, (y * W + x) * 3);
      }
    }
  }
  const colourAt = (x, y) => {
    for (const face of faces) {
      if (x < face.x0 || x > face.x1 || y < face.y0 || y > face.y1) continue;
      const [u, v] = warpPoint(face.back, x, y);
      if (u >= -m && v >= -m && u <= 1 + m && v <= 1 + m) {
        if (u < 0 || v < 0 || u >= 1 || v >= 1) return body;
        const [fu, fv] = [u * n, v * n];
        const [c, r] = [Math.floor(fu), Math.floor(fv)];
        if (Math.abs(fu - c - 0.5) <= fill / 2 && Math.abs(fv - r - 0.5) <= fill / 2) return COLOURS[face.stickers[r * n + c]];
        return body;
      }
    }
    if (!behind) return background;
    const at = (Math.max(0, Math.min(H - 1, Math.round(y))) * W + Math.max(0, Math.min(W - 1, Math.round(x)))) * 3;
    return [behind[at], behind[at + 1], behind[at + 2]];
  };
  const SUB = [
    [0.25, 0.25],
    [0.75, 0.25],
    [0.25, 0.75],
    [0.75, 0.75],
  ];
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const sum = [0, 0, 0];
      for (const [dx, dy] of SUB) {
        const c = colourAt(x + dx - 0.5, y + dy - 0.5);
        for (let d = 0; d < 3; d++) sum[d] += c[d];
      }
      const l = light(x, y);
      const i = (y * W + x) * 4;
      for (let d = 0; d < 3; d++) data[i + d] = (sum[d] / 4) * l + gauss() * noise;
      data[i + 3] = 255;
    }
  }
  return { width: W, height: H, data };
}

export function randomStickers(n, random) {
  return Array.from({ length: n * n }, () => Math.floor(random() * 6));
}

export function randomClutter(W, H, random, count = 60) {
  return Array.from({ length: count }, () => {
    const w = 6 + random() * 60;
    return { x: random() * W - 10, y: random() * H - 10, w, h: random() < 0.4 ? w * (0.8 + random() * 0.4) : 6 + random() * 60, colour: [random() * 255, random() * 255, random() * 255] };
  });
}

// How far the corners found are from the truth, as a share of the face's side.
export function cornerError(found, truth) {
  const side = (Math.hypot(truth[1][0] - truth[0][0], truth[1][1] - truth[0][1]) + Math.hypot(truth[3][0] - truth[0][0], truth[3][1] - truth[0][1])) / 2;
  return Math.max(...found.map(([x, y], k) => Math.hypot(x - truth[k][0], y - truth[k][1]))) / side;
}

// How many cells were read as another of the six colours than was drawn, the known light taken out.
export function wrongCells(cells, n, corners, stickers, light) {
  const h = solveHomography(UNIT, corners);
  let wrong = 0;
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      const [x, y] = warpPoint(h, (c + 0.5) / n, (r + 0.5) / n);
      const l = light(x, y);
      const seen = cells[r * n + c].map((v) => v / l);
      const far = (q) => Math.hypot(seen[0] - q[0], seen[1] - q[1], seen[2] - q[2]);
      let best = 0;
      for (let k = 1; k < 6; k++) if (far(COLOURS[k]) < far(COLOURS[best])) best = k;
      if (best !== stickers[r * n + c]) wrong++;
    }
  }
  return wrong;
}

// The made-up faces: where (in a picture of 320 by 240), how large (as a
// share of the picture's height), how held, in what light.
export const even = () => 1;
export const CASES = [
  { name: "centred", at: [160, 120], side: 0.5 },
  { name: "off-centre", at: [96, 84], side: 0.42 },
  { name: "in a corner", at: [236, 164], side: 0.4 },
  { name: "small", at: [150, 110], side: 0.25 },
  { name: "large", at: [160, 120], side: 0.9 },
  { name: "tilted 10", at: [170, 118], side: 0.5, tilt: 10 },
  { name: "tilted -10", at: [150, 122], side: 0.5, tilt: -10 },
  { name: "tilted 25", at: [160, 120], side: 0.5, tilt: 25 },
  { name: "tilted -25", at: [160, 120], side: 0.45, tilt: -25 },
  { name: "leaning back", at: [160, 124], side: 0.55, lean: 0.14 },
  { name: "leaning and tilted", at: [150, 120], side: 0.55, lean: 0.1, skew: 0.08, tilt: 12 },
  { name: "on clutter", at: [165, 115], side: 0.5, clutter: true },
  { name: "on clutter, tilted", at: [140, 125], side: 0.45, tilt: -15, clutter: true },
  { name: "white stickers on a light background", at: [160, 120], side: 0.5, background: [228, 226, 220], white: true },
  { name: "light from one side", at: [160, 120], side: 0.55, light: (x) => 0.55 + (0.45 * x) / 320 },
  { name: "a shadow across it", at: [160, 120], side: 0.55, light: (x, y) => 0.62 + 0.38 / (1 + Math.exp(-(x + 0.6 * y - 230) / 14)) },
  { name: "dim", at: [160, 120], side: 0.5, light: () => 0.35, background: [70, 70, 76] },
  { name: "noisy", at: [160, 120], side: 0.5, noise: 9 },
  { name: "thin gaps", at: [160, 120], side: 0.6, fill: 0.93, margin: 0.05 },
  { name: "wide gaps", at: [160, 120], side: 0.55, fill: 0.72 },
];

export function build(n, c, seed, W = 320, H = 240) {
  const random = rng(seed * 7919 + n * 131);
  const k = W / 320;
  const corners = quad(c.at[0] * k, c.at[1] * k, c.side * H, c.tilt ?? 0, c.lean ?? 0, c.skew ?? 0);
  let stickers = randomStickers(n, random);
  if (c.white) stickers = stickers.map((s, i) => (i % 2 ? s : 0));
  const light = c.light ? (x, y) => c.light(x / k, y / k) : even;
  const image = picture({
    W,
    H,
    n,
    corners,
    stickers,
    background: c.background,
    clutter: c.clutter ? randomClutter(W, H, random) : [],
    fill: c.fill,
    margin: c.margin,
    noise: c.noise,
    light,
    seed: seed + 17,
  });
  return { image, corners, stickers, light };
}

// Clutter and no cube.
export const clutterOnly = (n, seed, W = 320, H = 240) =>
  picture({ W, H, n, corners: null, stickers: [], clutter: randomClutter(W, H, rng(seed * 31 + n), Math.round((70 * W * H) / 76800)), seed });

// A face held any way within what is promised, for counting how often it is found.
export function randomPose(n, seed, W = 320, H = 240) {
  const random = rng(seed * 104729 + n * 17);
  const tilt = (random() * 2 - 1) * 25;
  const turned = Math.abs(Math.cos((tilt * Math.PI) / 180)) + Math.abs(Math.sin((tilt * Math.PI) / 180));
  // As large as fits in the picture when it is turned.
  const side = Math.min((0.25 + random() * 0.65) * H, (H - 14) / turned);
  const reach = (side / 2) * turned + 6;
  const cx = reach + random() * Math.max(0, W - 2 * reach);
  const cy = reach + random() * Math.max(0, H - 2 * reach);
  const corners = quad(cx, cy, side, tilt, random() * 0.14, random() * 0.1);
  const stickers = randomStickers(n, random);
  const [g0, gx, gy] = [0.5 + random() * 0.5, (random() - 0.5) * 0.5, (random() - 0.5) * 0.5];
  const light = (x, y) => Math.max(0.3, Math.min(1.05, g0 + (gx * x) / W + (gy * y) / H));
  const cluttered = random() < 0.5;
  const image = picture({
    W,
    H,
    n,
    corners,
    stickers,
    clutter: cluttered ? randomClutter(W, H, random) : [],
    fill: 0.74 + random() * 0.18,
    margin: 0.04 + random() * 0.12,
    noise: 3 + random() * 5,
    light,
    seed,
  });
  return {
    image,
    what: `side ${Math.round(side)} tilt ${Math.round(tilt)}${cluttered ? " clutter" : ""}`,
    error: (face) => cornerError(face.corners, corners),
    wrong: (face) => wrongCells(face.cells, n, corners, stickers, light),
  };
}

// A cube as it is held, and cubes of other makes: each returns a picture
// and its truth, as `build` does. The front face with the top face seen
// squashed above it and the right face beside it; a cube whose stickers
// are parted by seams of a pixel or less; one with a white body; one seen
// from the side, its far edge a quarter shorter than its near one.
export const HARDER = {
  "the top and right faces in view": (n, seed) => {
    const random = rng(seed * 13 + n);
    const side = 90 + random() * 40;
    const corners = quad(150 + random() * 20, 135 + random() * 10, side, (random() - 0.5) * 16);
    const [tl, tr, br] = corners;
    const up = [(tr[1] - tl[1]) * 0.45 + side * 0.18, -(tr[0] - tl[0]) * 0.45];
    const others = [
      { corners: [[tl[0] + up[0], tl[1] + up[1]], [tr[0] + up[0], tr[1] + up[1]], tr, tl], stickers: randomStickers(n, random) },
      { corners: [tr, [tr[0] + up[0], tr[1] + up[1]], [br[0] + up[0], br[1] + up[1]], br], stickers: randomStickers(n, random) },
    ];
    const stickers = randomStickers(n, random);
    return { image: picture({ n, corners, stickers, others, margin: 0.03, seed }), corners, stickers, light: even };
  },
  "seams of a pixel or less": (n, seed) => {
    const random = rng(seed * 19 + n);
    const corners = quad(160, 120, 120, (random() - 0.5) * 20);
    const stickers = randomStickers(n, random);
    return { image: picture({ n, corners, stickers, fill: 1 - (0.9 * n) / 120, margin: 0.04, seed }), corners, stickers, light: even };
  },
  "a white body": (n, seed) => {
    const random = rng(seed * 23 + n);
    const corners = quad(160, 120, 120, (random() - 0.5) * 20);
    // No white stickers on a white cube.
    const stickers = randomStickers(n, random).map((s) => (s === 0 ? 1 + Math.floor(random() * 5) : s));
    return { image: picture({ n, corners, stickers, body: [236, 236, 232], seed }), corners, stickers, light: even };
  },
  "seen from the side": (n, seed) => {
    const random = rng(seed * 29 + n);
    const corners = quad(160, 120, 130, (random() - 0.5) * 10, 0, 0.25);
    const stickers = randomStickers(n, random);
    return { image: picture({ n, corners, stickers, seed }), corners, stickers, light: even };
  },
};

// And what is not promised: no gaps at all between the stickers, and a face turned by 40 degrees.
export const BEYOND = {
  "no gaps between the stickers": (n, seed) => {
    const random = rng(seed * 17 + n);
    const corners = quad(160, 120, 120, (random() - 0.5) * 20);
    const stickers = randomStickers(n, random);
    return { image: picture({ n, corners, stickers, fill: 1, margin: 0.04, seed }), corners, stickers, light: even };
  },
  "turned by 40 degrees": (n, seed) => {
    const random = rng(seed * 31 + n);
    const corners = quad(160, 120, 110, 40);
    const stickers = randomStickers(n, random);
    return { image: picture({ n, corners, stickers, seed }), corners, stickers, light: even };
  },
};
