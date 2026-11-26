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
 * The least a frame is read at, in pixels a side: what MultiCuber's bridge
 * gives anycube is 320 (480 from 6x6x6); at 240 a sticker of a 5x5x5 the
 * robot holds is still some 35 pixels across, and a face is read in about
 * 8 ms. The last face of a scan is read about this small, since the robot
 * waits for nothing else by then.
 */
export const frameSize = (n) => (n >= 6 ? 360 : 240);

// The sizes a frame may be read at. The top one is the square of the 1280
// by 720 the camera is asked for, and where the robot's own pictures of a
// 4x4x4 (MultiCuber/CubeScan, 1080 pixels) have every sticker of every face
// found; between 320 and 560 a sticker or two of a face is filled in from
// the lattice, above 720 only the time grows. A face is read in about 9,
// 15, 35, 55 and 65 ms at 240, 320, 480, 640 and 720 (150 at 1080).
const READ_SIZES = [240, 320, 360, 400, 480, 560, 640, 720];

/**
 * What reading a picture of `size` pixels a side costs, up to the machine's
 * own rate: less than its area, since the stickers do not grow in number.
 * Measured in Chromium on a 3x3x3 and the robot's 4x4x4 between 240 and
 * 720: the times go as the side to the power 1.5, within a tenth.
 */
export const readCost = (size) => size ** 1.5;

/** The sizes a face of an n-cube may be read at, least first. */
export const readSizes = (n) => READ_SIZES.filter((s) => s >= frameSize(n));

/**
 * How large each photo of a scan is read, so that reading it is over well
 * before the robot asks for the next: the robot takes about a second from
 * photo to photo and the page reads meanwhile, so the time goes to a larger
 * picture, not to waiting.
 *
 * What a size costs is measured, not assumed: the time of every reading
 * gives the machine's rate (ms per `readCost`), taken at once when a
 * reading was slower and by halves when it was quicker. Only a face
 * of the cube says what a face costs (a noisy picture of a 3x3x3 several
 * times an even one), so the very first photo is read at `first`, a size
 * in the middle, and the rate is kept from scan to scan. What there
 * is to spend is `share` of the time to the next photo: the server's own
 * word for it (`next_ms` of the photo's event) when it gives one, else the
 * shortest time between two photos seen in this scan or the one before
 * (`gap` until any was seen). The largest size that fits is read; a size
 * down only when it would not. After the last photo the robot waits for
 * the cube alone, so that one gets `last` ms (more only if the server says
 * the robot is busy for longer).
 */
export class ReadPace {
  constructor(n, { share = 0.5, gap = 800, last = 25, first = 480 } = {}) {
    this.sizes = readSizes(n);
    this.first = first;
    this.share = share;
    this.gap = gap;
    this.last = last;
    this.rate = null; // ms per readCost, as measured
    this.seen = []; // the times between this scan's photos
    this.at = null;
  }

  /** A scan begins (of an n-cube): what the last one's photos took from one to the next is what to expect. */
  begin(n) {
    if (n) this.sizes = readSizes(n);
    if (this.seen.length) this.gap = Math.min(...this.seen);
    this.seen = [];
    this.at = null;
  }

  /** The time there is to read the photo asked for at `at`. `final`: the scan ends after it. */
  budget(at, final = false, nextMs = undefined) {
    if (this.at !== null && at > this.at) this.seen.push(at - this.at);
    this.at = at;
    // After the last photo there is `last` at least: the robot's own time to the end of the scan is about nothing.
    if (Number.isFinite(nextMs) && nextMs >= 0) return final ? Math.max(this.last, this.share * nextMs) : this.share * nextMs;
    return final ? this.last : this.share * Math.min(this.gap, ...this.seen);
  }

  /** The largest size read within `ms`; before anything was measured, `first` (the least for a last photo). */
  fit(ms, final = false) {
    if (this.rate === null) return final ? this.sizes[0] : (this.sizes.findLast((size) => size <= this.first) ?? this.sizes[0]);
    let best = this.sizes[0];
    for (const size of this.sizes) if (this.rate * readCost(size) <= ms) best = size;
    return best;
  }

  /** The size to read the photo asked for at `at` (the server's t_ms, or the page's clock). */
  photo(at, final = false, nextMs = undefined) {
    return this.fit(this.budget(at, final, nextMs), final);
  }

  /** A picture of `size` pixels a side took `ms` to read. */
  took(size, ms) {
    if (!(size > 0) || !Number.isFinite(ms)) return;
    const rate = ms / readCost(size);
    this.rate = this.rate === null || rate > this.rate ? rate : (this.rate + rate) / 2;
  }
}

/**
 * The frame of the moment from a playing video: its centred square, made
 * `size` pixels a side (no larger than the camera gives it), turned
 * `rotate` degrees clockwise. Null while there is no frame. A few
 * milliseconds: it is what stands between the robot's `photo` and the
 * page's answer.
 */
