// Reading a cube with the camera: the six faces one after another, each
// held inside a grid on the picture, the colour of every cell sampled in
// the browser. No picture is stored or sent anywhere; what leaves this file
// is 6*N*N colours.
//
// Three parts, so that one can be replaced without the others:
//
//   CameraFeed    the camera itself: open, close, mirror, switch.
//   sampleGrid    one frame to the colours of an N by N grid in a fixed
//                 place of the picture.
//   CameraScanner the six-step session and its controls.
//
// Following a cube as it is turned in the hand (README, "Live tracking")
// would keep the feed, put a detector that finds the cube in each frame in
// the place of sampleGrid, and a session that follows moves in the place
// of the six steps.

import { cellColour, guess, STICKERS, unalike } from "./colour.js";

const FACE_NAMES = ["Top", "Right", "Front", "Bottom", "Left", "Back"];

// The order the faces are asked for, and what to do to get to each, said
// as what the hands do. With the top kept on top for the four sides, and
// the top and bottom tipped towards the camera, every face is seen the way
// its facelets are written down (URFDLB, row by row), so no picture has to
// be turned. Which way the cube goes round does not have to be right, nor
// the order, nor the way up: the six pictures are put together afterwards
// the way they make a cube (scan-fit.js). `sign` is the small drawing.
export const SCAN_STEPS = [
  { face: 2, sign: "hold", text: "Hold any face to the camera, the cube level. Note which face is on top." },
  { face: 1, sign: "round", text: "Turn the cube a quarter turn sideways. The top stays on top." },
  { face: 5, sign: "round", text: "Another quarter turn the same way, top still on top." },
  { face: 4, sign: "round", text: "And once more the same way." },
  { face: 0, sign: "tip", text: "Now tip the cube so that its top faces the camera." },
  { face: 3, sign: "tip2", text: "Last, tip it the other way: the bottom faces the camera." },
];

// The small drawings: a cube face, and an arrow for how the cube is turned to bring it round.
const SIGNS = {
  hold: '<rect x="7" y="7" width="18" height="18" rx="3"/><path d="M11 13h10M11 16h10M11 19h10" opacity=".35"/>',
  round: '<rect x="9" y="9" width="14" height="14" rx="3"/><path d="M4 27c6 4 18 4 24 0"/><path d="M24.5 29.5 28 27l-3-3"/>',
  tip: '<rect x="9" y="9" width="14" height="14" rx="3"/><path d="M27 4c4 6 4 18 0 24"/><path d="M29.5 24.5 27 28l-3-3"/>',
  tip2: '<rect x="9" y="9" width="14" height="14" rx="3"/><path d="M27 28c4-6 4-18 0-24"/><path d="M29.5 7.5 27 4l-3 3"/>',
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

/** The six faces, one after another. */
export class CameraScanner {
  /**
   * @param {HTMLElement} frame an empty element for the picture and its grid
   * @param {HTMLElement} side an empty element for the instructions and buttons
   * @param hooks `onstep(face)` when a face is asked for, `onface(face, colours)`
   *   when one is captured (or taken back, with null), `ondone(samples)` with
   *   6*n*n colours in facelet order, `onstop()` when the camera is switched off.
   */
  constructor(frame, side, hooks) {
    this.hooks = hooks;
    this.n = 3;
    this.step = 0;
    this.faces = new Array(6).fill(null);
    this.live = null;
    this.timer = 0;
    this.scratch = document.createElement("canvas");

    this.video = el("video", { playsinline: "", muted: "", autoplay: "" });
    this.video.muted = true;
    this.feed = new CameraFeed(this.video);
    this.grid = el("div", { class: "cam-grid", "aria-hidden": "true" });
    this.frame = frame;
    frame.append(this.video, this.grid);

    this.sign = el("span", { class: "cam-sign", "aria-hidden": "true" });
    this.heading = el("p", { class: "cam-heading", "aria-live": "polite" });
    this.text = el("p", { class: "cam-text" });
    this.shots = el("ol", { class: "cam-shots", "aria-label": "Faces captured" });
    this.capture = el("button", { type: "button", class: "button primary", text: "Capture" });
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
    this.again = -1;
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
      ...SCAN_STEPS.map(({ face }) => {
        const li = el("li", { "data-face": face, title: FACE_NAMES[face] });
        li.style.setProperty("--n", n);
        for (let k = 0; k < n * n; k++) li.append(el("i"));
        return li;
      }),
    );
  }

  #show() {
    const done = this.step >= SCAN_STEPS.length;
    const { face, text, sign } = SCAN_STEPS[Math.min(this.step, SCAN_STEPS.length - 1)];
    this.heading.textContent = done ? "All six faces captured" : `Face ${this.step + 1} of 6: ${FACE_NAMES[face].toLowerCase()}`;
    this.text.textContent = done ? "" : text;
    this.text.classList.remove("warn");
    this.sign.innerHTML = `<svg viewBox="0 0 32 32">${SIGNS[sign]}</svg>`;
    this.again = -1;
    this.capture.disabled = done;
    this.back.disabled = this.step === 0;
    [...this.shots.children].forEach((li, k) => {
      const colours = this.faces[SCAN_STEPS[k].face];
      li.classList.toggle("current", k === this.step);
      li.classList.toggle("taken", !!colours);
      [...li.children].forEach((cell, i) => (cell.style.background = colours ? css(colours[i]) : ""));
    });
    if (!done) this.hooks.onstep?.(face);
  }

  #sample() {
    const colours = sampleGrid(this.video, this.n, this.scratch);
    if (!colours) return;
    this.live = colours;
    // Each cell of the grid carries a mark in the page's own colour for what
    // is being read under it (the page's red over a sticker read as red),
    // broken where the reading could as well be another colour; the sticker
    // itself shows round the mark. The grid turns with the picture when
    // that is mirrored.
    [...this.grid.children].forEach((cell, i) => {
      const read = guess(colours[i]);
      cell.style.color = STICKERS[read.colour];
      cell.classList.toggle("unsure", read.doubt > 0.7);
    });
  }

  #capture() {
    this.#sample();
    if (this.live) this.take(this.live);
  }

  /** Takes a face as read: n*n colours, row by row. What Capture does with the picture of the moment. */
  take(colours) {
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
    this.hooks.onface?.(face, this.live);
    this.step++;
    this.#show();
    if (this.step === SCAN_STEPS.length) {
      const samples = this.faces.flat();
      this.stop();
      this.hooks.ondone?.(samples);
    }
  }

  // The step at which a face was captured that the one in the picture now is alike:
  // by its middle sticker on an odd cube (no two faces share one), by all of it otherwise.
  #shownBefore(colours) {
    const nn = this.n * this.n;
    const mid = (nn - 1) / 2;
    for (let k = 0; k < this.step; k++) {
      const was = this.faces[SCAN_STEPS[k].face];
      if (!was) continue;
      if (this.n % 2 === 1) {
        if (unalike(colours[mid], was[mid]) < 2.5) return k;
      } else {
        let far = 0;
        for (let i = 0; i < nn; i++) far = Math.max(far, unalike(colours[i], was[i]));
        if (far < 2.5) return k;
      }
    }
    return -1;
  }

  #retake() {
    if (this.step === 0) return;
    this.step--;
    const face = SCAN_STEPS[this.step].face;
    this.faces[face] = null;
    this.hooks.onface?.(face, null);
    this.#show();
  }
}
