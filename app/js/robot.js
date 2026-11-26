// A cube robot driven from the page: the server holds the robot (its
// driver and serial port), the page holds the camera, reads the six faces
// and shows the cube solving in step with the robot. The contract between
// the two is robots-spec/API.md; nothing here is about one robot.
//
// Two parts:
//
//   the core     no document needed (the tests run it under Node): the
//                server's robot API, the run that follows its events, the
//                square cut out of a frame and turned, the six faces read
//                and put together the way MultiCuber's anycube_scan.mjs
//                does, and the time left while the robot turns.
//   mountRobot   the page's side: connecting, the camera, Scan and Stop,
//                and the cube on screen following the robot. It is
//                mounted only when the server says it drives robots; until
//                a robot is connected the page is as it always is.

import { CameraFeed } from "./camera.js";
import { cellColour, named, oppositeCentres, STICKERS } from "./colour.js";
import { CubeModel, FACE_LETTERS } from "./cube-model.js";
import { findFace } from "./detect.js";
import { arrange, assembleScan, reviewStickers } from "./scan-fit.js";

// ------------------------------------------------------------- the frame

/** The centred square of a w by h picture, as robot.py photo() cuts it. */
export function squareCrop(w, h) {
  const side = Math.min(w, h);
  return { sx: Math.floor((w - side) / 2), sy: Math.floor((h - side) / 2), side };
}

/** 0, 90, 180 or 270, from whatever the server sent; null if it is not a quarter turn. */
export function quarterTurn(rotate) {
  const r = (((Math.round(Number(rotate)) || 0) % 360) + 360) % 360;
  return r % 90 === 0 ? r : null;
}

/**
 * A square RGBA picture of `side` pixels turned `rotate` degrees clockwise
 * (cv2.ROTATE_90_CLOCKWISE for 90, ROTATE_180, ROTATE_90_COUNTERCLOCKWISE
 * for 270): the pixel at row r, column c of the result.
 */
export function rotateSquare(data, side, rotate) {
  const turn = quarterTurn(rotate);
  if (turn === null) throw new Error(`not a quarter turn: ${rotate}`);
  const out = new Uint8ClampedArray(data.length);
  const src = new Uint32Array(data.buffer, data.byteOffset, side * side);
  const dst = new Uint32Array(out.buffer);
  const last = side - 1;
  if (turn === 0) dst.set(src);
  else if (turn === 180) for (let i = 0, j = side * side - 1; j >= 0; i++, j--) dst[i] = src[j];
  else {
    for (let r = 0; r < side; r++) {
      const row = r * side;
      if (turn === 90) for (let c = 0; c < side; c++) dst[row + c] = src[(last - c) * side + r];
      else for (let c = 0; c < side; c++) dst[row + c] = src[c * side + last - r];
    }
  }
  return out;
}

/**
 * How many pixels a side a frame is read at. Smaller than MultiCuber's
 * bridge gives anycube (320, 480 from 6x6x6): finding a face costs about
 * its area, and at 240 a sticker of a 5x5x5 the robot holds is still some
 * 35 pixels across. Measured on the robot's own pictures of a 4x4x4
 * (MultiCuber/CubeScan): the same cube at 200, 240, 320 and 480, six faces
 * read in 33, 44, 80 and 172 ms.
 */
export const frameSize = (n) => (n >= 6 ? 360 : 240);

/**
 * The frame of the moment from a playing video: its centred square, made
 * `frameSize` small, turned `rotate` degrees clockwise. Null while there is
 * no frame. A few milliseconds: it is what stands between the robot's
 * `photo` and the page's answer.
 */
export function grabFrame(video, rotate, n, canvas) {
  const [w, h] = [video.videoWidth, video.videoHeight];
  if (!w || !h || video.readyState < 2) return null;
  const { sx, sy, side } = squareCrop(w, h);
  const size = Math.min(side, frameSize(n));
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  ctx.drawImage(video, sx, sy, side, side, 0, 0, size, size);
  const { data } = ctx.getImageData(0, 0, size, size);
  return { width: size, height: size, data: rotateSquare(data, size, rotate) };
}

// ------------------------------------------------------------ the faces

// When no face is found, the stickers are read from a grid this share of
// the picture's shorter side, in the middle: where the robot holds the face.
export const GRID_SHARE = 0.8;

