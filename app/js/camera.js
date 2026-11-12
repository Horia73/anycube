// Reading a cube with the camera: the six faces one after another, each
// held inside a grid on the picture, the colour of every cell sampled in
// the browser. No picture is stored or sent anywhere; what leaves this file
// is 6*N*N colours.
//
// Four parts, so that one can be replaced without the others:
//
//   CameraFeed    the camera itself: open, close, mirror, switch.
//   findFace      (detect.js) one frame to the face of the cube in it,
//                 wherever it is: its four corners and the colours of its
//                 N by N stickers with the perspective undone.
//   sampleGrid    one frame to the colours of an N by N grid in a fixed
//                 place of the picture: what is used when no face is found.
//   CameraScanner the six-step session and its controls.
//
// The face is looked for several times a second and what is found is
// drawn over the picture: its outline, and on each sticker a mark in the
// colour it is being read as. Nothing is captured until Capture is
// pressed; then the face as found at that moment is taken, or the grid's
// cells if none is.
//
// Following a cube as it is turned in the hand and reading moves from the
// picture (README, "Not done") would keep the feed and the detector and
// put a session that follows moves in the place of the six steps.

import { cellColour, guess, learn, similarReading } from "./colour.js";
import { findFace, FACE_SCORE } from "./detect.js";

// Names from the user's starting position. In facelet order the front is
// the first face the camera sees, on the far side of the cube for the user.
const FACE_NAMES = ["Top", "Left", "Back", "Bottom", "Right", "Front"];

// The order the faces are asked for, and what to do to get to each, said
// as what the hands do. With the top kept on top for the four sides, and
// the top and bottom tipped towards the camera, every face is seen the way
// its facelets are written down (URFDLB, row by row), so no picture has to
// be turned. Which way the cube goes round does not have to be right, nor
// the order, nor the way up: the six pictures are put together afterwards
// the way they make a cube (scan-fit.js). `sign` is the small drawing.
export const SCAN_STEPS = [
  { face: 2, signs: ["hold"], text: "Hold any face level, pointing away from you towards the camera. Note which face is on top." },
  { face: 4, signs: ["round"], text: "Rotate the cube a quarter turn counterclockwise as seen from above. The captured face moves to your left." },
  { face: 5, signs: ["round"], text: "Another quarter turn the same way. Keep the top on top." },
  { face: 1, signs: ["round"], text: "And once more the same way." },
  { face: 0, via: { face: 2 }, signs: ["round", "up"], text: "Round once more, then tip the top away from you, towards the camera." },
  { face: 3, via: { face: 5, upsideDown: true }, signs: ["up", "up"], text: "Keep tipping the same way, two quarter turns, until the bottom faces the camera." },
];

// The small drawings: the face held to the camera, and an arrow for how
// the cube is turned to bring the next one round. Two of them side by side
// where it takes two moves.
const HEAD = (x, y, turn) => `<path class="head" d="M-4.5 -3.5 4 0 -4.5 3.5z" transform="translate(${x} ${y}) rotate(${turn})"/>`;
const FACE = '<rect x="11" y="9" width="18" height="18" rx="3.5"/>';
const SIGNS = {
  hold: `${FACE}<circle class="head" cx="20" cy="18" r="2.6"/>`,
  round: `${FACE}<path d="M5 31c8 6 22 6 29 1"/>${HEAD(35, 31.5, -32)}`,
  up: `${FACE}<path d="M34 32c6 -7 6 -19 1 -26"/>${HEAD(34.5, 5, -122)}`,
};

// The grid takes this much of the picture's shorter side.
export const GRID_SHARE = 0.7;

const el = (tag, attrs = {}, children = []) => {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else node.setAttribute(k, v);
  }
  node.append(...children);
  return node;
};

const css = ([r, g, b]) => `rgb(${Math.round(r)}, ${Math.round(g)}, ${Math.round(b)})`;

/** The camera: a stream in a video element. */
export class CameraFeed {
  constructor(video) {
    this.video = video;
    this.stream = null;
    this.cameras = [];
    this.deviceId = undefined;
    // Whether the camera looks at its user, and is best shown as a mirror.
    this.facesUser = false;
  }

  get running() {
    return !!this.stream;
  }

