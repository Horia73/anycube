// The cube drawn in three dimensions on a 2D canvas, for any size, with one
// run of layers part of the way through a turn, and the cube as something
// to touch: a layer dragged with the mouse or a finger turns, a sticker
// tapped can be painted, a drag beside the cube turns the whole cube round.
//
// A turning cube is up to three boxes (the layers that turn and what is on
// either side of them) that touch along planes. Each box is convex, so its
// faces that look towards the eye never hide each other, and a box further
// from the eye, measured along the turn's axis, can never hide a nearer
// one: drawing them in that order needs no depth buffer and gives exact
// pictures.

import { FACES } from "./cube-model.js";

const EYE = 7.2; // distance of the eye from the middle of the cube, whose faces are at 1
const BLANK = 255; // a sticker not known yet

const mul = (a, b) => {
  const out = new Array(9);
  for (let r = 0; r < 3; r++)
    for (let c = 0; c < 3; c++) out[r * 3 + c] = a[r * 3] * b[c] + a[r * 3 + 1] * b[3 + c] + a[r * 3 + 2] * b[6 + c];
  return out;
};
const apply = (m, v) => [
  m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
  m[3] * v[0] + m[4] * v[1] + m[5] * v[2],
  m[6] * v[0] + m[7] * v[1] + m[8] * v[2],
];
const transposed = (m) => [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const rotX = (a) => [1, 0, 0, 0, Math.cos(a), -Math.sin(a), 0, Math.sin(a), Math.cos(a)];
const rotY = (a) => [Math.cos(a), 0, Math.sin(a), 0, 1, 0, -Math.sin(a), 0, Math.cos(a)];
const IDENTITY = [1, 0, 0, 0, 1, 0, 0, 0, 1];

// A rotation by `angle` about the unit vector `u`, counter-clockwise seen from its tip.
function about(u, angle) {
  const [x, y, z] = u;
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  const t = 1 - c;
  return [t * x * x + c, t * x * y - s * z, t * x * z + s * y, t * x * y + s * z, t * y * y + c, t * y * z - s * x, t * x * z - s * y, t * y * z + s * x, t * z * z + c];
}

function toQuat(m) {
  const tr = m[0] + m[4] + m[8];
  if (tr > 0) {
    const s = Math.sqrt(tr + 1) * 2;
    return [(m[7] - m[5]) / s, (m[2] - m[6]) / s, (m[3] - m[1]) / s, s / 4];
  }
  if (m[0] > m[4] && m[0] > m[8]) {
    const s = Math.sqrt(1 + m[0] - m[4] - m[8]) * 2;
    return [s / 4, (m[1] + m[3]) / s, (m[2] + m[6]) / s, (m[7] - m[5]) / s];
  }
  if (m[4] > m[8]) {
    const s = Math.sqrt(1 + m[4] - m[0] - m[8]) * 2;
    return [(m[1] + m[3]) / s, s / 4, (m[5] + m[7]) / s, (m[2] - m[6]) / s];
  }
  const s = Math.sqrt(1 + m[8] - m[0] - m[4]) * 2;
  return [(m[2] + m[6]) / s, (m[5] + m[7]) / s, s / 4, (m[3] - m[1]) / s];
}

function fromQuat([x, y, z, w]) {
  return [
    1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w),
    2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w),
    2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y),
  ];
}

function slerp(a, b, t) {
  let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
  if (d < 0) {
    b = b.map((v) => -v);
    d = -d;
  }
  if (d > 0.9995) {
    const q = a.map((v, i) => v + (b[i] - v) * t);
    const len = Math.hypot(...q);
    return q.map((v) => v / len);
  }
  const th = Math.acos(d);
  const sa = Math.sin((1 - t) * th) / Math.sin(th);
  const sb = Math.sin(t * th) / Math.sin(th);
  return a.map((v, i) => v * sa + b[i] * sb);
}

// The view that shows U, F and R, as cubes are drawn everywhere.
const HOME = mul(rotX(0.47), rotY(-0.62));

// The turn that brings each face to the front, as a person would turn the
// cube to look at it: the side faces by turning it round, U and D by tipping it.
const TO_FRONT = [rotX(Math.PI / 2), rotY(-Math.PI / 2), IDENTITY, rotX(-Math.PI / 2), rotY(Math.PI / 2), rotY(Math.PI)];