/** The n*n colours of a centred grid, the middle half of each cell (as anycube_scan.mjs). */
export function gridCells(image, n, share = GRID_SHARE) {
  const { width: W, height: H, data } = image;
  const side = Math.min(W, H) * share;
  const [x0, y0] = [(W - side) / 2, (H - side) / 2];
  const step = side / n;
  const cells = [];
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      const px = [];
      for (let y = Math.round(y0 + (r + 0.25) * step); y < y0 + (r + 0.75) * step; y++) {
        for (let x = Math.round(x0 + (c + 0.25) * step); x < x0 + (c + 0.75) * step; x++) {
          const i = (y * W + x) * 4;
          px.push(data[i], data[i + 1], data[i + 2], 255);
        }
      }
      cells.push(cellColour(Uint8ClampedArray.from(px)));
    }
  }
  return cells;
}

/** One picture to its n*n colours: the face found in it, else the grid. */
export function readFace(image, n) {
  let found = null;
  try {
    found = findFace(image, n);
  } catch {
    found = null;
  }
  return found ? { cells: found.cells, source: { found: true, score: found.score } } : { cells: gridCells(image, n), source: { found: false } };
}

/**
 * The faces (0..5) whose photos are the same picture, sticker for sticker
 * within a camera's noise: no two faces of a cube are, so a camera that
 * still showed the face before when the photo was taken.
 */
export function repeatedFaces(faces, tolerance = 10) {
  const pairs = [];
  for (let a = 0; a < faces.length; a++) {
    for (let b = a + 1; b < faces.length; b++) {
      const [p, q] = [faces[a], faces[b]];
      if (p && q && p.length === q.length && p.every((c, i) => c.every((v, k) => Math.abs(v - q[i][k]) <= tolerance))) pairs.push([a, b]);
    }
  }
  return pairs;
}

const NOTES = {
  taken: "read as the pictures were taken",
  placed: "some pictures were turned or out of place; placed the one way that makes a cube",
  recoloured: "two doubtful stickers were exchanged to make a cube",
  several: "the pictures fit together as more than one cube",
  none: "the pictures do not make a cube",
};

/**
 * Six faces' colours (6*n*n, URFDLB, row by row) to the cube they make, as
 * anycube_scan.mjs does it: `state` is the facelet text when they make one
 * cube, else null.
 */
export function assembleFaces(n, samples) {
  const model = new CubeModel(n);
  const { sorted, made } = assembleScan(n, samples);
  const lying = arrange(n, made.labels, made);
  const { facelets: reconstructed, palette } = named(n, sorted, lying);
  const centreMismatch = n % 2 === 1 && !oppositeCentres(palette);
  const ok = made.kind !== "none" && made.kind !== "several" && !centreMismatch;
  const facelets = centreMismatch ? reconstructed : model.withCentres(reconstructed);
  const doubt = arrange(n, sorted.doubt, made);
  return {
    ok,
    state: ok ? model.toText(facelets) : null,
    kind: made.kind,
    note: centreMismatch ? "the centres are not three pairs of opposite colours" : NOTES[made.kind],
    facelets,
    palette,
    made,
    sorted,
    doubt,
    review: reviewStickers(n, doubt),
    centreMismatch,
  };
}

// ------------------------------------------------------------ the server

export class RobotError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/** The server's robot API (robots-spec/API.md). Every failure is a RobotError. */
export class RobotApi {
  constructor(fetcher = (...args) => globalThis.fetch(...args)) {
    this.fetch = fetcher;
  }

  async #request(method, path, body, signal) {
    let response;
    try {
      response = await this.fetch(path, {
        method,
        headers: body === undefined ? undefined : { "content-type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal,
      });
    } catch (error) {
      if (error?.name === "AbortError") throw error;
      throw new RobotError("network", "No connection to the anycube server.");
    }
    let answer = null;
    try {
      answer = await response.json();
    } catch {
      answer = null;
    }
    if (answer?.ok === false || !response.ok) {
      const e = answer?.error ?? {};
      throw new RobotError(e.code ?? `http_${response.status}`, e.message ?? `The server answered ${response.status}.`);
    }
    if (!answer || typeof answer !== "object") throw new RobotError("bad_answer", "The server's answer was not understood.");
    return answer;
  }