  /** Opens a camera: the one looking away from the user if there is a choice. Throws what getUserMedia throws. */
  async open(video = { facingMode: { ideal: "environment" } }) {
    if (!navigator.mediaDevices?.getUserMedia) {
      const error = new Error("no camera access");
      error.name = window.isSecureContext ? "NotSupportedError" : "InsecureContextError";
      throw error;
    }
    this.close();
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: { ...video, width: { ideal: 1280 }, height: { ideal: 720 } },
    });
    this.video.srcObject = this.stream;
    await this.video.play().catch(() => {});
    const settings = this.stream.getVideoTracks()[0]?.getSettings?.() ?? {};
    this.deviceId = settings.deviceId;
    this.facesUser = settings.facingMode ? settings.facingMode === "user" : !/Android|iPhone|iPad/i.test(navigator.userAgent);
    const devices = await navigator.mediaDevices.enumerateDevices().catch(() => []);
    this.cameras = devices.filter((d) => d.kind === "videoinput");
  }

  /** Switches to the next camera of the device. */
  async next() {
    if (this.cameras.length < 2) return;
    const at = this.cameras.findIndex((c) => c.deviceId === this.deviceId);
    await this.open({ deviceId: { exact: this.cameras[(at + 1) % this.cameras.length].deviceId } });
  }

  close() {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    this.video.srcObject = null;
  }
}

/**
 * The colour of each cell of an n by n grid in the middle of the current
 * frame, row by row as the camera sees them (not as a mirrored picture
 * shows them), or null while there is no frame yet.
 */
export function sampleGrid(video, n, scratch) {
  const [w, h] = [video.videoWidth, video.videoHeight];
  if (!w || !h || video.readyState < 2) return null;
  const px = 12; // pixels a cell is sampled at
  const size = n * px;
  scratch.width = scratch.height = size;
  const ctx = scratch.getContext("2d", { willReadFrequently: true });
  // The picture is shown cropped to a square; the grid is in the middle of that square.
  const side = Math.min(w, h) * GRID_SHARE;
  ctx.drawImage(video, (w - side) / 2, (h - side) / 2, side, side, 0, 0, size, size);
  const colours = [];
  // The middle half of each cell: its edges are the gaps between stickers and the neighbours.
  const inset = Math.floor(px / 4);
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      colours.push(cellColour(ctx.getImageData(c * px + inset, r * px + inset, px - 2 * inset, px - 2 * inset).data));
    }
  }
  return colours;
}