export function grabFrame(video, rotate, size, canvas) {
  const [w, h] = [video.videoWidth, video.videoHeight];
  if (!w || !h || video.readyState < 2) return null;
  const { sx, sy, side } = squareCrop(w, h);
  size = Math.min(side, size);
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

  robots(probe = true) { return this.#request("GET", probe ? "/api/robots" : "/api/robots?probe=0"); }
  connect(driver, port, size) {
    return this.#request("POST", "/api/robot/connect", { driver, ...(port ? { port } : {}), ...(Number.isInteger(size) ? { size } : {}) });
  }
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

// ------------------------------------------------------ plug and play

/** Every serial port a /api/robots answer names, whoever claims it. */
export function portsListed(answer) {
  const paths = new Set();
  for (const d of answer?.drivers ?? []) for (const p of d.ports ?? []) if (typeof p === "string") paths.add(p);
  for (const p of answer?.ports ?? []) if (typeof p?.path === "string") paths.add(p.path);
  return paths;
}

/**
 * The robot to connect without being asked, from the server's /api/robots:
 * `{driver, port, sure}` or null. `last` is the robot connected last time
 * (`{driver, port}`), `skip(driver, port)` says which not to try.
 *
 * A server that identifies its boards says so port by port, in `ports`
 * (`{path, driver, busy}`): a port with a `driver` is that robot for sure;
 * one with `driver: null` answered as no robot and one that is `busy` is
 * somebody else's, and neither is touched. A port the server says nothing
 * of (an older server says nothing of any) goes by its name alone, as the
 * drivers claim it in their own `ports`: to the driver of last time if it
 * claims it, else to the only driver that does; claimed by several and none
 * of them the last one, it waits to be chosen by hand.
 */
export function choosePort(answer, last = null, skip = () => false) {
  const drivers = Array.isArray(answer?.drivers) ? answer.drivers.filter((d) => d && typeof d.id === "string") : [];
  const known = new Set(drivers.map((d) => d.id));
  const said = new Map();
  for (const p of Array.isArray(answer?.ports) ? answer.ports : []) if (typeof p?.path === "string") said.set(p.path, p);
  const found = [];
  for (const p of said.values()) {
    if (!p.busy && typeof p.driver === "string" && known.has(p.driver)) found.push({ driver: p.driver, port: p.path, sure: true });
  }
  const claims = new Map();
  for (const d of drivers) {
    for (const path of d.ports ?? []) {
      if (typeof path !== "string") continue;
      const p = said.get(path);
      if (p && (p.busy || "driver" in p)) continue;
      claims.set(path, [...(claims.get(path) ?? []), d.id]);
    }
  }
  for (const [port, ids] of claims) {
    const driver = ids.includes(last?.driver) ? last.driver : ids.length === 1 ? ids[0] : null;
    if (driver) found.push({ driver, port, sure: false });
  }
  // The identified before the named, and among each the robot of last time first.
  const rank = (c) => (c.sure ? 0 : 2) + (last && c.driver === last.driver && c.port === last.port ? 0 : 1);
  return found.filter((c) => !skip(c.driver, c.port)).sort((a, b) => rank(a) - rank(b))[0] ?? null;
}

const pair = (driver, port) => `${driver} ${port}`;
const ANY = "*"; // a robot whose port is not known: any port of its driver
// A robot that does not connect is tried again after this long, then twice
// as long, and after the last of these left alone until it is plugged in anew.
const RETRY_MS = [4000, 8000, 16000, 32000];

/**
 * Watches the server for a robot being plugged in and connects it: asks
 * /api/robots every `every` ms for as long as it is started (the page
 * starts it while no robot is connected and stops it when one is).
 *
 * `memory` keeps `auto` (connect without being asked) and `last` (the robot
 * connected last, `{driver, port}`) on the device. A robot the server
 * already holds is taken up whatever `auto` says. `size(driver)` is the
 * cube the robot is connected for (nothing: the robot's own). The hooks:
 *   onlist(answer)        what the server answered (null and the error if it did not)
 *   onconnecting(robot)   {driver, port}: being connected now
 *   onconnected(robot, how)  {driver, port, size, info}; how: "plugged" | "held"
 *   onfailed(robot, error)   it did not connect; `error.final` when it is given up
 */
export class RobotWatch {
  constructor({ api, memory, hooks = {}, every = 2000, size = () => undefined, now = () => Date.now(), sleep }) {
    this.api = api;
    this.memory = memory;
    this.size = size;
    this.hooks = hooks;
    this.every = every;
    this.now = now;
    this.sleep = sleep ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
    this.running = false;
    this.turn = 0;
    this.looking = null;
    this.held = new Set(); // disconnected by hand: not until plugged in anew
    this.failed = new Map(); // did not connect: {tries, until}
  }

  #hook(name, ...args) {
    try {
      this.hooks[name]?.(...args);
    } catch (error) {
      console.error(error);
    }
  }

  /** Looks now, and goes on looking for as long as robots are to be connected by themselves. */
  start() {
    if (this.running) return;
    this.running = true;
    const turn = ++this.turn;
    void (async () => {
      while (this.running && turn === this.turn) {
        await this.look();
        if (!this.memory.auto) break; // one look, for a robot the server holds already
        if (!this.running || turn !== this.turn) return;
        await this.sleep(this.every);
      }
      if (turn === this.turn) this.running = false;
    })();
  }

  stop() {
    this.running = false;
    this.turn++;
  }

  /** This robot was disconnected by hand: it is left alone until it is plugged in again. */
  hold(driver, port) {
    this.held.add(pair(driver, port || ANY));
  }

  /** Forgets what was held back or given up (the choice was made by hand). */
  release(driver, port) {
    for (const key of [pair(driver, port), pair(driver, ANY)]) {
      this.held.delete(key);
      this.failed.delete(key);
    }
  }

  /** One look at the server, one at a time. Says what came of it. */
  look() {
    this.looking ??= this.#look().finally(() => {
      this.looking = null;
    });
    return this.looking;
  }

  async #look() {
    const turn = this.turn;
    let answer;
    try {
      answer = await this.api.robots();
    } catch (error) {
      this.#hook("onlist", null, error);
      return "unreachable";
    }
    const mine = () => turn === this.turn; // not stopped since
    this.#hook("onlist", answer);
    // What has been unplugged is new when it comes back.
    const there = portsListed(answer);
    const gone = (key) => (key.endsWith(` ${ANY}`) ? !there.size : !there.has(key.slice(key.indexOf(" ") + 1)));
    for (const key of [...this.held]) if (gone(key)) this.held.delete(key);
    for (const key of [...this.failed.keys()]) if (gone(key)) this.failed.delete(key);
    if (answer.connected) {
      if (!mine()) return "stopped";
      this.stop();
      this.#hook("onconnected", answer.connected, "held");
      return "connected";
    }
    if (!mine() || !this.running || !this.memory.auto) return "nothing";
    const now = this.now();
    const robot = choosePort(answer, this.memory.last, (driver, port) => {
      const key = pair(driver, port);
      const wait = Math.max(this.failed.get(key)?.until ?? 0, this.failed.get(pair(driver, ANY))?.until ?? 0);
      return this.held.has(key) || this.held.has(pair(driver, ANY)) || wait > now;
    });
    if (!robot) return "nothing";
    this.#hook("onconnecting", robot);
    const key = pair(robot.driver, robot.port);
    try {
      const got = await this.api.connect(robot.driver, robot.port, this.size(robot.driver));
      this.failed.delete(key);
      this.memory.last = { driver: robot.driver, port: robot.port };
      // Stopped meanwhile or not, the server holds the robot now: the page is told.
      this.stop();
      this.#hook("onconnected", { driver: robot.driver, port: got.port ?? robot.port, size: got.size, info: got.info ?? {} }, "plugged");
      return "connected";
    } catch (error) {
      this.#hook("onfailed", robot, Object.assign(error, { final: this.missed(robot.driver, robot.port) }));
      return "failed";
    }
  }

  /** This robot did not connect: it waits before the next try. True when it is given up until plugged in anew. */
  missed(driver, port) {
    const key = pair(driver, port || ANY);
    const tries = (this.failed.get(key)?.tries ?? 0) + 1;
    const final = tries > RETRY_MS.length;
    this.failed.set(key, { tries, until: final ? Infinity : this.now() + RETRY_MS[tries - 1] });
    return final;
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
// The codes of an `error` that say the robot itself is gone, not only the run.
const LOST = new Set(["disconnected", "not_connected", "connection_lost", "port_lost"]);

/**
 * A connected robot: the events of the server followed for as long as it
 * is connected, and one run at a time (scan, then solve).
 *
 * `grab(rotate, size)` takes the frame of the moment, `size` pixels a side
 * (null if there is none), and must not wait for anything. How large each
 * photo is read is the `pace`'s to say (ReadPace). The hooks say what happens:
 *   onphase(phase)        "idle" | "starting" | "scanning" | "solving" | "done" | "failed"
 *   onscanstart(event)    the scan has begun ({steps, eta_ms})
 *   onphoto(index, face, image, event)  a frame was taken for that face
 *   onface(face, cells, source)  a face was read (face: 0..5 in URFDLB)
 *   onscanned(result)     the cube was put together (result of assembleFaces,
 *                         with analysis_ms, work_ms and, photo by photo,
 *                         sizes and read_ms); a bad one ends the run
 *   onplan(plan), onmove(event), onop(event), ondone(event), onlog(text)
 *   onerror(error)        {code, message}: the run is over
 *   onlost(event)         the server says the robot is gone (unplugged)
 *   onlink(ok)            whether the server's events are reaching the page
 */
export class RobotRun {
  constructor({ api, n, grab, hooks = {}, now = () => performance.now(), later = (f) => setTimeout(f, 0), wait = 10, sleep, pace }) {
    this.api = api;
    this.n = n;
    this.pace = pace ?? new ReadPace(n);
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
    this.sizes = new Array(SIDES).fill(null); // photo by photo: the size read at, and how long it took
    this.readMs = new Array(SIDES).fill(null);
    this.pace.begin(this.n);
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
        this.#fail({ code: event.code, message: event.message });
        if (LOST.has(event.code)) this.#hook("onlost", event);
        return;
      case "disconnected":
        if (this.busy) this.#fail({ code: "disconnected", message: event.message ?? "The robot is no longer connected." });
        return this.#hook("onlost", event);
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
    // Every photo but the last has the robot's way to the next to be read in.
    const size = this.pace.photo(Number.isFinite(event.t_ms) ? event.t_ms : asked, index >= SIDES - 1, event.next_ms);
    try {
      image = this.grab(rotate, size);
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
    this.#hook("onphoto", index, face, image, event);
    // Read while the robot goes on to the next face.
    this.later(() => {
      if (this.run !== run) return;
      const began = this.now();
      const { cells, source } = readFace(image, this.n);
      const took = this.now() - began;
      this.work += took;
      this.pace.took(image.width, took);
      if (index >= 0 && index < SIDES) [this.sizes[index], this.readMs[index]] = [image.width, took];
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
    result.sizes = this.sizes.slice();
    result.read_ms = this.readMs.map((ms) => (ms === null ? null : Math.round(ms * 10) / 10));
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
const LAST_ROBOT = "anycube.robot-last";
const AUTO_CONNECT = "anycube.robot-auto";

/** A serial port as it is shown: without the /dev/ every one of them begins with. */
export const portName = (port) => String(port ?? "").replace(/^\/dev\//, "");

const ICON = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4.5" y="8" width="15" height="11" rx="3"/><path d="M12 8V4.5M9.5 13.5v.5M14.5 13.5v.5M2 12.5v3M22 12.5v3"/></svg>';

/**
 * The robot picker's choices from the server's /api/robots: every port it
 * names, in its order, and the port held now (`active`, `{driver, port}`)
 * even if the server no longer lists it. Each is `{path, driver, state,
 * text, current}`: `driver` is the robot the port would be connected as
 * (none: it cannot be picked), `state` what stands in the way or is still
 * open ("busy", "not a robot", "checking"), `text` what the picker shows,
 * as "MultiCuber · cu.usbmodem21301" or "Simulated MultiCuber".
 */
export function robotChoices(answer, active = null, last = null) {
  const drivers = Array.isArray(answer?.drivers) ? answer.drivers.filter((d) => d && typeof d.id === "string") : [];
  const said = new Map((Array.isArray(answer?.ports) ? answer.ports : []).filter((p) => typeof p?.path === "string").map((p) => [p.path, p]));
  const paths = [...portsListed(answer)];
  if (active && !paths.includes(active.port ?? "")) paths.push(active.port ?? "");
  return paths.map((path) => {
    const p = said.get(path);
    const current = !!active && path === (active.port ?? "");
    const driver = current ? active.driver : (choosePort(answer, last, (_, port) => port !== path)?.driver ?? null);
    // Only a server that tells of its ports one by one can be still asking one.
    const state = current ? "" : p?.busy ? "busy" : p && p.driver === null ? "not a robot" : p && !("driver" in p) ? "checking" : "";
    const label = drivers.find((d) => d.id === driver)?.name ?? driver;
    const name = path === "sim" && label ? `Simulated ${label}` : [label, portName(path)].filter(Boolean).join(" · ") || "Robot";
    return { path, driver, state, current, text: [name, state].filter(Boolean).join(" · ") };
  });
}

/**
 * How the photos of a scan were read, as the details list it: rows of
 * `[label, value]` from what RobotRun's `onscanned` gives.
 */
export function scanMeasures(result) {
  const found = (result.sources ?? []).filter((s) => s?.found).length;
  const rows = [["Faces found", `${found} of ${SIDES}`]];
  const sizes = (result.sizes ?? []).filter(Boolean);
  const times = (result.read_ms ?? []).filter((ms) => ms !== null).map(Math.round);
  const span = (list, unit) => (Math.min(...list) === Math.max(...list) ? `${list[0]} ${unit}` : `${Math.min(...list)} to ${Math.max(...list)} ${unit}`);
  if (sizes.length) rows.push(["Photos read at", `${span(sizes, "px")} a side`]);
  if (times.length) rows.push(["Reading a face", span(times, "ms")]);
  if (Number.isFinite(result.work_ms)) rows.push(["Reading in all", duration(result.work_ms)]);
  if (Number.isFinite(result.grab_ms)) rows.push(["Slowest photo grab", duration(result.grab_ms)]);
  if (Number.isFinite(result.analysis_ms)) rows.push(["Ready after the scan", duration(result.analysis_ms)]);
  return rows;
}

/**
 * The "i" and what it opens: shown while a mouse is over either, and
 * pinned or let go by a click, a tap or the keyboard; Escape, and a click
 * or the focus going anywhere else, close it. Returns what closes it.
 */
export function bindInfo(button, details, doc) {
  let pinned = false;
  const area = button.parentElement;
  function show(open) {
    details.hidden = !open;
    button.setAttribute("aria-expanded", String(open));
    if (!open) pinned = false;
  }
  area.addEventListener("pointerenter", (e) => {
    if (e.pointerType === "mouse") show(true);
  });
  area.addEventListener("pointerleave", (e) => {
    if (e.pointerType === "mouse" && !pinned) show(false);
  });
  button.addEventListener("click", () => {
    // A mouse that opened it by coming near pins it with its first click.
    const open = !pinned;
    pinned = open;
    show(open);
  });
  doc.addEventListener("pointerdown", (e) => {
    if (!area.contains(e.target)) show(false);
  });
  doc.addEventListener("focusin", (e) => {
    if (!area.contains(e.target)) show(false);
  });
  doc.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || details.hidden) return;
    show(false);
    // The dialog or whatever else listens for Escape is not closed with it.
    e.stopPropagation?.();
  });
  return () => show(false);
}

// Each picker is the browser's own select, with its keyboard and its list,
// laid unseen over the chosen name: a select shows its name on one line and
// cuts it, the name under it wraps and is whole.
const PANEL = `
  <div class="robot-row">
    <label for="robot-device">Robot</label>
    <span class="robot-state" id="robot-state" role="status"></span>
  </div>
  <div class="robot-choice">
    <span aria-hidden="true" id="robot-device-name"></span>
    <select id="robot-device"></select>
  </div>
  <div class="robot-row">
    <label for="robot-camera">Camera</label>
    <div class="robot-info">
      <button type="button" id="robot-info" aria-label="Robot and scan details" aria-controls="robot-details" aria-expanded="false">i</button>
      <section id="robot-details" aria-label="Robot and scan details" hidden></section>
    </div>
  </div>
  <div class="robot-choice">
    <span aria-hidden="true" id="robot-camera-name">Default camera</span>
    <select id="robot-camera"><option>Default camera</option></select>
  </div>
  <div class="robot-body">
    <div class="robot-cam" id="robot-cam">
      <video playsinline muted autoplay aria-label="What the camera sees"></video>
      <div class="robot-grid" aria-hidden="true"></div>
      <button type="button" class="button small" id="robot-camera-start" hidden>Start the camera</button>
    </div>
    <ol class="robot-shots" id="robot-shots" aria-label="Photos of this scan"></ol>
  </div>`;

const DIALOG = `
  <h2 id="robot-title">Robot</h2>
  <div class="robot-fields">
    <label>Robot <select id="robot-driver"></select></label>
    <label>Port <select id="robot-port"></select></label>
    <label class="robot-auto"><input type="checkbox" id="robot-auto"> Connect by itself when the robot is plugged in</label>
  </div>
  <p id="robot-dialog-status" role="status"></p>
  <form method="dialog">
    <button type="button" class="button quiet" id="robot-refresh">Look again</button>
    <button class="button">Close</button>
    <button type="button" class="button primary" id="robot-connect">Connect</button>
  </form>`;

const SCAN = "Scan & solve";
const HINT = "Put a cube in the robot, then press Scan & solve.";

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
  // What is kept on the device; for this visit only where there is no storage.
  const kept = new Map();
  const remember = (key, value) => {
    kept.set(key, value);
    try {
      localStorage.setItem(key, value);
    } catch {
      // No storage: the choice lasts for this visit.
    }
  };
  const recall = (key) => {
    try {
      return localStorage.getItem(key) ?? kept.get(key) ?? null;
    } catch {
      return kept.get(key) ?? null;
    }
  };
  const memory = {
    get auto() {
      return recall(AUTO_CONNECT) !== "off";
    },
    set auto(on) {
      remember(AUTO_CONNECT, on ? "on" : "off");
    },
    get last() {
      try {
        const last = JSON.parse(recall(LAST_ROBOT));
        return typeof last?.driver === "string" ? last : null;
      } catch {
        return null;
      }
    },
    set last(robot) {
      remember(LAST_ROBOT, JSON.stringify(robot));
    },
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

  const scanButton = el("button", { type: "button", class: "button primary large", id: "robot-scan", text: SCAN });
  $("solve").after(scanButton);
  // The robot's line, over the button it is about: what to do next, or what is wrong.
  const note = el("p", { class: "robot-note", id: "robot-note", role: "status", "aria-live": "polite" });
  $("valid").before(note);

  const video = panel.querySelector("video");
  video.muted = true;
  const feed = new CameraFeed(video);
  let openingCamera = false;
  const scratch = document.createElement("canvas");
  const thumb = document.createElement("canvas");
  const closeInfo = bindInfo($("robot-info"), $("robot-details"), document);
  // The robot's line: a hint in a few words, or (`warn`) something that needs doing. Always whole.
  const say = (text, warn = false) => {
    delete note.dataset.source;
    note.textContent = text;
    note.classList.toggle("warn", warn);
  };

  let drivers = [];
  let ports = new Map(); // what the server says of each port, if it says anything
  let listed = ""; // the robots and ports the dialog shows
  let asked = false; // the server has answered about its robots
  let connected = null; // {driver, port, size, info}
  let connecting = null; // {driver, port} while it is being connected
  let trouble = ""; // why the robot found did not connect
  let robot = null;
  let raw = null;
  let lastFace = -1;
  let following = false; // the page can animate the plan's moves
  let eta = null;
  let ticker = 0;
  let scanEnd = null; // when the scan is expected to be over (the page's clock)
  let measures = []; // how the last scan's photos were read, for the details
  let planned = []; // the last plan, likewise
  let logged = ""; // the last thing the server or the robot had to say
  let linked = true; // the server's events are reaching the page
  let switching = false;
  let sizing = false; // the robot is being set to another size of cube
  let gone = false; // the cube's line says the robot is no longer connected
  // What reading costs on this machine, kept from one robot and scan to the next.
  const pace = new ReadPace(3);

  // ------------------------------------------------------------ dialog

  const driverName = (id) => drivers.find((d) => d.id === id)?.name ?? id;
  // The cube on the page is the cube the robot is connected for, if it takes that size.
  const sizeFor = (id) => (drivers.find((d) => d.id === id)?.sizes?.includes(host.state.size) ? host.state.size : undefined);
  const portsOf = (id) => (drivers.find((d) => d.id === id)?.ports ?? []).filter((p) => typeof p === "string");

  // The ports of the robot chosen and those the server knows to be it, each with what is known of it.
  function fillPorts(prefer = "") {
    const id = $("robot-driver").value;
    const paths = [...new Set([...[...ports.values()].filter((p) => p.driver === id).map((p) => p.path), ...portsOf(id)])];
    const about = (path) => {
      const p = ports.get(path);
      return p?.busy ? " (in use)" : p?.driver === id ? " (recognised)" : p && "driver" in p && !p.driver ? " (no answer)" : "";
    };
    $("robot-port").replaceChildren(
      el("option", { value: "", text: paths.length ? "The first one found" : "None found: try anyway" }),
      ...paths.map((path) => el("option", { value: path, text: `${portName(path)}${about(path)}` })),
    );
    const free = paths.filter((path) => !ports.get(path)?.busy);
    const choice = [prefer, memory.last?.driver === id ? memory.last.port : "", free.find((path) => ports.get(path)?.driver === id), free[0], paths[0]].find((path) => path && paths.includes(path));
    $("robot-port").value = choice ?? "";
  }

  // What the server found, in the dialog: only when it has changed, and what was chosen stays chosen.
  function showList(answer) {
    asked = true;
    drivers = Array.isArray(answer.drivers) ? answer.drivers.filter((d) => d && typeof d.id === "string") : [];
    ports = new Map((Array.isArray(answer.ports) ? answer.ports : []).filter((p) => typeof p?.path === "string").map((p) => [p.path, p]));
    const now = JSON.stringify([answer.drivers ?? null, answer.ports ?? null]);
    if (connected) {
      if (now !== listed) showWhere();
      listed = now;
      return;
    }
    if (now === listed) return;
    if (listed) trouble = ""; // something was plugged in or out since
    listed = now;
    const was = [$("robot-driver").value, $("robot-port").value];
    $("robot-driver").replaceChildren(...drivers.map((d) => el("option", { value: d.id, text: `${d.name ?? d.id}${d.sizes?.length ? ` (${d.sizes.map(host.name).join(", ")})` : ""}` })));
    const found = choosePort(answer, memory.last);
    const choice = [was[0], found?.driver, memory.last?.driver].find((id) => id && drivers.some((d) => d.id === id));
    if (choice) $("robot-driver").value = choice;
    fillPorts(was[1] || (found?.driver === $("robot-driver").value ? found.port : ""));
  }

  function showDialog(said = "") {
    const on = !!connected;
    const busy = !!connecting;
    $("robot-driver").disabled = $("robot-port").disabled = on || busy || !drivers.length;
    $("robot-refresh").hidden = on;
    $("robot-auto").checked = memory.auto;
    const connect = $("robot-connect");
    connect.textContent = on ? "Disconnect" : busy ? "Connecting…" : "Connect";
    connect.classList.toggle("primary", !on);
    connect.disabled = switching || sizing || (on ? !!robot?.busy : busy || !drivers.length);
    connect.title = connect.disabled ? locked() || (busy ? "Connecting to the robot…" : "This server has no robot drivers.") : "";
    if (on) {
      $("robot-driver").replaceChildren(el("option", { text: driverName(connected.driver) }));
      $("robot-port").replaceChildren(el("option", { text: portName(connected.port) || "–" }));
    }
    const watching = memory.auto && !on;
    $("robot-dialog-status").textContent =
      said ||
      (busy
        ? `Connecting ${driverName(connecting.driver)}… The first time a size is used its solver tables are loaded, which can take a while.`
        : on
          ? ""
          : trouble || (asked && !drivers.length ? "This server has no robot drivers." : watching && !drivers.some((d) => portsOf(d.id).length) && !ports.size ? "No robot is plugged in. Plug one in and it connects by itself." : ""));
    // The button by the cube says the same in a word.
    open.setAttribute("aria-pressed", String(on));
    open.classList.toggle("busy", busy);
    open.classList.toggle("trouble", !on && !busy && !!trouble);
    open.querySelector("span").textContent = busy ? "Connecting…" : "Robot";
    open.title = on
      ? `${driverName(connected.driver)} connected`
      : busy
        ? `Connecting ${driverName(connecting.driver)} on ${portName(connecting.port)}…`
        : trouble || (watching ? "No robot connected: plug one in and it connects by itself" : "Connect a robot");
  }

  // Plug and play: the server is watched for a robot while none is connected.
  const watch = new RobotWatch({
    api,
    memory,
    size: sizeFor,
    hooks: {
      onlist(answer, error) {
        if (answer) showList(answer);
        showDialog(answer ? "" : `Could not ask the server about robots: ${error.message}`);
      },
      onconnecting(found) {
        connecting = found;
        trouble = "";
        showDialog();
      },
      onconnected(info) {
        connecting = null;
        trouble = "";
        if (dialog.open) dialog.close();
        void enter(info);
      },
      onfailed(found, error) {
        connecting = null;
        // A board that went by its name and answered as no robot is nothing to tell about:
        // the server says so port by port from now on, and it is left alone.
        const stranger = error.code === "no_answer" && !found.sure && ports.has(found.port);
        trouble = stranger ? "" : `${driverName(found.driver)} on ${portName(found.port)} did not connect: ${error.message} ${error.final ? "Plug it in again, or connect it here." : "Trying again shortly."}`;
        showDialog();
      },
    },
  });
  document.addEventListener("visibilitychange", () => {
    // Back on the page: looked at once, not at the browser's leisure.
    if (!document.hidden && watch.running) void watch.look();
  });

  open.addEventListener("click", () => {
    dialog.showModal();
    showDialog();
    if (!connected) void watch.look();
  });
  $("robot-driver").addEventListener("change", () => fillPorts());
  $("robot-refresh").addEventListener("click", () => watch.look());
  $("robot-auto").addEventListener("change", (e) => {
    memory.auto = e.target.checked;
    if (!e.target.checked) watch.stop();
    else if (!connected) watch.start();
    showDialog();
  });
  $("robot-connect").addEventListener("click", async () => {
    if (connected) {
      if (await disconnect()) dialog.close();
      else showDialog(note.textContent);
      return;
    }
    if (connecting) return;
    const driver = $("robot-driver").value;
    const port = $("robot-port").value || undefined;
    await connectManually(driver, port);
  });

  async function connectManually(driver, port) {
    // The choice is made by hand: the watching stands back until it is settled.
    watch.stop();
    await watch.looking;
    if (connected) return;
    connecting = { driver, port: port ?? "" };
    trouble = "";
    showDialog();
    try {
      const answer = await api.connect(driver, port, sizeFor(driver));
      connecting = null;
      memory.last = { driver, port: answer.port ?? port };
      watch.release(driver, answer.port ?? port);
      if (dialog.open) dialog.close();
      await enter({ driver, port: answer.port ?? port ?? "", size: answer.size, info: answer.info ?? {} });
    } catch (error) {
      connecting = null;
      trouble = `${driverName(driver)}${port ? ` on ${portName(port)}` : ""} did not connect: ${error.message}`;
      // The panel stayed for the robot that was to come: it goes now.
      if (root.classList.contains("robot-mode")) host.setMode(false);
      if (!dialog.open) host.say("bad", [el("b", { text: "Not connected. " }), trouble], "Choose a robot with the Robot button.");
      showDialog();
      // Not tried again at once behind the dialog's back.
      watch.missed(driver, port);
      if (memory.auto) watch.start();
    }
  }

  // ------------------------------------------------------------ camera

  async function openCamera(deviceId = recall(CAMERA_CHOICE)) {
    if (openingCamera) return false;
    const session = robot;
    openingCamera = true;
    setButtons();
    try {
      return await startCamera(deviceId, session);
    } finally {
      if (robot === session) {
        openingCamera = false;
        setButtons();
      }
    }
  }

  async function startCamera(deviceId, session) {
    const start = $("robot-camera-start");
    try {
      try {
        await feed.open(deviceId ? { deviceId: { exact: deviceId } } : undefined);
      } catch (error) {
        // The camera remembered is not there today (the phone is away): any other.
        if (session !== robot || !deviceId || !["OverconstrainedError", "NotFoundError", "NotReadableError"].includes(error.name)) throw error;
        await feed.open();
      }
    } catch (error) {
      if (session !== robot) return false;
      start.hidden = false;
      say(error.name === "NotAllowedError" ? "The camera is not allowed for this page. Allow it, then start it." : "The camera could not be started.", true);
      note.dataset.source = "camera";
      return false;
    }
    if (session !== robot) return false;
    start.hidden = true;
    const select = $("robot-camera");
    select.replaceChildren(...feed.cameras.map((c, k) => el("option", { value: c.deviceId, text: c.label || `Camera ${k + 1}` })));
    select.value = feed.deviceId ?? deviceId ?? "";
    if (select.selectedIndex < 0) select.selectedIndex = 0;
    if (!select.options.length) select.add(new Option("Default camera", ""));
    showChosen(select);
    if (note.dataset.source === "camera") say(HINT);
    showDetails();
    return true;
  }

  $("robot-camera").addEventListener("change", async (e) => {
    const id = e.target.value;
    showChosen(e.target);
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
    measures = [];
    planned = [];
    showDetails();
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
    li.title = `${FACE_LETTERS[face]}: as the page read it, ${image.width} pixels a side`;
  }

  // ------------------------------------------------------------ status

  // Why the robot's choices cannot be changed just now ("" when they can).
  const locked = () =>
    robot?.busy ? "Not while the robot is moving: stop it first." : sizing ? "The robot is being set to the cube's size…" : switching ? "Changing robots…" : openingCamera ? "The camera is being started…" : "";

  // The word beside "Robot": how the robot chosen stands, at a glance.
  function showState() {
    const phase = robot?.phase;
    const [word, kind] = !linked
      ? ["No connection", "warn"]
      : switching
        ? ["Connecting…", "busy"]
        : sizing
          ? ["Setting size…", "busy"]
          : phase === "starting" || phase === "scanning"
            ? ["Scanning", "busy"]
            : phase === "solving"
              ? ["Solving", "busy"]
              : openingCamera
                ? ["Starting camera…", "busy"]
                : !feed.running
                  ? ["No camera", "warn"]
                  : ["Ready", "ok"];
    $("robot-state").textContent = word;
    $("robot-state").dataset.kind = kind;
  }

  function setButtons() {
    const busy = !!robot?.busy;
    const reason = locked();
    scanButton.textContent = busy ? "Stop" : SCAN;
    scanButton.classList.toggle("primary", !busy);
    scanButton.classList.toggle("danger", busy);
    scanButton.disabled = !robot || sizing || switching || openingCamera;
    scanButton.title = busy ? "Stop the robot at once: its motors are switched off" : scanButton.disabled ? reason : "The robot shows the camera the six faces, then solves the cube";
    $("robot-device").disabled = busy || sizing || switching;
    $("robot-camera").disabled = !!reason;
    $("robot-camera-start").disabled = openingCamera;
    // Why a picker does not open is said where the pointer is, and to a screen reader.
    for (const id of ["robot-device", "robot-camera"]) $(id).title = $(id).disabled ? reason : "";
    showState();
    if (dialog.open) showDialog();
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
      // The scan's time left: the plan's, counted down, and at the last photo what the robot says is left of it.
      scanEnd = Number.isFinite(event.eta_ms) ? performance.now() + event.eta_ms : null;
      host.wait("Scanning", event.eta_ms ? `About ${duration(event.eta_ms)} for the six photos.` : "");
    },
    onphoto(index, face, image, event) {
      lastFace = face;
      if (index >= 5 && Number.isFinite(event?.next_ms)) scanEnd = performance.now() + event.next_ms;
      const left = el("span");
      const tick = () => {
        const ms = scanEnd === null ? 0 : scanEnd - performance.now();
        left.textContent = ms > 50 ? ` · ${duration(ms)} left` : "";
      };
      tick();
      host.say("busy", ["Scanning · photo ", el("b", { text: String(index + 1) }), " of 6", left]);
      stopTicker();
      ticker = setInterval(tick, 100);
      requestAnimationFrame(() => drawShot(face, image));
    },
    onface(face, cells) {
      const nn = host.state.size * host.state.size;
      if (!raw || raw.length !== 6 * nn) return;
      for (let i = 0; i < nn; i++) raw[face * nn + i] = `rgb(${cells[i].map(Math.round).join(",")})`;
      view.setRaw(raw);
    },
    onscanned(result) {
      stopTicker();
      host.wait(null);
      const { ok, note: said } = host.scanned(result, result.sources, lastFace);
      // How the photos were read is for whoever asks: behind the "i".
      measures = [...scanMeasures(result), ["Cube", result.ok ? "read whole" : "not made"]];
      showDetails();
      if (ok && result.ok) {
        host.say("busy", "Finding the robot's fastest solution…");
      } else {
        const twice = result.repeated?.[0];
        if (twice) {
          // The likeliest cause of all, said first.
          const [a, b] = twice.map((f) => FACE_LETTERS[f]);
          host.say("bad", [el("b", { text: `The photos of ${a} and ${b} are the same picture. ` }), "The camera was still showing one face when the robot asked for the next: the robot has to wait longer before each photo."], "Nothing more was asked of the robot.");
          return;
        }
        host.say("bad", [el("b", { text: "The scan did not make a cube. " }), said], `Nothing more was asked of the robot: press ${SCAN} to try again.`);
      }
    },
    onplan(plan) {
      following = host.plan(plan.moves ?? []);
      eta = new Eta(plan, performance.now());
      const left = el("b");
      const more = following ? "" : "The page could not follow these moves on its cube.";
      planned = [
        ["Moves", String(plan.moves?.length ?? 0)],
        ["Planned in", duration(plan.compute_ms)],
        ...(plan.candidates ? [["Solutions compared", String(plan.candidates)]] : []),
        ["Expected", duration(plan.eta_ms)],
      ];
      showDetails();
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
        ["Scan", t.scan_ms, "The robot showing the camera the six faces."],
        ["Analysis", t.analysis_ms, "From the end of the scan to the cube being read."],
        ["Compute", t.compute_ms, "Finding the robot's fastest solution."],
        ["Execution", t.exec_ms, "The robot turning the cube."],
        ["Total", t.total_ms, "From Scan to solved."],
      ].map(([label, ms, title]) => el("div", { title }, [el("dt", { text: label }), el("dd", { text: duration(ms) })])));
      host.say("good", [el("b", { text: "Solved by the robot" }), ` in ${duration(t.total_ms)}`], "", times, true);
    },
    onerror(error) {
      stopTicker();
      host.wait(null);
      host.player.stop();
      host.player.onchange();
      const stopped = error.code === "stopped";
      const message = error.message || error.code;
      // The server's own "Stopped: …" is not said twice.
      const after = message.trim().replace(/^stopped\b[\s:.]*/i, "");
      const text = stopped
        ? [el("b", { text: "Stopped. " }), after ? after[0].toUpperCase() + after.slice(1) : "The motors are off."]
        : [el("b", { text: "The robot stopped. " }), message];
      host.say("bad", text, stopped ? "Put the robot back in its rest position before the next scan." : "", null, true);
      if (!stopped) {
        logged = `${message} (${error.code})`;
        showDetails();
      }
      setButtons();
      // Whatever went wrong, the robot may be gone with it.
      if (!stopped) void checkStill(message);
    },
    onlost(event) {
      lost(event.message);
    },
    onlog(text) {
      if (!text) return;
      logged = text;
      showDetails();
    },
    onlink(ok) {
      linked = ok;
      say(ok ? "" : "No answer from the anycube server. Trying again…", !ok);
      showState();
      // A server that was away may have come back without the robot.
      if (ok) void checkStill();
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
    const session = robot;
    if (!feed.running && !(await openCamera())) return;
    if (robot !== session) return;
    closeInfo();
    const n = connected.size;
    host.setSize(n);
    robot.n = n;
    raw = host.scanStart();
    lastFace = -1;
    eta = null;
    clearShots();
    drawGrid(n);
    say("");
    await robot.scan();
  });

  // ------------------------------------------------------------ connected

  // The name of what a picker has chosen, whole, under the select itself.
  function showChosen(select) {
    $(`${select.id}-name`).textContent = select.selectedOptions[0]?.textContent ?? "";
  }

  // The robot's picker: the robot held, the other ports with what is known of them, and none.
  function showWhere() {
    const select = $("robot-device");
    const choices = robotChoices({ drivers, ports: [...ports.values()] }, connected, memory.last);
    select.replaceChildren(
      ...choices.map((choice) => {
        const option = new Option(choice.text, choice.path, choice.current, choice.current);
        option.disabled = !choice.current && (!choice.driver || choice.state === "busy");
        option.dataset.driver = choice.driver ?? "";
        return option;
      }),
      Object.assign(new Option("None / disconnect", ""), { id: "robot-none" }),
    );
    showChosen(select);
    showDetails();
  }

  // What the "i" opens: the robot and the camera as they are, and how the last scan and solve went.
  function showDetails() {
    if (!connected) return;
    const firmware = connected.info?.fw ?? connected.info?.firmware ?? connected.info?.version;
    const groups = [
      ["Robot", [
        ["Driver", driverName(connected.driver)],
        ["Port", connected.port || "–"],
        ["Cube", Number.isInteger(connected.size) ? host.name(connected.size) : "–"],
        ...(firmware ? [["Firmware", String(firmware)]] : []),
      ]],
      ["Camera", [["Picture", video.videoWidth ? `${video.videoWidth} × ${video.videoHeight}` : "not started"]]],
      ["Last scan", measures],
      ["Last solve", planned],
    ];
    $("robot-details").replaceChildren(
      ...groups.flatMap(([title, rows]) =>
        rows.length ? [el("h3", { text: title }), el("dl", {}, rows.flatMap(([label, value]) => [el("dt", { text: label }), el("dd", { text: value })]))] : [],
      ),
      ...(measures.length ? [] : [el("p", { text: "How the photos were read and the solution was found is listed here after a scan." })]),
      ...(logged ? [el("h3", { text: "Last message" }), el("p", { text: logged })] : []),
    );
  }

  // The ports are asked for when the picker is come to (the server is not
  // asked by itself while a robot is connected). With the watching switched
  // off the server only says what it knows, and wakes no board.
  async function refreshDevices() {
    const was = connected;
    if (!was || switching || robot?.busy) return;
    try {
      const answer = await api.robots(memory.auto);
      if (connected === was && !switching && !robot?.busy) showList(answer);
    } catch {
      // The picker keeps what it had; a server that is away says so on its own line.
    }
  }
  const devices = $("robot-device");
  devices.addEventListener("focus", refreshDevices);
  devices.addEventListener("pointerenter", refreshDevices);
  devices.addEventListener("change", async () => {
    if (switching || robot?.busy || sizing) return showWhere();
    const chosen = devices.selectedOptions[0];
    const none = chosen?.id === "robot-none";
    const [driver, port] = [chosen?.dataset.driver, devices.value];
    if (!none && (!driver || (port || "") === (connected?.port ?? ""))) return showWhere();
    showChosen(devices);
    switching = true;
    setButtons();
    try {
      // To another robot the panel stays; to none the page is as it is without one.
      if (!(await disconnect(none))) return showWhere();
      if (!none) await connectManually(driver, port || undefined);
    } finally {
      switching = false;
      setButtons();
    }
  });

  // Another size chosen on the page while the robot is at rest: the robot is
  // connected anew for that cube (its firmware starts as a 3x3x3 and is told).
  async function resize(n) {
    const was = connected;
    const sizes = drivers.find((d) => d.id === was.driver)?.sizes;
    if (sizes?.length && !sizes.includes(n)) {
      say(`${driverName(was.driver)} does not take a ${host.name(n)}: it stays at ${host.name(was.size)}.`, true);
      return;
    }
    sizing = true;
    setButtons();
    say(`Setting the robot to ${host.name(n)}…`);
    try {
      const answer = await api.connect(was.driver, was.port || undefined, n);
      if (connected !== was) return;
      connected = { ...was, size: answer.size, info: answer.info ?? was.info };
      robot.n = answer.size;
      showWhere();
      drawGrid(answer.size);
      clearShots();
      if (answer.size !== host.state.size) host.setSize(answer.size);
      say(HINT);
    } catch (error) {
      say(`The robot could not be set to ${host.name(n)}: ${error.message}`, true);
      // The server let go of the robot to connect it anew: it may be gone.
      void checkStill(error.message);
    } finally {
      sizing = false;
      setButtons();
    }
  }
  host.onsize((n) => {
    if (connected && robot && !robot.busy && !switching && !sizing && Number.isInteger(connected.size) && n !== connected.size) void resize(n);
  });

  async function enter(info) {
    watch.stop();
    connected = info;
    // It is back: what was said of its going is over.
    if (gone) host.say(null);
    gone = false;
    showDialog();
    showWhere();
    if (Number.isInteger(info.size)) host.setSize(info.size);
    host.setMode(true);
    clearShots();
    drawGrid(info.size ?? 3);
    const mine = (robot = new RobotRun({
      api,
      n: info.size ?? 3,
      grab: (rotate, size) => grabFrame(video, rotate, size, scratch),
      hooks,
      pace,
    }));
    linked = true;
    setButtons();
    say(HINT);
    void robot.follow();
    await openCamera();
    setTimeout(() => robot === mine && warmUp(mine), 300);
  }

  // The first reading in a page is several times slower than the next
  // (measured in Chromium: a grab 22 ms against 1, six faces 180 ms against
  // 50): done once on connecting, so that the first scan does not pay it.
  function warmUp(run) {
    try {
      const n = run.n;
      const sizes = readSizes(n);
      for (const size of [sizes[0], sizes.at(-1)]) {
        const frame = grabFrame(video, 90, size, scratch) ?? { width: size, height: size, data: new Uint8ClampedArray(size * size * 4) };
        readFace(frame, n);
      }
      const rgb = STICKERS.map((hex) => [1, 3, 5].map((k) => parseInt(hex.slice(k, k + 2), 16)));
      assembleFaces(n, Array.from(new CubeModel(n).solved(), (k) => rgb[k]));
    } catch {
      // Only time is lost.
    }
  }

  // `stay`: another robot is about to be connected, and the panel waits for it.
  function leave(stay = false) {
    stopTicker();
    closeInfo();
    robot?.unfollow();
    robot = null;
    connected = null;
    feed.close();
    openingCamera = false;
    listed = "";
    measures = [];
    planned = [];
    logged = "";
    say("");
    if (!stay) host.setMode(false);
    setButtons();
    showDialog();
  }

  // Disconnects by hand. `rest`: no other robot follows, the page goes back to being without one.
  async function disconnect(rest = true) {
    const was = connected;
    try {
      await api.disconnect();
    } catch (error) {
      say(`Could not disconnect: ${error.message}`, true);
      return false;
    }
    // Disconnected by hand: it stays so until it is plugged in again or connected by hand.
    if (was) watch.hold(was.driver, was.port);
    leave(!rest);
    if (rest) watch.start();
    return true;
  }

  // The robot is gone (unplugged, or the server lost it): the page as it is
  // without one, watching for it to come back.
  function lost(why = "") {
    if (!connected) return;
    const name = driverName(connected.driver);
    leave();
    host.say("bad", [el("b", { text: `${name} is no longer connected. ` }), memory.auto ? "It connects again by itself when it is plugged in." : "Connect it again with Robot."], why);
    gone = true;
    watch.start();
  }

  // Asks the server whether it still holds the robot, after anything that may have lost it.
  async function checkStill(why = "") {
    const was = connected;
    if (!was) return;
    try {
      const answer = await api.robots();
      if (connected === was && !answer.connected) lost(why);
    } catch {
      // No answer says nothing about the robot.
    }
  }

  // A robot left connected (the page was reloaded) is taken up again; one
  // plugged in is connected, now or whenever it is.
  setButtons();
  showDialog();
  watch.start();

  return { api, watch, get robot() { return robot; }, get connected() { return connected; } };
}