  robots() { return this.#request("GET", "/api/robots"); }
  connect(driver, port) { return this.#request("POST", "/api/robot/connect", port ? { driver, port } : { driver }); }
  disconnect() { return this.#request("POST", "/api/robot/disconnect", {}); }
  scan() { return this.#request("POST", "/api/robot/scan", {}); }
  photoTaken(run, index) { return this.#request("POST", "/api/robot/photo-taken", { run, index }); }
  solve(run, size, facelets, analysisMs) {
    return this.#request("POST", "/api/robot/solve", { run, size, facelets, analysis_ms: analysisMs });
  }
  stop() { return this.#request("POST", "/api/robot/stop", {}); }
  events(since, wait, signal) {
    return this.#request("GET", `/api/robot/events?since=${since}&wait=${wait}`, undefined, signal);
  }
}

// ----------------------------------------------------------- time left

/**
 * The time left of a plan as the robot works through it: the plan's own
 * estimate to begin with, then what is left of it scaled by how the ops
 * done so far compared with their estimates.
 */
export class Eta {
  constructor(plan, now) {
    this.est = (plan.ops ?? []).map((op) => Number(op.est_ms) || 0);
    const sum = this.est.reduce((a, b) => a + b, 0);
    this.total = Number.isFinite(plan.eta_ms) ? plan.eta_ms : sum;
    this.doneEst = 0; // the estimates of the ops up to the last one done
    this.seenEst = 0; // of the ops reported done: their estimates and what they took
    this.seenActual = 0;
    this.last = -1;
    this.ratio = 1;
    this.endAt = now + this.total;
  }

  op(i, actualMs, now) {
    if (!Number.isInteger(i) || i < 0 || i >= this.est.length || i <= this.last) return;
    for (let k = this.last + 1; k <= i; k++) this.doneEst += this.est[k];
    this.last = i;
    if (Number.isFinite(actualMs)) {
      this.seenEst += this.est[i];
      this.seenActual += actualMs;
    }
    if (this.seenEst > 0) this.ratio = Math.max(0.5, Math.min(2, this.seenActual / this.seenEst));
    this.endAt = now + Math.max(0, this.total - this.doneEst) * this.ratio;
  }

  left(now) {
    return Math.max(0, this.endAt - now);
  }
}

// ------------------------------------------------------------- the run

const SIDES = 6;

/**
 * A connected robot: the events of the server followed for as long as it
 * is connected, and one run at a time (scan, then solve).
 *
 * `grab(rotate)` takes the frame of the moment (null if there is none) and
 * must not wait for anything. The hooks say what happens:
 *   onphase(phase)        "idle" | "starting" | "scanning" | "solving" | "done" | "failed"
 *   onscanstart(event)    the scan has begun ({steps, eta_ms})
 *   onphoto(index, face, image)  a frame was taken for that face
 *   onface(face, cells, source)  a face was read (face: 0..5 in URFDLB)
 *   onscanned(result)     the cube was put together (result of assembleFaces,
 *                         with analysis_ms and work_ms); a bad one ends the run
 *   onplan(plan), onmove(event), onop(event), ondone(event), onlog(text)
 *   onerror(error)        {code, message}: the run is over
 *   onlink(ok)            whether the server's events are reaching the page
 */
export class RobotRun {
  constructor({ api, n, grab, hooks = {}, now = () => performance.now(), later = (f) => setTimeout(f, 0), wait = 10, sleep }) {
    this.api = api;
    this.n = n;
    this.grab = grab;
    this.hooks = hooks;
    this.now = now;
    this.later = later;
    this.wait = wait;
    this.sleep = sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
    this.seq = 0;
    this.polling = false;
    this.abort = null;
    this.run = null;
    this.phase = "idle";
    this.held = []; // events that came while the run's id was not known yet
    this.linkOk = true;
  }

  #hook(name, ...args) {
    try {
      this.hooks[name]?.(...args);
    } catch (error) {
      // What the page does with an event never stops the run from being followed.
      console.error(error);
    }
  }

  #setPhase(phase) {
    this.phase = phase;
    this.#hook("onphase", phase);
  }

  get busy() {
    return this.phase === "starting" || this.phase === "scanning" || this.phase === "solving";
  }

  /** Begins following the server's events from now on (what came before is passed over). */
  async follow() {
    if (this.polling) return;
    this.polling = true;
    try {
      const { seq } = await this.api.events(0, 0);
      if (Number.isFinite(seq)) this.seq = seq;
    } catch {
      // The loop below says so if the server cannot be reached.
    }
    void this.#loop();
  }

  /** Stops following (on disconnect). */
  unfollow() {
    this.polling = false;
    this.abort?.abort();
  }

  async #loop() {
    let failures = 0;
    while (this.polling) {
      this.abort = new AbortController();
      try {
        const answer = await this.api.events(this.seq, this.wait, this.abort.signal);
        if (!this.polling) break;
        failures = 0;
        this.#link(true);
        for (const event of answer.events ?? []) {
          if (Number.isFinite(event.seq) && event.seq <= this.seq) continue;
          if (Number.isFinite(event.seq)) this.seq = event.seq;
          this.take(event);
        }
        if (Number.isFinite(answer.seq) && answer.seq > this.seq) this.seq = answer.seq;
      } catch (error) {
        if (!this.polling) break;
        failures++;
        if (failures > 1) this.#link(false);
        await this.sleep(Math.min(5000, 500 * failures));
      }
    }
  }

  #link(ok) {
    if (ok === this.linkOk) return;
    this.linkOk = ok;
    this.#hook("onlink", ok);
  }

  /** Starts a scan: the robot shows the camera the six faces. */
  async scan() {
    if (this.busy) return;
    this.faces = new Array(SIDES).fill(null);
    this.sources = new Array(SIDES).fill(null);
    this.taken = 0;
    this.read = 0;
    this.work = 0;
    this.grabMs = 0; // the longest a photo took to grab, before its photo-taken went
    this.scanDoneAt = null;
    this.scanMs = null;
    this.result = null;
    this.sent = false;
    this.held = [];
    this.run = null;
    this.#setPhase("starting");
    try {
      const { run } = await this.api.scan();
      this.run = run;
    } catch (error) {
      this.#fail(error);
      return;
    }
    this.#setPhase("scanning");
    const held = this.held.filter((e) => e.run === this.run);
    this.held = [];
    for (const event of held) this.take(event);
  }

  /** Emergency stop: the server stops the motors and ends the run with `error`. */
  async stop() {
    try {
      await this.api.stop();
    } catch (error) {
      this.#hook("onerror", { code: error.code ?? "network", message: `Stop was not received: ${error.message}` });
    }
  }

  /** One event of the server's stream. */
  take(event) {
    if (event.run !== undefined && event.run !== null) {
      if (this.phase === "starting" && this.run === null) {
        this.held.push(event);
        return;
      }
      if (event.run !== this.run) return;
    }
    switch (event.type) {
      case "scan-start": return this.#hook("onscanstart", event);
      case "photo": return this.#photo(event);
      case "scan-done":
        this.scanDoneAt = this.now();
        this.scanMs = event.scan_ms;
        return this.#maybeSolve();
      case "plan":
        if (this.phase !== "solving") return;
        return this.#hook("onplan", event);
      case "move": return this.#hook("onmove", event);
      case "op": return this.#hook("onop", event);
      case "done":
        this.#setPhase("done");
        return this.#hook("ondone", event);
      case "error":
        if (event.run !== undefined && event.run !== null && !this.busy) return;
        return this.#fail({ code: event.code, message: event.message });
      case "log": return this.#hook("onlog", String(event.text ?? ""));
      default:
    }
  }

  #fail(error) {
    if (error?.name === "AbortError") return;
    const wasBusy = this.busy;
    if (wasBusy) this.#setPhase("failed");
    this.#hook("onerror", { code: error.code ?? "error", message: error.message ?? String(error), during: wasBusy });
  }

  #photo(event) {
    if (this.phase !== "scanning") return;
    const index = event.index;
    const face = FACE_LETTERS.indexOf(event.face);
    const rotate = quarterTurn(event.rotate);
    if (face < 0 || rotate === null || !Number.isInteger(index)) {
      this.#fail({ code: "bad_photo", message: `The robot asked for a photo the page does not understand (face ${event.face}, rotate ${event.rotate}).` });
      void this.stop();
      return;
    }
    let image = null;
    const asked = this.now();
    try {
      image = this.grab(rotate);
    } catch {
      image = null;
    }
    this.grabMs = Math.max(this.grabMs, this.now() - asked);
    if (!image) {
      // No picture is no photo: the robot is stopped rather than left to wait.
      this.#fail({ code: "no_frame", message: "The camera gave no picture. Check that it is on and pointing at the cube." });
      void this.stop();
      return;
    }
    const run = this.run;
    this.api.photoTaken(run, index).catch((error) => {
      if (this.run === run && this.busy) this.#fail(error);
    });
    this.taken++;
    this.#hook("onphoto", index, face, image);
    // Read while the robot goes on to the next face.
    this.later(() => {
      if (this.run !== run) return;
      const began = this.now();
      const { cells, source } = readFace(image, this.n);
      this.work += this.now() - began;
      if (!this.faces[face]) this.read++;
      this.faces[face] = cells;
      this.sources[face] = source;
      this.#hook("onface", face, cells, source);
      this.#maybeAssemble();
    });
  }