const reducedMotion = () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

// The next frame, or a fifth of a second, whichever comes first: a page
// that is not on screen gets few frames or none, and a turn that has begun
// must still end.
export function nextFrame(callback) {
  let done = false;
  let raf = 0;
  let timer = 0;
  const run = () => {
    if (done) return;
    done = true;
    cancelAnimationFrame(raf);
    clearTimeout(timer);
    callback(performance.now());
  };
  raf = requestAnimationFrame(run);
  timer = setTimeout(run, 200);
}

export class CubeView {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.n = 3;
    this.facelets = new Uint8Array(54);
    this.palette = ["#fff", "#f00", "#0f0", "#ff0", "#f80", "#00f"];
    this.body = "#101216";
    this.inside = "#1c1f26";
    this.blank = "#3b404c";
    this.raw = null; // a colour per facelet that overrides the palette where set
    this.marks = null; // facelets to ring: ones to look at again
    this.rot = HOME;
    // Layers `from` up to `to` under `face`, turned by `angle` radians clockwise seen from the face.
    this.turn = null;
    // The move that comes next, shown as arrows: { face, from, to, quarters }.
    this.cue = null;
    this.cueSince = 0; // when these arrows were put up
    this.calm = window.matchMedia?.("(prefers-reduced-motion: reduce)") ?? { matches: false };
    this.pending = false;
    // While true nothing is drawn: the page holds the picture back until
    // it has the cube to show, so that no other cube is seen first.
    this.waiting = false;
    this.glide = null;
    // What a touch on the cube does: "turn" a layer, "paint" a sticker, or only "look".
    this.mode = "turn";
    this.onturn = () => {}; // (face, layer, quarters) once a dragged layer has settled
    this.onpaint = () => {}; // (facelet, whether it was touched first or dragged over)
    this.busy = () => false; // whether something else is moving the cube just now
    this.#watch();
  }

  // ------------------------------------------------------------ geometry

  #frame() {
    const size = this.canvas.width;
    return { size, half: size / 2, scale: size * 0.195 };
  }

  #project(v, f) {
    const k = (EYE / (EYE - v[2])) * f.scale;
    return [f.half + v[0] * k, f.half * 0.96 - v[1] * k];
  }

  // The sticker under a point of the canvas (in CSS pixels), if any: its
  // facelet, its face, and where on the face (each from -1 to 1).
  pick(x, y) {
    const f = this.#frame();
    const ratio = f.size / this.canvas.clientWidth;
    // The ray from the eye through the point, in the cube's own frame.
    const back = transposed(this.rot);
    const eye = apply(back, [0, 0, EYE]);
    const through = apply(back, [(x * ratio - f.half) / f.scale, (f.half * 0.96 - y * ratio) / f.scale, 0]);
    const dir = [through[0] - eye[0], through[1] - eye[1], through[2] - eye[2]];
    let best = null;
    for (let face = 0; face < 6; face++) {
      const { normal, right, down } = FACES[face];
      const k = normal.findIndex((v) => v !== 0);
      if (dir[k] * normal[k] >= 0) continue; // seen from behind
      const t = (normal[k] - eye[k]) / dir[k];
      const p = [eye[0] + dir[0] * t, eye[1] + dir[1] * t, eye[2] + dir[2] * t];
      const a = p[0] * right[0] + p[1] * right[1] + p[2] * right[2];
      const b = p[0] * down[0] + p[1] * down[1] + p[2] * down[2];
      if (Math.abs(a) > 1 || Math.abs(b) > 1) continue;
      if (!best || t < best.t) best = { t, face, a, b, point: p };
    }
    if (!best) return null;
    const n = this.n;
    const col = Math.min(n - 1, Math.floor(((best.a + 1) / 2) * n));
    const row = Math.min(n - 1, Math.floor(((best.b + 1) / 2) * n));
    return { ...best, index: best.face * n * n + row * n + col };
  }

  // -------------------------------------------------------------- touch

  #watch() {
    const canvas = this.canvas;
    new ResizeObserver(() => this.draw()).observe(canvas);
    let drag = null;
    const at = (e) => {
      const box = canvas.getBoundingClientRect();
      return [e.clientX - box.left, e.clientY - box.top];
    };
    canvas.addEventListener("pointerdown", (e) => {
      if (e.button > 0) return;
      const [x, y] = at(e);
      const hit = this.mode === "look" || this.busy() ? null : this.pick(x, y);
      canvas.setPointerCapture(e.pointerId);
      this.glide = null;
      if (hit && this.mode === "paint") {
        drag = { kind: "paint", last: hit.index };
        this.onpaint(hit.index, true);
      } else if (hit && this.mode === "turn") {
        drag = { kind: "layer", hit, x, y, slab: null };
      } else {
        drag = { kind: "look", x: e.clientX, y: e.clientY };
      }
    });
    canvas.addEventListener("pointermove", (e) => {
      if (!drag) return;
      const [x, y] = at(e);
      if (drag.kind === "paint") {
        const hit = this.pick(x, y);
        if (hit && hit.index !== drag.last) {
          drag.last = hit.index;
          this.onpaint(hit.index, false);
        }
      } else if (drag.kind === "layer") {
        this.#dragLayer(drag, x - drag.x, y - drag.y);
      } else {
        const k = 3.2 / canvas.clientWidth;
        this.rot = mul(mul(rotX((e.clientY - drag.y) * k), rotY((e.clientX - drag.x) * k)), this.rot);
        [drag.x, drag.y] = [e.clientX, e.clientY];
        this.draw();
      }
    });
    const up = () => {
      if (drag?.kind === "layer" && drag.slab) this.#settle(drag.slab);
      drag = null;
    };
    canvas.addEventListener("pointerup", up);
    canvas.addEventListener("pointercancel", up);
    canvas.addEventListener("dblclick", (e) => {
      const [x, y] = at(e);
      if (!this.pick(x, y) || this.mode === "look") this.look(null);
    });
    canvas.addEventListener("keydown", (e) => {
      const step = { ArrowLeft: rotY(-0.2), ArrowRight: rotY(0.2), ArrowUp: rotX(-0.2), ArrowDown: rotX(0.2) }[e.key];
      if (!step || e.altKey || e.metaKey || e.ctrlKey) return;
      this.rot = mul(step, this.rot);
      this.draw();
      e.preventDefault();
    });
  }

  // A drag that began on a sticker: once it has a direction, the layer the
  // sticker is in turns with it, about the axis that carries the sticker
  // the way the finger goes.
  #dragLayer(drag, dx, dy) {
    const f = this.#frame();
    const ratio = f.size / this.canvas.clientWidth;
    if (!drag.slab) {
      if (Math.hypot(dx, dy) < 7) return;
      const { face, point } = drag.hit;
      const { normal, right, down } = FACES[face];
      // How each of the face's two directions looks on the screen at this sticker.
      const base = this.#project(apply(this.rot, point), f);
      const seen = (dir) => {
        const p = this.#project(apply(this.rot, [point[0] + dir[0] * 0.2, point[1] + dir[1] * 0.2, point[2] + dir[2] * 0.2]), f);
        return [(p[0] - base[0]) / 0.2 / ratio, (p[1] - base[1]) / 0.2 / ratio];
      };
      const [er, ed] = [seen(right), seen(down)];
      const along = (v) => Math.abs(dx * v[0] + dy * v[1]) / Math.hypot(v[0], v[1]);
      const [t, e] = along(er) >= along(ed) ? [right, er] : [down, ed];
      // A turn about `axis` (counter-clockwise seen from its tip) carries this sticker along `t`.
      const axis = cross(normal, t);
      const k = axis.findIndex((v) => v !== 0);
      const turnFace = FACES.findIndex((g) => g.normal[k] === axis[k]);
      const layer = Math.max(0, Math.min(this.n - 1, Math.floor(((1 - point[k] * axis[k]) / 2) * this.n)));
      drag.slab = { face: turnFace, from: layer, to: layer + 1, e };
    }
    const { e } = drag.slab;
    // The sticker is one unit from the axis: distance along `t` is the angle.
    // Counter-clockwise about the axis is the negative sense for its face.
    const angle = (-(dx * e[0] + dy * e[1]) / (e[0] * e[0] + e[1] * e[1])) * 1.25;
    drag.slab.angle = Math.max(-Math.PI, Math.min(Math.PI, angle));
    this.setTurn({ face: drag.slab.face, from: drag.slab.from, to: drag.slab.to, angle: drag.slab.angle });
  }

  // A layer let go: it runs on to the nearest quarter turn and is reported.
  #settle({ face, from, to, angle }) {
    const quarters = Math.round(angle / (Math.PI / 2));
    this.animateTurn({ face, from, to }, angle, (quarters * Math.PI) / 2, 140).then(() => {
      this.setTurn(null);
      if (quarters !== 0) this.onturn(face, from, Math.abs(quarters) === 2 ? 2 : quarters);
    });
  }

  /** Turns layers from one angle to another over `ms`; resolves when done. */
  animateTurn(slab, fromAngle, toAngle, ms) {
    return new Promise((resolve) => {
      if (reducedMotion() || ms <= 0) {
        this.setTurn({ ...slab, angle: toAngle });
        resolve();
        return;
      }
      const start = performance.now();
      const frame = (now) => {
        const t = Math.min(1, (now - start) / ms);
        const eased = 1 - (1 - t) ** 2;
        this.turn = { ...slab, angle: fromAngle + (toAngle - fromAngle) * eased };
        this.#paint();
        if (t < 1) nextFrame(frame);
        else resolve();
      };
      nextFrame(frame);
    });
  }

  // --------------------------------------------------------------- state

  setCube(n, facelets) {
    this.n = n;
    this.facelets = facelets;
    this.turn = null;
    this.draw();
  }

  setFacelets(facelets) {
    this.facelets = facelets;
    this.draw();
  }

  // Colours given per facelet (CSS colours, or null for "as the palette
  // says"): what the camera saw, before it is sorted into six colours.
  setRaw(raw) {
    this.raw = raw;
    this.draw();
  }

  // Facelets to ring (a number above 0.55 each), or null.
  setMarks(marks) {
    this.marks = marks;
    this.draw();
  }

  setTurn(turn) {
    this.turn = turn;
    this.draw();
  }

  setCue(cue) {
    const was = this.cue;
    const same = cue && was && cue.face === was.face && cue.from === was.from && cue.to === was.to && cue.quarters === was.quarters;
    // Arrows for another move come in over a moment, not all at once.
    if (cue && !same) this.cueSince = performance.now();
    this.cue = cue;
    this.draw();
  }

  setColours({ palette, body, inside, blank }) {
    if (palette) this.palette = palette;
    if (body) this.body = body;
    if (inside) this.inside = inside;
    if (blank) this.blank = blank;
    this.draw();
  }

  // Turns the picture as the layers of `face` would turn by `quarters`,
  // without touching the stickers: for the middle layer of an odd cube,
  // which is the whole cube turned one way and its two halves the other.
  spin(face, quarters) {
    this.rot = mul(this.rot, about(FACES[face].normal, (-quarters * Math.PI) / 2));
    this.draw();
  }

  // Turns the cube to show a face (or back to the usual view, with `null`).
  look(face, ms = 450) {
    const tilt = mul(rotX(0.3), rotY(-0.34));
    const target = face === null ? HOME : mul(tilt, TO_FRONT[face]);
    if (reducedMotion() || ms <= 0) {
      this.rot = target;
      this.glide = null;
      this.draw();
      return;
    }
    const glide = { from: toQuat(this.rot), to: toQuat(target), start: performance.now(), ms };
    this.glide = glide;
    const frame = (now) => {
      if (this.glide !== glide) return;
      const t = Math.min(1, (now - glide.start) / glide.ms);
      this.rot = fromQuat(slerp(glide.from, glide.to, 1 - (1 - t) ** 3));
      this.#paint();
      if (t < 1) nextFrame(frame);
      else this.glide = null;
    };
    nextFrame(frame);
  }

  // Draws on the next frame; many calls in one frame draw once.
  draw() {
    if (this.pending) return;
    this.pending = true;
    nextFrame(() => {
      this.pending = false;
      this.#paint();
    });
  }

  /** Draws now and says how long it took: for measuring. */
  paintNow() {
    const t = performance.now();
    this.#paint();
    this.ctx.getImageData(0, 0, 1, 1);
    return performance.now() - t;
  }

  // ------------------------------------------------------------- drawing

  #paint() {
    if (this.waiting) return;
    const { canvas, ctx, n } = this;
    const ratio = Math.min(window.devicePixelRatio || 1, 3);
    const size = Math.round(canvas.clientWidth * ratio);
    if (size === 0) return;
    if (canvas.width !== size || canvas.height !== size) {
      canvas.width = size;
      canvas.height = size;
    }
    ctx.clearRect(0, 0, size, size);
    const f = this.#frame();
    const half = f.half;

    // A soft shadow on the ground under the cube.
    const shadow = ctx.createRadialGradient(half, size * 0.9, 0, half, size * 0.9, size * 0.36);
    shadow.addColorStop(0, "rgba(0,0,0,0.30)");
    shadow.addColorStop(1, "rgba(0,0,0,0)");
    ctx.save();
    ctx.translate(0, size * 0.9);
    ctx.scale(1, 0.16);
    ctx.translate(0, -size * 0.9);
    ctx.fillStyle = shadow;
    ctx.fillRect(0, 0, size, size * 2);
    ctx.restore();

    // The boxes: the layers that turn, and what lies on either side of them.
    const boxes = [];
    const turn = this.turn;
    if (turn && turn.angle !== 0) {
      const normal = FACES[turn.face].normal;
      const axis = normal.findIndex((v) => v !== 0);
      const sign = normal[axis];
      // The run of layers as a stretch of the axis.
      const [near, far] = [1 - (2 * turn.from) / n, 1 - (2 * turn.to) / n];
      const [lo, hi] = sign > 0 ? [far, near] : [-near, -far];
      // Clockwise seen from outside is the negative sense about the outward normal.
      boxes.push({ axis, lo, hi, m: mul(this.rot, about(normal, -turn.angle)) });
      if (lo > -1 + 1e-9) boxes.push({ axis, lo: -1, hi: lo, m: this.rot });
      if (hi < 1 - 1e-9) boxes.push({ axis, lo: hi, hi: 1, m: this.rot });
      // Where the eye is along the axis, in the cube's own frame.
      const eye = this.rot[6 + axis] * EYE;
      const away = (b) => (eye < b.lo ? b.lo - eye : eye > b.hi ? eye - b.hi : 0);
      boxes.sort((a, b) => away(b) - away(a));
    } else {
      boxes.push({ axis: 0, lo: -1, hi: 1, m: this.rot });
    }

    const cell = 2 / n;
    const gap = cell * (n > 12 ? 0.05 : 0.07);
    const round = n <= 9 ? cell * 0.17 : 0;
    const line = Math.max(1, size / 900);
    // The body is not a sharp box: its edges and corners are rounded, by a
    // measure taken from the stickers so that none of them is cut. A flat
    // face stops that far short of the edge; a round bar along each edge and
    // a ball at each corner make up the rest. Where layers are cut apart
    // for a turn the cut itself stays flat.
    const bevel = n > 9 ? gap : Math.min(0.07, gap * 2.2);
    const scaleAt = (v) => (EYE / (EYE - v[2])) * f.scale;
    for (const box of boxes) {
      const range = [
        [-1, 1],
        [-1, 1],
        [-1, 1],
      ];
      range[box.axis] = [box.lo, box.hi];
      // How far each end of the box is rounded: only where it is an end of the cube.
      const round3 = range.map(([lo, hi]) => [lo === -1 ? bevel : 0, hi === 1 ? bevel : 0]);
      ctx.strokeStyle = this.body;
      ctx.fillStyle = this.body;
      ctx.lineCap = "butt";
      for (let e = 0; e < 3; e++) {
        const [u, v] = [(e + 1) % 3, (e + 2) % 3];
        for (const su of [0, 1]) {
          for (const sv of [0, 1]) {
            if (!round3[u][su] || !round3[v][sv]) continue;
            const p = [0, 0, 0];
            p[u] = range[u][su] + (su ? -bevel : bevel);
            p[v] = range[v][sv] + (sv ? -bevel : bevel);
            const ends = [0, 1].map((se) => {
              p[e] = range[e][se] + (se ? -round3[e][1] : round3[e][0]);
              return apply(box.m, p);
            });
            const [x1, y1] = this.#project(ends[0], f);
            const [x2, y2] = this.#project(ends[1], f);
            ctx.beginPath();
            ctx.moveTo(x1, y1);
            ctx.lineTo(x2, y2);
            ctx.lineWidth = bevel * (scaleAt(ends[0]) + scaleAt(ends[1]));
            ctx.stroke();
            // A ball where three rounded edges meet: once per corner, from the edges along the first axis.
            if (e !== 0) continue;
            for (const se of [0, 1]) {
              if (!round3[e][se]) continue;
              const [cx, cy] = this.#project(ends[se], f);
              ctx.beginPath();
              ctx.arc(cx, cy, bevel * scaleAt(ends[se]), 0, Math.PI * 2);
              ctx.fill();
            }
          }
        }
      }
      for (let face = 0; face < 6; face++) {
        const { normal, right, down } = FACES[face];
        const k = normal.findIndex((v) => v !== 0);
        const level = normal[k] > 0 ? range[k][1] : range[k][0];
        // A point of this face of the box from its place along `right` and `down`, each from -1 to 1.
        const m = box.m;
        const at = (a, b) => {
          const v = [right[0] * a + down[0] * b, right[1] * a + down[1] * b, right[2] * a + down[2] * b];
          v[k] = level;
          return this.#project(apply(m, v), f);
        };
        // How far the face reaches along its two directions (cut short along the turn's axis).
        const span = (dir) => {
          const j = dir.findIndex((v) => v !== 0);
          const [lo, hi] = range[j];
          return dir[j] > 0 ? [lo, hi] : [-hi, -lo];
        };
        const [a0, a1] = span(right);
        const [b0, b1] = span(down);
        const outer = Math.abs(level) === 1;
        // An outer face is flat up to where the rounding of its edges begins.
        const inset = (lo, hi) => [lo === -1 && outer ? lo + bevel : lo, hi === 1 && outer ? hi - bevel : hi];
        const [fa0, fa1] = inset(a0, a1);
        const [fb0, fb1] = inset(b0, b1);
        const corners = [at(fa0, fb0), at(fa1, fb0), at(fa1, fb1), at(fa0, fb1)];
        // Faces that look away from the eye are not drawn.
        let area = 0;
        for (let i = 0; i < 4; i++) {
          const [x1, y1] = corners[i];
          const [x2, y2] = corners[(i + 1) % 4];
          area += x1 * y2 - x2 * y1;
        }
        if (area <= 0) continue;
        // The whole face of the box, its corners rounded where they are corners of the cube.
        const whole = () => {
          ctx.beginPath();
          const q = bevel;
          const turns = [
            [a0, b0, 1, 1],
            [a1, b0, -1, 1],
            [a1, b1, -1, -1],
            [a0, b1, 1, -1],
          ];
          turns.forEach(([a, b, da, db], i) => {
            const round = q > 0 && Math.abs(a) === 1 && Math.abs(b) === 1;
            // Going round the square, each corner is entered along one side and left along the next.
            const along = i % 2 === 0;
            const before = round ? (along ? at(a, b + db * q) : at(a + da * q, b)) : at(a, b);
            const after = round ? (along ? at(a + da * q, b) : at(a, b + db * q)) : null;
            i ? ctx.lineTo(before[0], before[1]) : ctx.moveTo(before[0], before[1]);
            if (after) {
              const corner = at(a, b);
              ctx.quadraticCurveTo(corner[0], corner[1], after[0], after[1]);
            }
          });
          ctx.closePath();
        };
        const outline = () => {
          if (!outer) return whole();
          ctx.beginPath();
          corners.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
          ctx.closePath();
        };
        outline();
        ctx.fillStyle = outer ? this.body : this.inside;
        ctx.strokeStyle = this.body;
        ctx.lineWidth = line;
        ctx.lineJoin = "round";
        ctx.fill();
        ctx.stroke();
        if (!outer) continue;

        // The stickers of this face that are on this box, one path per colour.
        const paths = new Map();
        const ringed = [];
        const base = face * n * n;
        for (let r = 0; r < n; r++) {
          const y0 = -1 + r * cell;
          if (y0 + cell / 2 < b0 || y0 + cell / 2 > b1) continue;
          for (let c = 0; c < n; c++) {
            const x0 = -1 + c * cell;
            if (x0 + cell / 2 < a0 || x0 + cell / 2 > a1) continue;
            const i = base + r * n + c;
            const colour = this.raw?.[i] ?? (this.facelets[i] === BLANK ? this.blank : this.palette[this.facelets[i]]);
            let path = paths.get(colour);
            if (!path) paths.set(colour, (path = new Path2D()));
            const [xa, xb, ya, yb] = [x0 + gap, x0 + cell - gap, y0 + gap, y0 + cell - gap];
            if (round) {
              // Rounded corners: each corner of the square is the control point of a curve.
              const q = round;
              const p = [
                at(xa + q, ya), at(xb - q, ya), at(xb, ya), at(xb, ya + q),
                at(xb, yb - q), at(xb, yb), at(xb - q, yb), at(xa + q, yb),
                at(xa, yb), at(xa, yb - q), at(xa, ya + q), at(xa, ya),
              ];
              path.moveTo(p[0][0], p[0][1]);
              path.lineTo(p[1][0], p[1][1]);
              path.quadraticCurveTo(p[2][0], p[2][1], p[3][0], p[3][1]);
              path.lineTo(p[4][0], p[4][1]);
              path.quadraticCurveTo(p[5][0], p[5][1], p[6][0], p[6][1]);
              path.lineTo(p[7][0], p[7][1]);
              path.quadraticCurveTo(p[8][0], p[8][1], p[9][0], p[9][1]);
              path.lineTo(p[10][0], p[10][1]);
              path.quadraticCurveTo(p[11][0], p[11][1], p[0][0], p[0][1]);
            } else {
              const p = [at(xa, ya), at(xb, ya), at(xb, yb), at(xa, yb)];
              path.moveTo(p[0][0], p[0][1]);
              path.lineTo(p[1][0], p[1][1]);
              path.lineTo(p[2][0], p[2][1]);
              path.lineTo(p[3][0], p[3][1]);
            }
            path.closePath();
            if (this.marks && this.marks[i] > 0.55) ringed.push([x0 + cell / 2, y0 + cell / 2]);
          }
        }
        for (const [colour, path] of paths) {
          ctx.fillStyle = colour;
          ctx.fill(path);
        }
        // A ring on the stickers to look at again.
        for (const [x, y] of ringed) {
          ctx.beginPath();
          for (let s = 0; s <= 16; s++) {
            const a = (s / 16) * Math.PI * 2;
            const [px, py] = at(x + Math.cos(a) * cell * 0.26, y + Math.sin(a) * cell * 0.26);
            s ? ctx.lineTo(px, py) : ctx.moveTo(px, py);
          }
          ctx.lineWidth = Math.max(2, size / 260);
          ctx.strokeStyle = "rgba(0,0,0,0.85)";
          ctx.stroke();
          ctx.lineWidth = Math.max(1, size / 520);
          ctx.strokeStyle = "rgba(255,255,255,0.95)";
          ctx.stroke();
        }
        // Light from above and a little from the right: faces turned away from it are darker.
        const facing = apply(m, normal);
        const shade = Math.max(0, Math.min(0.26, 0.13 - facing[1] * 0.12 - facing[0] * 0.05));
        if (shade > 0.01) {
          ctx.fillStyle = `rgba(0,0,0,${shade.toFixed(3)})`;
          outline();
          ctx.fill();
          if (bevel > gap) {
            // The stickers reach past the flat of the face into the rounding of
            // its edges: that part of them is darkened by itself.
            const past = new Path2D();
            past.rect(0, 0, size, size);
            corners.forEach(([x, y], i) => (i ? past.lineTo(x, y) : past.moveTo(x, y)));
            past.closePath();
            ctx.save();
            ctx.clip(past, "evenodd");
            for (const path of paths.values()) ctx.fill(path);
            ctx.restore();
          }
        }
      }
    }
    // The arrows of the move that is next. While its own layers turn they
    // turn with them; while other layers turn there are none.
    const cue = this.cue;
    if (cue) {
      const turning = turn && turn.angle !== 0;
      const own = turning && turn.face === cue.face && turn.from === cue.from && turn.to === cue.to;
      if (!turning || own) {
        const shown = this.calm.matches ? 1 : Math.min(1, (performance.now() - this.cueSince) / 220);
        ctx.globalAlpha = shown;
        this.#arrows(cue, f, own ? mul(this.rot, about(FACES[turn.face].normal, -turn.angle)) : this.rot);
        ctx.globalAlpha = 1;
        if (shown < 1) this.draw();
      }
    }
  }

  // The next move as arrows on the cube: one along the middle of every row
  // of stickers that will turn, on every side of the cube that can be seen,
  // pointing the way the stickers will go, and one round the face itself
  // when it is the face that turns. Each arrow is a flat shape lying on its
  // face, head and all, and is projected with the face, so it looks painted
  // on however the cube is held.
  #arrows({ face, from, to, quarters }, f, m) {
    const { ctx, n } = this;
    const a = FACES[face].normal;
    const sense = quarters < 0 ? -1 : 1;
    const lift = 1.012;
    // Half the width of a shaft, the length of a head and half its width, in the cube's own units.
    const [hw, head, wing] = [0.036, 0.2, 0.105];
    // An arrow along a row is no wider than the row: on a large cube it is drawn to the row's measure.
    const fit = Math.min(1, (2 / n) * 0.4 / wing);
    const fill = (points, scale = 1) => {
      ctx.beginPath();
      points.forEach((v, i) => {
        const [x, y] = this.#project(apply(m, v), f);
        i ? ctx.lineTo(x, y) : ctx.moveTo(x, y);
      });
      ctx.closePath();
      ctx.lineJoin = "round";
      ctx.lineWidth = Math.max(scale < 1 ? 1 : 2, (f.size / 260) * Math.max(scale, 0.4));
      ctx.strokeStyle = "rgba(0,0,0,0.8)";
      ctx.stroke();
      ctx.fillStyle = "#ffffff";
      ctx.fill();
    };
    const visible = (normal) => {
      // A face is seen when the eye is on its outer side.
      return (m[6] * normal[0] + m[7] * normal[1] + m[8] * normal[2]) * EYE > 1;
    };
    const at = (o, u, v, x, y) => [o[0] + u[0] * x + v[0] * y, o[1] + u[1] * x + v[1] * y, o[2] + u[2] * x + v[2] * y];
    // Along the layers, on each side face in view.
    for (const side of FACES) {
      const s = side.normal;
      if (s.every((v, i) => Math.abs(v) === Math.abs(a[i])) || !visible(s)) continue;
      // Clockwise seen from the face: stickers on this side move along (-a) x s.
      const d = cross([-a[0] * sense, -a[1] * sense, -a[2] * sense], s);
      const reach = Math.abs(quarters) === 2 ? 0.8 : 0.58;
      const [w, h, g] = [hw * fit, head * Math.max(fit, 0.5), wing * fit];
      for (let layer = from; layer < to; layer++) {
        // The middle of this layer's row, as a distance from the centre along the turning face's normal.
        const mid = 1 - (2 * layer + 1) / n;
        // The middle of the arrow; `d` runs along it and `a` across it, both in the face.
        const o = [s[0] * lift + a[0] * mid, s[1] * lift + a[1] * mid, s[2] * lift + a[2] * mid];
        fill([
          at(o, d, a, -reach, -w),
          at(o, d, a, reach - h, -w),
          at(o, d, a, reach - h, -g),
          at(o, d, a, reach, 0),
          at(o, d, a, reach - h, g),
          at(o, d, a, reach - h, w),
          at(o, d, a, -reach, w),
        ], fit);
      }
    }
    // Round the face itself, when it turns and is in view.
    if (from === 0 && visible(a)) {
      const { right, down } = FACES[face];
      const o = [a[0] * lift, a[1] * lift, a[2] * lift];
      const radius = 0.5;
      const sweep = Math.abs(quarters) === 2 ? Math.PI * 1.1 : Math.PI * 0.6;
      // `right` to `down` is clockwise seen from outside; the arc is centred on the top of the face.
      const angle = (k) => -Math.PI / 2 + sense * (-sweep / 2 + (sweep * k) / 24);
      const ring = (r, k) => at(o, right, down, Math.cos(angle(k)) * r, Math.sin(angle(k)) * r);
      const outer = [];
      const inner = [];
      for (let k = 0; k <= 24; k++) {
        outer.push(ring(radius + hw, k));
        inner.push(ring(radius - hw, k));
      }
      // The head: across the end of the arc, its tip a head's length further round.
      const end = angle(24);
      const tip = end + (sense * head) / radius;
      const point = (r, t) => at(o, right, down, Math.cos(t) * r, Math.sin(t) * r);
      fill([...outer, point(radius + wing, end), point(radius, tip), point(radius - wing, end), ...inner.reverse()]);
    }
  }
}

export { BLANK };