// Where the point (u, v) of the unit square lies in a four-cornered shape
// seen in perspective: `corners` are its top-left, top-right, bottom-right
// and bottom-left.
function inQuad(corners, u, v) {
  const [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = corners;
  const [dx1, dy1, dx2, dy2] = [x1 - x2, y1 - y2, x3 - x2, y3 - y2];
  const [sx, sy] = [x0 - x1 + x2 - x3, y0 - y1 + y2 - y3];
  const den = dx1 * dy2 - dx2 * dy1 || 1e-9;
  const g = (sx * dy2 - dx2 * sy) / den;
  const h = (dx1 * sy - sx * dy1) / den;
  const [a, b, c] = [x1 - x0 + g * x1, x3 - x0 + h * x3, x0];
  const [d, e, f] = [y1 - y0 + g * y1, y3 - y0 + h * y3, y0];
  const w = g * u + h * v + 1;
  return [(a * u + b * v + c) / w, (d * u + e * v + f) / w];
}

// The picture is looked at this small to find the face: enough for stickers, quick on a phone.
const LOOK = 288; // and half as much again for the 6x6x6 and 7x7x7, whose stickers are small

/** The six faces, one after another. */
export class CameraScanner {
  /**
   * @param {HTMLElement} frame an empty element for the picture and its grid
   * @param {HTMLElement} side an empty element for the instructions and buttons
   * @param hooks `onstep(face)` when a face is asked for, `onface(face, colours)`
   *   when one is captured (or taken back, with null), `ondone(samples, sources)`
   *   with 6*n*n colours and six capture methods in facelet order (return a
   *   warning to keep the scan open),
   *   `onstop()` when the camera is switched off.
   */
  constructor(frame, side, hooks) {
    this.hooks = hooks;
    this.n = 3;
    this.step = 0;
    this.faces = new Array(6).fill(null);
    this.sources = new Array(6).fill(null);
    this.live = null;
    this.timer = 0;
    this.scratch = document.createElement("canvas");

    this.video = el("video", { playsinline: "", muted: "", autoplay: "" });
    this.video.muted = true;
    this.feed = new CameraFeed(this.video);
    this.grid = el("div", { class: "cam-grid", "aria-hidden": "true" });
    // What is found in the picture is drawn here, over it.
    this.over = el("canvas", { class: "cam-found", "aria-hidden": "true" });
    this.small = document.createElement("canvas");
    this.found = null; // { corners, cells, at }: the face as last found, corners as shares of the picture shown
    this.frame = frame;
    // Whether the cube is found, said on the picture.
    this.note = el("p", { class: "cam-note", role: "status" });
    frame.append(this.video, this.grid, this.over, this.note);

    this.sign = el("span", { class: "cam-sign", "aria-hidden": "true" });
    this.heading = el("p", { class: "cam-heading", "aria-live": "polite" });
    this.text = el("p", { class: "cam-text" });
    this.shots = el("ol", { class: "cam-shots", "aria-label": "Faces captured" });
    this.capture = el("button", { type: "button", class: "button primary", text: "Capture", title: "Capture (Space)", "aria-keyshortcuts": "Space" });
    this.back = el("button", { type: "button", class: "button", text: "Retake" });
    this.flip = el("button", { type: "button", class: "button quiet", "aria-pressed": "false", text: "Mirror" });
    this.swap = el("button", { type: "button", class: "button quiet", text: "Other camera", hidden: "" });
    this.off = el("button", { type: "button", class: "button quiet", text: "Stop" });
    side.append(
      el("div", { class: "cam-say" }, [this.sign, el("div", {}, [this.heading, this.text])]),
      this.shots,
      el("div", { class: "cam-buttons" }, [this.capture, this.back, this.flip, this.swap, this.off]),
    );
    this.capture.addEventListener("click", () => this.#capture());
    this.back.addEventListener("click", () => this.#retake());
    this.flip.addEventListener("click", () => this.#setMirror(!this.mirror));
    this.swap.addEventListener("click", async () => {
      try {
        await this.feed.next();
        this.#opened();
      } catch {
        this.stop();
      }
    });
    this.off.addEventListener("click", () => this.stop());
  }

  get running() {
    return this.feed.running;
  }

  /** Asks for the camera and begins with the first face. Throws what getUserMedia throws. */
  async start(n) {
    this.n = n;
    this.step = 0;
    this.faces.fill(null);
    this.sources.fill(null);
    this.again = -1;
    this.jumped = false;
    await this.feed.open();
    this.#opened();
    this.#layout();
    this.#show();
    this.timer = setInterval(() => this.#sample(), 120);
  }

  #opened() {
    // A camera that looks at its user is shown as a mirror, as people expect; one that looks away is not.
    this.#setMirror(this.feed.facesUser);
    this.swap.hidden = this.feed.cameras.length < 2;
  }

  /** Switches the camera off. */
  stop() {
    clearInterval(this.timer);
    const was = this.running;
    this.feed.close();
    if (was) this.hooks.onstop?.();
  }

  #setMirror(on) {
    this.mirror = on;
    this.frame.classList.toggle("mirrored", on);
    this.flip.setAttribute("aria-pressed", String(on));
  }

  #layout() {
    const n = this.n;
    this.grid.style.setProperty("--n", n);
    this.grid.replaceChildren(...Array.from({ length: n * n }, () => el("i")));
    this.shots.replaceChildren(
      ...SCAN_STEPS.map(({ face }, k) => {
        // Each is a button: pressing a face goes to it, and takes it back if it was captured.
        const li = el("li", { "data-face": face, role: "button", tabindex: "0" });
        li.style.setProperty("--n", n);
        for (let i = 0; i < n * n; i++) li.append(el("i"));
        const go = () => this.#goTo(k);
        li.addEventListener("click", go);
        li.addEventListener("keydown", (e) => {
          if (e.key !== "Enter" && e.key !== " ") return;
          e.preventDefault();
          go();
        });
        return li;
      }),
    );
  }

  #show() {
    const done = this.step >= SCAN_STEPS.length;
    const { face, text, signs, via } = SCAN_STEPS[Math.min(this.step, SCAN_STEPS.length - 1)];
    this.heading.textContent = done ? "All six faces captured" : `Face ${this.step + 1} of 6: ${FACE_NAMES[face].toLowerCase()}`;
    // A face gone back to is not the next one round: it is shown as the cube in the panel shows it.
    const again = this.jumped && !done;
    this.text.textContent = done ? "" : again ? "Hold the cube as shown, with the face to capture pointing away from you, towards the camera." : text;
    this.text.classList.remove("warn");
    this.sign.innerHTML = (again ? ["hold"] : signs).map((name) => `<svg viewBox="0 0 40 38">${SIGNS[name]}</svg>`).join("<i>then</i>");
    this.again = -1;
    this.capture.disabled = done;
    this.back.disabled = !this.faces.some(Boolean);
    [...this.shots.children].forEach((li, k) => {
      const colours = this.faces[SCAN_STEPS[k].face];
      li.classList.toggle("current", k === this.step);
      li.classList.toggle("taken", !!colours);
      const name = FACE_NAMES[SCAN_STEPS[k].face].toLowerCase();
      li.title = colours ? `Take the ${name} face again` : `Go to the ${name} face`;
      li.setAttribute("aria-label", li.title);
      [...li.children].forEach((cell, i) => (cell.style.background = colours ? css(colours[i]) : ""));
    });
    // The face asked for, and the face the cube passes on its way there when it takes two moves.
    if (!done) this.hooks.onstep?.(face, again ? undefined : via);
  }

  // Looks for the face in the picture of the moment: the middle square of
  // the frame (what the page shows), made small.
  #find() {
    const video = this.video;
    const [w, h] = [video.videoWidth, video.videoHeight];
    if (!w || !h || video.readyState < 2) return null;
    const side = Math.min(w, h);
    const look = this.n >= 6 ? LOOK * 1.5 : LOOK;
    this.small.width = this.small.height = look;
    const ctx = this.small.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(video, (w - side) / 2, (h - side) / 2, side, side, 0, 0, look, look);
    const began = performance.now();
    let face = null;
    try {
      face = findFace(ctx.getImageData(0, 0, look, look), this.n);
    } catch {
      face = null;
    }
    this.cost = performance.now() - began;
    if (!face || face.score < FACE_SCORE) return null;
    return { corners: face.corners.map(([x, y]) => [x / look, y / look]), cells: face.cells, at: performance.now() };
  }

  // Draw the detected face and the camera's reading over each sticker.
  #mark() {
    const box = this.over;
    const size = Math.round(box.clientWidth * Math.min(window.devicePixelRatio || 1, 2));
    if (!size) return;
    if (box.width !== size || box.height !== size) box.width = box.height = size;
    const ctx = box.getContext("2d");
    ctx.clearRect(0, 0, size, size);
    const found = this.found;
    this.grid.hidden = !!found;
    if (!found) return;
    const n = this.n;
    const at = (u, v) => inQuad(found.corners, u, v).map((p) => p * size);
    ctx.lineJoin = "round";
    ctx.beginPath();
    [[0, 0], [1, 0], [1, 1], [0, 1]].forEach(([u, v], i) => {
      const [x, y] = at(u, v);
      i ? ctx.lineTo(x, y) : ctx.moveTo(x, y);
    });
    ctx.closePath();
    ctx.lineWidth = Math.max(4, size / 110);
    ctx.strokeStyle = "rgba(0,0,0,0.55)";
    ctx.stroke();
    ctx.lineWidth = Math.max(2, size / 220);
    ctx.strokeStyle = "#ffffff";
    ctx.stroke();
    const part = 0.18; // half the mark, as a share of a sticker
    for (let r = 0; r < n; r++) {
      for (let c = 0; c < n; c++) {
        const [u, v] = [(c + 0.5) / n, (r + 0.5) / n];
        ctx.beginPath();
        [[-1, -1], [1, -1], [1, 1], [-1, 1]].forEach(([du, dv], i) => {
          const [x, y] = at(u + (du * part) / n, v + (dv * part) / n);
          i ? ctx.lineTo(x, y) : ctx.moveTo(x, y);
        });
        ctx.closePath();
        ctx.lineWidth = Math.max(5, size / 130);
        ctx.strokeStyle = "rgba(0,0,0,0.6)";
        ctx.stroke();
        ctx.lineWidth = Math.max(3, size / 200);
        ctx.strokeStyle = "#ffffff";
        ctx.stroke();
        ctx.fillStyle = css(found.cells[r * n + c]);
        ctx.fill();
      }
    }
  }

  #sample() {
    // The face wherever it is, if it can be found; a little steadier than frame by frame.
    const face = this.#find();
    if (face && this.found) {
      face.corners = face.corners.map(([x, y], k) => [(x + this.found.corners[k][0]) / 2, (y + this.found.corners[k][1]) / 2]);
    }
    // A face lost for a moment (a blur, a hand in the way) is kept for that moment.
    this.found = face ?? (this.found && performance.now() - this.found.at < 350 ? this.found : null);
    this.#mark();
    this.frame.classList.toggle("found", !!this.found);
    const say = this.found ? "Cube found" : "Looking for the cube. Or hold a face inside the grid.";
    if (this.note.textContent !== say) this.note.textContent = say;
    const colours = sampleGrid(this.video, this.n, this.scratch);
    if (!colours) return;
    this.live = colours;
    [...this.grid.children].forEach((cell, i) => (cell.style.color = css(colours[i])));
  }

  // What `guess` is to go by: the stickers captured so far and those in the picture now.
  #colours(now) {
    return learn([...this.faces.filter(Boolean).flat(), ...now]);
  }

  #capture() {
    this.#sample();
    // The face as found at this moment; the grid's cells if there is none.
    if (this.found) this.take(this.found.cells, "detected");
    else if (this.live) this.take(this.live, "grid");
  }

  /** Takes a face as read: n*n colours, row by row. What Capture does with the picture of the moment. */
  take(colours, source = "provided") {
    if (this.step >= SCAN_STEPS.length) return;
    this.live = colours;
    // A face that looks like one already shown is said so at once; a second
    // press takes it all the same (two faces of a cube nearly solved can be alike).
    const like = this.#shownBefore(colours);
    if (like >= 0 && this.again !== this.step) {
      this.again = this.step;
      this.text.textContent = `This looks like face ${like + 1} again. Show another face, or press Capture once more to keep it.`;
      this.text.classList.add("warn");
      return;
    }
    const face = SCAN_STEPS[this.step].face;
    this.faces[face] = this.live;
    this.sources[face] = source;
    this.hooks.onface?.(face, this.live);
    // On to the first face not captured yet: the next one as a rule, another after one was taken again.
    const open = SCAN_STEPS.findIndex((step) => !this.faces[step.face]);
    // In turn when it is the face after the one just captured; otherwise it is one gone back to.
    this.jumped = open !== this.step + 1 && open >= 0;
    this.step = open < 0 ? SCAN_STEPS.length : open;
    this.#show();
    if (this.step === SCAN_STEPS.length) {
      const warning = this.hooks.ondone?.(this.faces.flat(), this.sources.slice());
      if (warning) {
        this.heading.textContent = "Check the centre colours";
        this.text.textContent = warning;
        this.text.classList.add("warn");
      } else {
        this.stop();
      }
    }
  }

  // The step at which a face was captured that the one in the picture now is alike:
  // by its middle sticker on an odd cube (no two faces share one), by all of it
  // otherwise. The live colour names and the observed shades must both agree.
  #shownBefore(colours) {
    const nn = this.n * this.n;
    const mid = (nn - 1) / 2;
    const seen = this.#colours(colours);
    const name = (rgb) => guess(rgb, seen).colour;
    const now = colours.map(name);
    const same = (rgb, i) => name(rgb) === now[i] && similarReading(rgb, colours[i]);
    for (let k = 0; k < SCAN_STEPS.length; k++) {
      const was = this.faces[SCAN_STEPS[k].face];
      if (!was || k === this.step) continue;
      if (this.n % 2 === 1 ? same(was[mid], mid) : was.every(same)) return k;
    }
    return -1;
  }

  // Goes to a face: to capture it, or, if it was captured, to take it away and capture it again.
  #goTo(k) {
    if (!this.running || k < 0 || k >= SCAN_STEPS.length) return;
    const face = SCAN_STEPS[k].face;
    if (this.faces[face]) {
      this.faces[face] = null;
      this.sources[face] = null;
      this.hooks.onface?.(face, null);
    }
    this.step = k;
    this.jumped = true;
    this.#show();
  }

  // Retake: the face captured last before the one being asked for is taken away and asked for again.
  #retake() {
    let k = Math.min(this.step, SCAN_STEPS.length) - 1;
    while (k >= 0 && !this.faces[SCAN_STEPS[k].face]) k--;
    if (k < 0) k = SCAN_STEPS.findLastIndex((step) => this.faces[step.face]);
    if (k >= 0) this.#goTo(k);
  }
}