  #maybeAssemble() {
    if (this.result || this.read < SIDES) return;
    const began = this.now();
    const result = assembleFaces(this.n, this.faces.flat());
    const ready = this.now();
    this.work += ready - began;
    result.readyAt = ready;
    result.samples = this.faces.flat();
    result.repeated = repeatedFaces(this.faces);
    result.sources = this.sources.slice();
    this.result = result;
    this.#maybeSolve();
  }

  #maybeSolve() {
    const result = this.result;
    if (!result || this.scanDoneAt === null || this.sent || this.phase !== "scanning") return;
    this.sent = true;
    // From the end of the scan to the facelets: what the reading added to the robot's time.
    result.analysis_ms = Math.max(0, Math.round(result.readyAt - this.scanDoneAt));
    result.work_ms = Math.round(this.work);
    result.grab_ms = Math.round(this.grabMs * 10) / 10;
    result.scan_ms = this.scanMs;
    this.#hook("onscanned", result);
    if (!result.ok) {
      // Nothing more is asked of the robot: the cube can be scanned again.
      this.#setPhase("idle");
      return;
    }
    this.#setPhase("solving");
    const run = this.run;
    this.api.solve(run, this.n, result.state, result.analysis_ms).catch((error) => {
      if (this.run === run && this.busy) this.#fail(error);
    });
  }
}

// ------------------------------------------------------------- the page

const CAMERA_CHOICE = "anycube.robot-camera";

const ICON = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4.5" y="8" width="15" height="11" rx="3"/><path d="M12 8V4.5M9.5 13.5v.5M14.5 13.5v.5M2 12.5v3M22 12.5v3"/></svg>';

const PANEL = `
  <div class="robot-head">
    <p class="robot-name"><i aria-hidden="true"></i><b id="robot-name"></b><span id="robot-where"></span></p>
    <button type="button" class="button quiet small" id="robot-disconnect">Disconnect</button>
  </div>
  <div class="robot-body">
    <div class="robot-cam" id="robot-cam">
      <video playsinline muted autoplay></video>
      <div class="robot-grid" aria-hidden="true"></div>
      <button type="button" class="button small" id="robot-camera-start" hidden>Start the camera</button>
    </div>
    <div class="robot-side">
      <ol class="robot-shots" id="robot-shots" aria-label="Photos of this scan"></ol>
      <label class="robot-camera"><span class="sr">Camera</span><select id="robot-camera" title="The camera that sees the cube"></select></label>
      <p class="robot-note" id="robot-note" role="status"></p>
    </div>
  </div>`;

const DIALOG = `
  <h2 id="robot-title">Robot</h2>
  <div class="robot-fields">
    <label>Robot <select id="robot-driver"></select></label>
    <label>Port <select id="robot-port"></select></label>
  </div>
  <p id="robot-dialog-status" role="status"></p>
  <form method="dialog">
    <button type="button" class="button quiet" id="robot-refresh">Look again</button>
    <button class="button">Close</button>
    <button type="button" class="button primary" id="robot-connect">Connect</button>
  </form>`;

/** Milliseconds as people read them: 840 ms, 12.4 s, 1:05. */
export function duration(ms) {
  if (!Number.isFinite(ms)) return "–";
  if (ms < 1000) return `${Math.max(0, Math.round(ms))} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/**
 * The robot on the page. `host` is what app.js lends it (robotHost there):
 * its elements, view, player and state, and a few things to do with them.
 */
export function mountRobot(host, { api = new RobotApi() } = {}) {
  const { root, $, el, view } = host;
  const remember = (key, value) => {
    try {
      localStorage.setItem(key, value);
    } catch {
      // No storage: the choice lasts for this visit.
    }
  };
  const recall = (key) => {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  };

  // The button by the cube that opens the robot's dialog.
  const open = el("button", { type: "button", class: "tool", id: "robot-open", "aria-haspopup": "dialog", "aria-controls": "robot-dialog", "aria-pressed": "false", title: "Connect a robot" });
  open.innerHTML = `${ICON}<span>Robot</span>`;
  $("open-settings").before(open);

  const dialog = el("dialog", { class: "app-dialog robot-dialog", id: "robot-dialog", "aria-labelledby": "robot-title" });
  dialog.innerHTML = DIALOG;
  root.append(dialog);

  const panel = el("div", { class: "robot-panel", id: "robot-panel", role: "group", "aria-label": "Robot" });
  panel.innerHTML = PANEL;
  root.querySelector(".panels").after(panel);

  // The panel lies over the tabs and what they open, which keep their place.
  const tabs = root.querySelector(".tabs");
  const panels = root.querySelector(".panels");
  const place = () => {
    if (!tabs.offsetParent) return;
    panel.style.top = `${tabs.offsetTop}px`;
    panel.style.height = `${panels.offsetTop + panels.offsetHeight - tabs.offsetTop}px`;
  };
  new ResizeObserver(place).observe(root.querySelector(".panel"));

  const scanButton = el("button", { type: "button", class: "button primary large", id: "robot-scan", text: "Scan" });
  $("solve").after(scanButton);

  const video = panel.querySelector("video");
  video.muted = true;
  const feed = new CameraFeed(video);
  const scratch = document.createElement("canvas");
  const thumb = document.createElement("canvas");
  const note = $("robot-note");
  const say = (text, warn = false, more = "") => {
    note.textContent = text;
    note.title = more;
    note.classList.toggle("warn", warn);
  };

  let drivers = [];
  let connected = null; // {driver, port, size, info}
  let robot = null;
  let raw = null;
  let lastFace = -1;
  let following = false; // the page can animate the plan's moves
  let eta = null;
  let ticker = 0;

  // ------------------------------------------------------------ dialog

  function fillPorts() {
    const driver = drivers.find((d) => d.id === $("robot-driver").value);
    const ports = driver?.ports ?? [];
    $("robot-port").replaceChildren(
      el("option", { value: "", text: ports.length ? "The first one found" : "None found: try anyway" }),
      ...ports.map((p) => el("option", { value: p, text: p })),
    );
    if (ports.length) $("robot-port").value = ports[0];
  }

  function showDialog() {
    const on = !!connected;
    $("robot-driver").disabled = $("robot-port").disabled = on || !drivers.length;
    $("robot-refresh").hidden = on;
    const connect = $("robot-connect");
    connect.textContent = on ? "Disconnect" : "Connect";
    connect.classList.toggle("primary", !on);
    connect.disabled = on ? !!robot?.busy : !drivers.length;
    if (on) {
      $("robot-driver").replaceChildren(el("option", { text: driverName(connected.driver) }));
      $("robot-port").replaceChildren(el("option", { text: connected.port || "–" }));
    }
  }

  const driverName = (id) => drivers.find((d) => d.id === id)?.name ?? id;

  async function lookForRobots() {
    const status = $("robot-dialog-status");
    status.textContent = "Looking for robots…";
    try {
      const answer = await api.robots();
      drivers = Array.isArray(answer.drivers) ? answer.drivers : [];
      $("robot-driver").replaceChildren(...drivers.map((d) => el("option", { value: d.id, text: `${d.name}${d.sizes?.length ? ` (${d.sizes.map(host.name).join(", ")})` : ""}` })));
      fillPorts();
      status.textContent = drivers.length ? "" : "This server has no robot drivers.";
      return answer.connected ?? null;
    } catch (error) {
      status.textContent = `Could not ask the server about robots: ${error.message}`;
      return null;
    } finally {
      showDialog();
    }
  }

  open.addEventListener("click", async () => {
    dialog.showModal();
    if (!connected) await lookForRobots();
    else showDialog();
  });
  $("robot-driver").addEventListener("change", fillPorts);
  $("robot-refresh").addEventListener("click", lookForRobots);
  $("robot-connect").addEventListener("click", async () => {
    const status = $("robot-dialog-status");
    if (connected) {
      if (await disconnect()) dialog.close();
      else status.textContent = note.textContent;
      return;
    }
    const driver = $("robot-driver").value;
    const port = $("robot-port").value || undefined;
    $("robot-connect").disabled = true;
    status.textContent = "Connecting… The first time a size is used its solver tables are loaded, which can take a while.";
    try {
      const answer = await api.connect(driver, port);
      status.textContent = "";
      dialog.close();
      await enter({ driver, port: answer.port ?? port ?? "", size: answer.size, info: answer.info ?? {} });
    } catch (error) {
      status.textContent = `Not connected: ${error.message}`;
    } finally {
      showDialog();
    }
  });

  // ------------------------------------------------------------ camera

  async function openCamera(deviceId = recall(CAMERA_CHOICE)) {
    const start = $("robot-camera-start");
    try {
      try {
        await feed.open(deviceId ? { deviceId: { exact: deviceId } } : undefined);
      } catch (error) {
        // The camera remembered is not there today (the phone is away): any other.
        if (!deviceId || !["OverconstrainedError", "NotFoundError", "NotReadableError"].includes(error.name)) throw error;
        await feed.open();
      }
    } catch (error) {
      start.hidden = false;
      say(error.name === "NotAllowedError" ? "The browser did not allow the camera. Allow it for this page, then start it." : "The camera could not be started.", true);
      return false;
    }
    start.hidden = true;
    const select = $("robot-camera");
    select.replaceChildren(...feed.cameras.map((c, k) => el("option", { value: c.deviceId, text: c.label || `Camera ${k + 1}` })));
    select.value = feed.deviceId ?? "";
    if (select.selectedIndex < 0) select.selectedIndex = 0;
    select.hidden = feed.cameras.length < 2;
    if (note.classList.contains("warn")) say("");
    return true;
  }

  $("robot-camera").addEventListener("change", async (e) => {
    const id = e.target.value;
    if (await openCamera(id)) remember(CAMERA_CHOICE, feed.deviceId ?? id);
  });
  $("robot-camera-start").addEventListener("click", () => openCamera());

  function drawGrid(n) {
    const grid = panel.querySelector(".robot-grid");
    grid.style.setProperty("--n", n);
    grid.replaceChildren(...Array.from({ length: n * n }, () => el("i")));
  }

  // ------------------------------------------------------------ shots

  function clearShots() {
    $("robot-shots").replaceChildren(
      ...[..."URFDLB"].map((f) => el("li", { "data-face": f, title: `${f}: not taken yet` }, [el("canvas", { width: "56", height: "56" }), el("span", { text: f })])),
    );
  }

  function drawShot(face, image) {
    const li = $("robot-shots").children[face];
    if (!li) return;
    const canvas = li.querySelector("canvas");
    thumb.width = image.width;
    thumb.height = image.height;
    thumb.getContext("2d").putImageData(new ImageData(image.data, image.width, image.height), 0, 0);
    canvas.getContext("2d").drawImage(thumb, 0, 0, canvas.width, canvas.height);
    li.classList.add("taken");
    li.title = `${FACE_LETTERS[face]}: as the page read it`;
  }

  // ------------------------------------------------------------ status

  function setButtons() {
    const busy = !!robot?.busy;
    scanButton.textContent = busy ? "Stop" : "Scan";
    scanButton.classList.toggle("primary", !busy);
    scanButton.classList.toggle("danger", busy);
    scanButton.title = busy ? "Stop the robot at once" : "Scan the cube in the robot, then solve it";
    scanButton.disabled = !robot;
    $("robot-disconnect").disabled = busy;
  }

  function stopTicker() {
    clearInterval(ticker);
    ticker = 0;
  }

  const hooks = {
    onphase(phase) {
      setButtons();
      if (phase === "starting") host.say("busy", "Starting the scan…");
    },
    onscanstart(event) {
      host.wait("Scanning", event.eta_ms ? `About ${duration(event.eta_ms)} for the six photos.` : "");
    },
    onphoto(index, face, image) {
      lastFace = face;
      host.say("busy", ["Scanning · photo ", el("b", { text: String(index + 1) }), " of 6"]);
      requestAnimationFrame(() => drawShot(face, image));
    },
    onface(face, cells) {
      const nn = host.state.size * host.state.size;
      if (!raw || raw.length !== 6 * nn) return;
      for (let i = 0; i < nn; i++) raw[face * nn + i] = `rgb(${cells[i].map(Math.round).join(",")})`;
      view.setRaw(raw);
    },
    onscanned(result) {
      host.wait(null);
      const { ok, note: said } = host.scanned(result, result.sources, lastFace);
      const found = result.sources.filter((s) => s?.found).length;
      const read = `${found} of 6 faces found · ready ${duration(result.analysis_ms)} after the scan`;
      const more = `Reading the six photos took ${duration(result.work_ms)} in all, while the robot moved; each photo was grabbed within ${duration(result.grab_ms)}.`;
      if (ok && result.ok) {
        say(read, false, more);
        host.say("busy", "Finding the robot's fastest solution…");
      } else {
        const twice = result.repeated?.[0];
        if (twice) {
          // The likeliest cause of all, said first.
          const [a, b] = twice.map((f) => FACE_LETTERS[f]);
          host.say("bad", [el("b", { text: `The photos of ${a} and ${b} are the same picture. ` }), "The camera was still showing one face when the robot asked for the next: the robot has to wait longer before each photo."], "Nothing more was asked of the robot.");
          say(read, true, more);
          return;
        }
        say(read, true, more);
        host.say("bad", [el("b", { text: "The scan did not make a cube. " }), said], "Nothing more was asked of the robot: press Scan to try again.");
      }
    },
    onplan(plan) {
      following = host.plan(plan.moves ?? []);
      eta = new Eta(plan, performance.now());
      const left = el("b");
      const more = `${host.count(plan.moves?.length ?? 0, "move")} · found in ${duration(plan.compute_ms)}${plan.candidates ? ` among ${plan.candidates}` : ""}${following ? "" : " · the page could not follow these moves on its cube"}`;
      const tick = () => {
        left.textContent = duration(eta.left(performance.now()));
      };
      tick();
      host.say("busy", ["The robot is solving · ", left, " left"], more);
      stopTicker();
      ticker = setInterval(tick, 100);
    },
    onmove(event) {
      if (following) host.move(event.move, Number(event.est_ms) || 300);
    },
    onop(event) {
      eta?.op(event.i, Number(event.actual_ms), performance.now());
    },
    ondone(event) {
      stopTicker();
      const t = event.times ?? {};
      const times = el("dl", { class: "robot-times" }, [
        ["Scan", t.scan_ms], ["Analysis", t.analysis_ms], ["Compute", t.compute_ms], ["Execution", t.exec_ms], ["Total", t.total_ms],
      ].map(([label, ms]) => el("div", {}, [el("dt", { text: label }), el("dd", { text: duration(ms) })])));
      host.say("good", [el("b", { text: "Solved by the robot" }), ` in ${duration(t.total_ms)}`], "", times, true);
    },
    onerror(error) {
      stopTicker();
      host.wait(null);
      host.player.stop();
      host.player.onchange();
      const stopped = error.code === "stopped";
      const message = error.message || error.code;
      const text = stopped
        ? [el("b", { text: "Stopped. " }), /^stopped\.?$/i.test(message.trim()) ? "The motors are off." : message]
        : [el("b", { text: "The robot stopped. " }), message];
      host.say("bad", text, stopped ? "Put the robot back in its rest position before the next scan." : `(${error.code})`, null, true);
      setButtons();
    },
    onlog(text) {
      if (text) say(text);
    },
    onlink(ok) {
      say(ok ? "" : "No answer from the anycube server. Trying again…", !ok);
    },
  };

  scanButton.addEventListener("click", async () => {
    if (!robot) return;
    if (robot.busy) {
      scanButton.disabled = true;
      await robot.stop();
      setButtons();
      return;
    }
    if (!feed.running && !(await openCamera())) return;
    const n = connected.size;
    host.setSize(n);
    robot.n = n;
    raw = host.scanStart();
    lastFace = -1;
    clearShots();
    drawGrid(n);
    say("");
    await robot.scan();
  });

  // ------------------------------------------------------------ connected

  async function enter(info) {
    connected = info;
    open.setAttribute("aria-pressed", "true");
    open.title = `${driverName(info.driver)} connected`;
    $("robot-name").textContent = driverName(info.driver);
    $("robot-where").textContent = [Number.isInteger(info.size) ? host.name(info.size) : "", info.port].filter(Boolean).join(" · ");
    if (Number.isInteger(info.size)) host.setSize(info.size);
    host.setMode(true);
    place();
    clearShots();
    drawGrid(info.size ?? 3);
    robot = new RobotRun({
      api,
      n: info.size ?? 3,
      grab: (rotate) => grabFrame(video, rotate, robot.n, scratch),
      hooks,
    });
    setButtons();
    say("Put a cube in the robot, then press Scan.");
    void robot.follow();
    await openCamera();
    setTimeout(() => warmUp(robot?.n ?? 3), 300);
  }

  // The first reading in a page is several times slower than the next
  // (measured in Chromium: a grab 22 ms against 1, six faces 180 ms against
  // 50): done once on connecting, so that the first scan does not pay it.
  function warmUp(n) {
    try {
      const frame = grabFrame(video, 90, n, scratch) ?? { width: frameSize(n), height: frameSize(n), data: new Uint8ClampedArray(frameSize(n) ** 2 * 4) };
      readFace(frame, n);
      const rgb = STICKERS.map((hex) => [1, 3, 5].map((k) => parseInt(hex.slice(k, k + 2), 16)));
      assembleFaces(n, Array.from(new CubeModel(n).solved(), (k) => rgb[k]));
    } catch {
      // Only time is lost.
    }
  }

  function leave() {
    stopTicker();
    robot?.unfollow();
    robot = null;
    connected = null;
    feed.close();
    open.setAttribute("aria-pressed", "false");
    open.title = "Connect a robot";
    host.setMode(false);
    setButtons();
  }

  async function disconnect() {
    try {
      await api.disconnect();
    } catch (error) {
      say(`Could not disconnect: ${error.message}`, true);
      return false;
    }
    leave();
    return true;
  }
  $("robot-disconnect").addEventListener("click", disconnect);

  // A robot left connected (the page was reloaded): straight back to it.
  setButtons();
  void lookForRobots().then((was) => {
    if (was && !connected) enter(was);
  });

  return { api, get robot() { return robot; }, get connected() { return connected; } };
}
