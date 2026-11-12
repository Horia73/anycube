// The application: one cube to enter, turn, solve and follow.
//
// `mount(root, options)` builds it inside an element of the hosting page.
// What differs from host to host comes in as options, never as text here:
//
//   top        nodes for the bar at the top (a host's own links and words)
//   playMax    the largest cube drawn and turned by hand (50 unless given)
//   overLimit  (size, largest) => what to say about a cube larger than the
//              solver behind the page takes, as text or nodes
//
// What the solver behind the page can do (largest size, time cap) is read
// from its `/api/config`.

import { CameraScanner } from "./camera.js";
import { named, sortColours, STICKERS } from "./colour.js";
import { CubeModel, FACE_LETTERS, scrambleLength, ScrambleError } from "./cube-model.js";
import { BLANK, CubeView } from "./cube-view.js";
import { APP_HTML } from "./markup.js";
import { FACE_NAMES, PaintEditor } from "./paint.js";
import { Player } from "./player.js";
// (`arrange` here is the page's own layout; the scan's is `placed`.)
import { arrange as placed, assemble } from "./scan-fit.js";
import { Solver } from "./solver.js";
import { ViewCube } from "./view-cube.js";

// Cubes larger than this are entered as scrambles: nobody paints, or shows
// a camera, 384 stickers and more.
const PAINT_MAX = 7;
const CAMERA_MAX = 7;
const TOP_COLOUR = "anycube.top-colour";
// Turning the cube 180° with the front held fixed exchanges both pairs.
const YELLOW_TOP = [3, 4, 2, 0, 1, 5].map((face) => STICKERS[face]);

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

const name = (n) => `${n}x${n}x${n}`;
const count = (k, thing) => `${k} ${thing}${k === 1 ? "" : "s"}`;

// What each way of being impossible means, in words for someone holding a
// cube, and in a few words for the line above Solve.
const IMPOSSIBLE = {
  colour_count: ["Too many of one colour", "The colours do not add up: each belongs on the same number of stickers."],
  fixed_centres: ["A middle sticker has the wrong colour", "On a cube of odd size the middle sticker of each face never moves, so it must have the colour of its own face."],
  centre_colours: ["Too many centres of one colour", "The centre stickers are not a set that exists: some ring of centres has too many of one colour."],
  bad_corner: ["A corner that no cube has", "One corner shows colours that no corner piece has. Look for a wrong sticker on a corner."],
  duplicate_corner: ["The same corner twice", "The same corner piece appears twice, so another is missing. One corner sticker is wrong."],
  twisted_corner: ["One corner is twisted", "One corner is twisted in its place: the three stickers of a corner are in the wrong order."],
  bad_edge: ["An edge that no cube has", "One edge piece shows colours that no edge has. Look for a wrong sticker on an edge."],
  duplicate_edge: ["The same edge twice", "The same edge piece appears twice, so another is missing. One edge sticker is wrong."],
  flipped_edge: ["One edge is flipped", "One edge is flipped in its place: its two stickers are exchanged."],
  swapped_pieces: ["Two pieces are exchanged", "Two pieces have changed places, which no sequence of turns does: two corners, or two edges, are exchanged."],
};

const CAMERA_PROBLEMS = {
  NotAllowedError: "The browser did not allow the camera. Allow it for this page in the address bar, or paint the cube instead.",
  NotFoundError: "No camera was found on this device. Paint the cube instead.",
  NotReadableError: "The camera is in use by another program. Close it there and try again.",
  OverconstrainedError: "The camera could not be opened with these settings. Paint the cube instead.",
  InsecureContextError: "Browsers give a page the camera only over https. Open the page by its https address.",
  NotSupportedError: "This browser does not give pages a camera. Paint the cube instead.",
};

const FAILURES = {
  queue_full: "The server is busy",
  rate_limited: "Too many solves in a short time",
  too_many_jobs: "You already have solves waiting",
  timeout: "The search took too long",
  network: "No connection to the server",
  shutting_down: "The server is restarting",
};

export async function mount(root, options = {}) {
  root.innerHTML = APP_HTML;
  const $ = (id) => root.querySelector(`#${id}`);
  if (options.top) $("top-extra").append(options.top);

  // The largest cube the page draws and lets a hand turn. Measured on a
  // laptop (M1 Pro, a canvas of 1100 pixels): a frame with layers mid-turn
  // takes 5 ms at 20x20x20, 9 ms at 30, 22 ms at 50, 60 ms at 75 and 145 ms
  // at 101. Up to 50 a layer follows the finger.
  const PLAY_MAX = options.playMax ?? 50;

  const canvas = $("cube");
  const cubeLabel = canvas.getAttribute("aria-label");
  const view = new CubeView(canvas);
  const player = new Player(view);
  view.busy = () => player.moving;
  const solver = new Solver();
  let defaultPalette = STICKERS;
  try {
    if (localStorage.getItem(TOP_COLOUR) === "yellow") defaultPalette = YELLOW_TOP;
  } catch {
    // Storage can be disabled. The choice still works for this visit.
  }

  const state = {
    size: 3,
    model: new CubeModel(3),
    tab: "scramble", // the tab in view
    input: "scramble", // where the cube comes from: "scramble", "paint" or "camera"
    mode: "normal",
    scramble: { moves: [], problem: null },
    painted: new Map(), // size -> { facelets, palette, doubt }
    carried: null, // the scramble's cube as it stood when Paint was last left for it (see setTab)
    shows: "scramble", // whose cube is on screen, with any solution of it: the scramble's or the painted one
    scan: null, // while the camera reads a cube: the colours seen so far, per facelet
    job: null, // an AbortController while a solve is under way
    ticker: 0,
    solution: null,
    valid: false, // whether the cube as entered can be solved
    detail: null, // what is wrong with it, at length, for the line over the cube
    result: null, // how the last solve went, for the same line
  };

  const narrow = window.matchMedia("(max-width: 759px)");

  // ------------------------------------------------- kept over a reload

  // The page as it stands is kept for this tab, so that a reload brings it
  // back: the size, the tab, the cube, the scramble, the solution and how
  // far it was played, the way the cube is held. A new tab starts afresh.
  const KEPT = "anycube.page";
  let keeping = false; // not before the page has started: a reload then keeps what was kept
  let keepSoon = 0;

  function keep() {
    if (!keeping) return;
    const { size, model, solution } = state;
    const p = size <= PAINT_MAX ? state.painted.get(size) : null;
    const page = {
      v: 1,
      size,
      input: state.input,
      tab: state.tab,
      mode: state.mode,
      speed: player.speed,
      scramble: $("scramble").value,
      painted: p
        ? { facelets: Array.from(p.facelets), palette: p.palette, followsDefault: p.palette === defaultPalette,
            doubt: p.doubt ? Array.from(p.doubt) : null, note: $("paint-note").textContent }
        : null,
      carried: state.carried,
      shows: state.shows,
      solution: solution
        ? { moves: model.names(solution.moves), parts: solution.parts ?? [], label: solution.label ?? "", optimal: !!solution.optimal, found: solution.found ?? solution.moves.length }
        : null,
      position: player.position,
      turns: {
        partial: model.names(player.partial?.made ?? []),
        detour: model.names(player.detour),
      },
      rot: Array.from(view.rot),
    };
    try {
      sessionStorage.setItem(KEPT, JSON.stringify(page));
    } catch {
      // No storage here: a reload starts afresh.
    }
  }

  function keepLater() {
    clearTimeout(keepSoon);
    keepSoon = setTimeout(keep, 250);
  }

  const viewCube = new ViewCube($("view-cube"), (direction) => view.lookAlong(direction));
  viewCube.update(view.rot);
  view.onorientation = (rot) => {
    viewCube.update(rot);
    keepLater();
  };

  function showPalette(palette) {
    viewCube.setPalette(palette);
    view.setColours({ palette });
  }

  // What was kept, with the cube it shows worked out: null if there is
  // nothing, or if it does not hold together.
  function kept() {
    try {
      const page = JSON.parse(sessionStorage.getItem(KEPT) ?? "null");
      if (page?.v !== 1 || !Number.isInteger(page.size) || page.size < 2 || page.size > PLAY_MAX) return null;
      if (!["scramble", "paint", "camera"].includes(page.input)) return null;
      const model = new CubeModel(page.size);
      const scramble = model.normalise(String(page.scramble ?? "")).moves;
      let palette = defaultPalette;
      let start = model.applyAll(model.solved(), scramble);
      if (page.painted) {
        if (page.painted.facelets?.length !== model.count || page.painted.palette?.length !== 6) return null;
        page.painted.facelets = model.withCentres(Uint8Array.from(page.painted.facelets));
      }
      if (page.input !== "scramble" && page.painted && page.size <= PAINT_MAX && page.shows !== "scramble") {
        start = page.painted.facelets;
        palette = page.painted.followsDefault ? defaultPalette : page.painted.palette;
      }
      let moves = [];
      if (page.solution) {
        moves = model.parse(String(page.solution.moves), { slices: true });
        // A solution is kept only with the cube it solves.
        if (!model.isSolved(model.applyAll(start, moves))) page.solution = null;
      }
      if (!page.solution) moves = [];
      const position = Math.max(0, Math.min(moves.length, Math.round(Number(page.position) || 0)));
      const turns = position < moves.length
        ? { partial: model.parse(page.turns?.partial ?? "", { slices: true }), detour: model.parse(page.turns?.detour ?? "", { slices: true }) }
        : { partial: [], detour: [] };
      const rot = Array.isArray(page.rot) && page.rot.length === 9 && page.rot.every(Number.isFinite) ? page.rot : null;
      const shown = model.applyAll(start, [...moves.slice(0, position), ...turns.partial, ...turns.detour]);
      return { ...page, model, start, palette, moves, position, turns, rot, shown };
    } catch {
      return null;
    }
  }

  // The first thing drawn is the cube as it was: before anything is asked
  // of a server or a solver, and with nothing else drawn before it.
  view.waiting = true;
  let page = kept();
  if (page) {
    showPalette(page.palette);
    if (page.rot) view.rot = page.rot;
    view.setCube(page.size, page.shown);
    view.waiting = false;
    view.draw();
  }

  // -------------------------------------------------------------- colours

  function themeColours() {
    const css = getComputedStyle(root);
    const get = (v) => css.getPropertyValue(v).trim();
    view.setColours({ body: get("--cube-body"), inside: get("--cube-inside"), blank: get("--cube-blank") });
  }
  themeColours();
  window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", themeColours);

  // --------------------------------------------------------------- layout

  // Everything has one place and keeps it: the cube and its playback on one
  // side, the input, the solution and Solve on the other. On a narrow
  // screen the sizes go into the bar at the top and the solution into a tab
  // of its own. While the camera reads a cube its picture takes the stage,
  // and the cube being filled in sits where the solution will be.
  function arrange() {
    const sizes = root.querySelector(".size-row");
    if (narrow.matches) {
      root.querySelector(".top").insertBefore(sizes, $("top-extra"));
      $("panel-moves").append($("solution"));
    } else {
      root.querySelector(".panel").prepend(sizes);
      $("panel-lower").prepend($("solution"));
      if (state.tab === "moves") setTab(state.input);
    }
    placeCube();
  }

  function placeCube() {
    const scanning = scanner.running;
    const host = scanning ? (narrow.matches ? $("scan-slot-narrow") : $("scan-slot")) : $("stage");
    if (canvas.parentElement !== host) host.prepend(canvas);
    canvas.setAttribute("aria-label", scanning ? "The cube from your perspective, with the face to capture on the far side. Drag or use the arrow keys to look around." : cubeLabel);
    $("stage").classList.toggle("scanning", scanning);
    $("panel-lower").classList.toggle("scanning", scanning);
    $("camera-frame").hidden = !scanning;
    $("camera-scan").hidden = !scanning;
    $("camera-intro").hidden = scanning || state.size > CAMERA_MAX;
  }

  // -------------------------------------------- the line over the cube

  function drawStatus(kind, text, more = "", extra = null) {
    const box = $("status");
    clearInterval(state.ticker);
    if (!kind) {
      box.hidden = true;
      drawWait(null);
      return;
    }
    box.hidden = false;
    box.className = `status ${kind}`;
    if (kind !== "busy") drawWait(null);
    const mark =
      kind === "busy"
        ? el("span", { class: "busy", "aria-hidden": "true" }, [el("i"), el("i"), el("i"), el("i")])
        : el("span", { class: "mark", "aria-hidden": "true", text: kind === "good" ? "✓" : "!" });
    const body = el("p", { class: "text" });
    body.append(...(Array.isArray(text) ? text : [text]));
    box.replaceChildren(mark, body);
    if (more) box.append(el("p", { class: "more-line", text: more }));
    if (extra) box.append(extra);
  }

  // While a solve is under way the same is said large where the solution
  // will be: a place in the line is not to be missed.
  function drawWait(title, more = "", extra = null) {
    const box = $("wait");
    $("solution").classList.toggle("waiting", Boolean(title));
    box.hidden = !title;
    if (!title) return box.replaceChildren();
    const head = el("p", { class: "wait-title" });
    head.append(...(Array.isArray(title) ? title : [title]));
    box.replaceChildren(head);
    if (extra) box.append(extra);
    if (more) box.append(el("p", { class: "wait-more", text: more }));
  }

  // What it says when no solve is under way: what is wrong with the cube
  // at length, else how the last solve went, else nothing.
  function restStatus() {
    if (state.job) return;
    if (state.detail) drawStatus("bad", state.detail);
    else if (state.result) drawStatus(...state.result);
    else drawStatus(null);
  }

  // ------------------------------------------- the line above Solve

  // Whether the cube as it stands can be solved, in a few words, always in
  // view; Solve works only when it can.
  function setValid(kind, text, title = "") {
    const line = $("valid");
    line.className = `valid ${kind}`;
    line.replaceChildren(...(Array.isArray(text) ? text : [text]));
    // The line is one line long; its whole text, or the longer explanation, is there on hover.
    line.title = title || line.textContent;
    state.valid = kind === "ok";
    setBusy(!!state.job);
  }

  // ---------------------------------------------------------------- input

  // The painted cube of a size. The middle of each face of an odd cube
  // names the face, whatever way the cube came to be here.
  function painted(n = state.size) {
    let p = state.painted.get(n);
    if (!p) {
      p = { facelets: new CubeModel(n).solved(), palette: defaultPalette, doubt: null };
      state.painted.set(n, p);
    }
    p.facelets = new CubeModel(n).withCentres(p.facelets);
    return p;
  }

  function solvable(n = state.size) {
    return solver.inBrowser(n) || (!solver.config.offline && n <= solver.config.max_size);
  }

  // The cube as entered: what to send to a solver and what to show, or what is wrong with it.
  function entered() {
    const { model, size } = state;
    if (state.scan) {
      return { problem: "Capture all six faces first", start: new Uint8Array(model.count).fill(BLANK), palette: defaultPalette };
    }
    if (state.input === "scramble") {
      if (state.scramble.problem) return { problem: state.scramble.problem, start: model.solved(), palette: defaultPalette };
      const moves = state.scramble.moves;
      return { kind: "moves", input: model.names(moves), start: model.applyAll(model.solved(), moves), palette: defaultPalette, moves: moves.length };
    }
    const p = painted();
    const cube = { start: p.facelets, palette: p.palette };
    const nn = size * size;
    const placed = new Array(6).fill(0);
    let blank = 0;
    for (const c of p.facelets) c === BLANK ? blank++ : placed[c]++;
    if (blank > 0) {
      cube.problem = `${count(blank, "sticker")} not painted yet`;
    } else if (placed.some((k) => k !== nn)) {
      const over = placed.findIndex((k) => k > nn);
      cube.problem = `Too many of one colour: ${placed[over]} of ${nn}`;
    } else {
      cube.kind = "facelets";
      cube.input = model.toText(p.facelets);
    }
    return cube;
  }

  // Shows the cube as entered, forgetting any solution of an earlier one,
  // and says whether it can be solved.
  let checking = 0;
  function show() {
    state.job?.abort();
    state.job = null;
    state.solution = null;
    state.result = null;
    state.detail = null;
    const cube = entered();
    showPalette(cube.palette);
    view.setMarks(state.input === "scramble" || state.scan ? null : painted().doubt);
    player.load(state.model, cube.start, []);
    state.shows = state.input === "scramble" ? "scramble" : "painted";
    view.setRaw(state.scan);
    renderSolution(null);
    restStatus();
    judge(cube);
    keepLater();
  }

  // The tab changed and the cube on screen is the one the new tab is about:
  // it stays as it is, a solution and how far it was played with it. Only
  // what the tab says about it is brought up to date.
  function stay() {
    const cube = entered();
    view.setMarks(state.input === "scramble" ? null : painted().doubt);
    restStatus();
    judge(cube);
    keepLater();
  }

  async function judge(cube) {
    const mine = ++checking;
    const { size } = state;
    const where = solver.where(size);
    const good = (what) => [el("b", { text: "Valid" }), ` · ${what} · ${where}`];
    if (cube.problem) {
      setValid("bad", cube.problem);
      return;
    }
    if (!solvable(size)) {
      const largest = solver.config.offline ? 3 : solver.config.max_size;
      setValid("play", options.overLimit?.(size, largest) ?? `Larger than the ${name(largest)} solved here: this one is for playing.`);
      return;
    }
    if (cube.kind === "moves") {
      // The moves are those of the scramble as written, not how far the cube is from solved.
      setValid("ok", cube.moves ? good(`a scramble of ${count(cube.moves, "move")}`) : "Solved already: turn it, or scramble it");
      return;
    }
    // Painted or scanned: every piece is looked at before Solve may be pressed.
    // The check is back at once as a rule; only one that takes a moment is
    // said, so that the line and the button do not blink at every change.
    const slow = setTimeout(() => {
      if (mine === checking) setValid("wait", "Checking the cube…");
    }, 400);
    const error = await solver.check(size, "facelets", cube.input);
    clearTimeout(slow);
    if (mine !== checking) return;
    if (!error) {
      setValid("ok", good("every piece checked"));
    } else if (error.impossible) {
      const [brief, long] = IMPOSSIBLE[error.code] ?? [error.message, error.message];
      setValid("bad", `Cannot be solved: ${brief.toLowerCase()}`, long);
      state.detail = problemNodes(error);
      restStatus();
    } else {
      setValid("bad", error.message);
    }
  }

  function problemNodes(error) {
    if (!error.impossible) return [error.message];
    const nodes = [el("b", { text: "This cube cannot be solved. " }), IMPOSSIBLE[error.code]?.[1] ?? error.message];
    // The solver's own sentence, where it names the piece or the colours.
    if (/\b[URFDLB]\b/.test(error.message)) {
      nodes.push(el("small", { text: `${error.message}. U, R, F, D, L and B stand for the colours of the top, right, front, bottom, left and back faces.` }));
    }
    return nodes;
  }

  // What a touch on the cube does, by what the page is doing.
  function setTouch() {
    view.mode = scanner.running ? "look" : state.input !== "scramble" && state.size <= PAINT_MAX ? "paint" : "turn";
  }

  // ----------------------------------------------------------------- size

  function renderSizes() {
    const sizes = [2, 3, 4, 5, 6, 7];
    $("sizes").replaceChildren(
      ...sizes.map((n) => {
        const b = el("button", { type: "button", role: "radio", "aria-checked": "false", "aria-label": name(n), text: String(n) });
        b.addEventListener("click", () => setSize(n));
        return b;
      }),
    );
    const custom = $("custom-size");
    custom.max = String(PLAY_MAX);
    custom.title = `Any size from 2 to ${PLAY_MAX}`;
    const take = () => {
      const n = Math.round(Number(custom.value));
      if (!custom.value || !Number.isFinite(n)) return;
      setSize(Math.max(2, Math.min(PLAY_MAX, n)));
    };
    custom.addEventListener("change", take);
    custom.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        take();
        custom.blur();
      }
    });
  }

  function setSize(n) {
    scanner.stop();
    if (state.tab === "moves") setTab(state.input);
    state.size = n;
    state.model = new CubeModel(n);
    let onButton = false;
    for (const b of $("sizes").children) {
      const on = Number(b.textContent) === n;
      b.setAttribute("aria-checked", String(on));
      onButton ||= on;
      if (on) b.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
    const custom = $("custom-size");
    custom.classList.toggle("on", !onButton);
    custom.value = onButton ? "" : String(n);
    $("stage-size").textContent = name(n);
    const here = solver.inBrowser(n);
    // The 2x2x2 has one answer, the shortest; a cube too large to solve has none.
    for (const b of $("mode-field").children) b.disabled = n === 2 || !solvable(n);
    const half = Math.floor(n / 2);
    const wide = half === 1 ? "" : half === 2 ? ", wide turns as <code>Rw</code>" : `, wide turns as <code>Rw</code> to <code>${half}Rw</code>`;
    $("notation").innerHTML =
      `Face turns <code>U R F D L B</code> with <code>'</code> and <code>2</code>${wide}. ` +
      `Also read: lower case, <code>x y z</code>${n % 2 ? ", <code>M E S</code>" : ""}. Or drag a layer of the cube.`;
    $("scramble-kind").textContent = here
      ? "Random scramble: a random state, every position equally likely."
      : `Random scramble: ${scrambleLength(n)} random moves, none undoing the one before.`;
    // What can be painted and scanned at this size.
    const paintable = n <= PAINT_MAX;
    $("paint").hidden = $("paint-note").hidden = $("paint-buttons").hidden = !paintable;
    $("paint-large").hidden = paintable;
    $("paint-large").textContent = `Painting is offered up to ${name(PAINT_MAX)}: a ${name(n)} has ${6 * n * n} stickers. Enter it as a scramble, or turn it by hand.`;
    $("camera-large").hidden = n <= CAMERA_MAX;
    $("camera-large").textContent = `The camera reads cubes up to ${name(CAMERA_MAX)}. Enter a ${name(n)} as a scramble.`;
    placeCube();
    if (paintable) {
      const p = painted();
      paint.set(n, p.facelets, p.palette, p.doubt);
    }
    setTouch();
    readScramble();
    if (state.input !== "scramble") show();
    if (here) solver.warm(n);
  }

  // ----------------------------------------------------------------- tabs

  const TABS = ["scramble", "paint", "camera", "moves"];

  const PAINT_NOTE = "Pick a colour, then tap or drag over the stickers.";

  // Which tab is in view, and nothing else.
  function showTab(tab) {
    state.tab = tab;
    for (const t of TABS) {
      const on = t === tab;
      $(`tab-${t}`).setAttribute("aria-selected", String(on));
      $(`tab-${t}`).tabIndex = on ? 0 : -1;
      $(`panel-${t}`).hidden = !on;
    }
  }

  function setTab(tab, focus = false) {
    if (tab !== "camera") scanner.stop();
    setHelp(false);
    const from = state.tab;
    showTab(tab);
    if (focus) $(`tab-${tab}`).focus();
    // The solution's tab only shows what there is; the cube stays as it is.
    if (tab === "moves" || (from === "moves" && tab === state.input)) return;
    const came = state.input;
    const paintable = state.size <= PAINT_MAX;
    // Whether the cube on screen is the cube the new tab is about: then it
    // stays, with its solution and how far that was played.
    let same = false;
    if (tab !== "scramble" && came === "scramble" && paintable && !state.scramble.problem) {
      // Paint (and the camera's tab, before a scan) start from the cube on
      // screen: the scramble's, with as much of a solution as was played,
      // there to be corrected sticker by sticker. Only a cube painted before
      // and not changed under Scramble since is left as it was painted.
      const shown = state.model.toText(player.facelets);
      if (shown !== state.carried) {
        state.painted.set(state.size, { facelets: player.facelets.slice(), palette: defaultPalette, doubt: null });
        $("paint-note").textContent = PAINT_NOTE;
      }
      same = state.model.toText(painted().facelets) === shown;
    } else if (tab === "scramble" && came !== "scramble") {
      // Back to the scramble: its cube is still on screen if nothing was painted meanwhile.
      same = state.shows === "scramble" && !state.scramble.problem;
    } else if (tab !== "scramble" && came !== "scramble") {
      same = true;
    }
    state.input = tab;
    setTouch();
    if (tab !== "scramble" && paintable) {
      const p = painted();
      paint.set(state.size, p.facelets, p.palette, p.doubt);
    }
    if (same && !state.scan) stay();
    else show();
    if (tab === "scramble" && came !== "scramble") state.carried = state.model.toText(player.facelets);
  }

  for (const t of TABS) {
    $(`tab-${t}`).addEventListener("click", () => setTab(t));
    $(`tab-${t}`).addEventListener("keydown", (e) => {
      const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
      if (!step) return;
      const shown = TABS.filter((x) => x !== "moves" || narrow.matches);
      setTab(shown[(shown.indexOf(t) + step + shown.length) % shown.length], true);
      e.preventDefault();
    });
  }

  // ------------------------------------------------------------- scramble

  const scrambleBox = $("scramble");
  const scrambleMirror = $("scramble-mirror");

  function syncScrambleScroll() {
    scrambleMirror.scrollTop = scrambleBox.scrollTop;
  }

  function markScrambleError(range) {
    scrambleMirror.hidden = !range;
    if (!range) {
      scrambleMirror.replaceChildren();
      return;
    }
    const { start, end } = range;
    const text = scrambleBox.value;
    scrambleMirror.replaceChildren(
      text.slice(0, start),
      el("mark", { text: text.slice(start, end) }),
      text.slice(end),
    );
    // The textarea has a blank line after a final newline.
    if (text.endsWith("\n")) scrambleMirror.append("\n");
    syncScrambleScroll();
  }

  function readScramble() {
    let range = null;
    try {
      state.scramble = { moves: state.model.normalise(scrambleBox.value).moves, problem: null };
      if (state.scramble.moves.length > solver.config.max_scramble_moves && solvable()) {
        state.scramble.problem = `${state.scramble.moves.length} moves: the solver takes scrambles up to ${solver.config.max_scramble_moves}`;
      }
    } catch (error) {
      if (!(error instanceof ScrambleError)) throw error;
      state.scramble = { moves: [], problem: error.message };
      range = error.range;
    }
    scrambleBox.setAttribute("aria-invalid", String(!!state.scramble.problem));
    markScrambleError(range);
    $("undo").disabled = state.scramble.moves.length === 0;
    if (state.input === "scramble") show();
  }

  let typing = 0;
  scrambleBox.addEventListener("scroll", syncScrambleScroll);
  scrambleBox.addEventListener("input", () => {
    scrambleMirror.hidden = true;
    clearTimeout(typing);
    typing = setTimeout(readScramble, 150);
  });

  function setScramble(moves) {
    $("scramble").value = state.model.names(moves);
    readScramble();
  }

  let scrambling = 0;
  async function randomScramble() {
    const { size, model } = state;
    const mine = ++scrambling;
    const was = $("scramble").value;
    // The small cubes: a random state, from the solver in this browser.
    let text = await solver.randomState(size);
    // It took a moment: if the cube was changed meanwhile, that stands.
    if (mine !== scrambling || state.size !== size || $("scramble").value !== was) return;
    if (text === null) {
      const random = () => crypto.getRandomValues(new Uint32Array(1))[0] / 2 ** 32;
      text = model.names(model.randomScramble(scrambleLength(size), random));
    }
    if (state.input !== "scramble") setTab("scramble");
    $("scramble").value = text;
    readScramble();
  }
  $("random").addEventListener("click", randomScramble);

  // The "i": what a random scramble is here and what may be typed. It lies
  // over the panel by the buttons, and goes at the next click or Escape.
  function setHelp(open) {
    const box = $("help-text");
    $("help").setAttribute("aria-expanded", String(open));
    box.hidden = !open;
    if (!open) return;
    const panel = root.querySelector(".panel").getBoundingClientRect();
    const row = $("help").getBoundingClientRect();
    // Under the buttons where there is room on the screen, over them where there is not.
    const under = row.bottom + 6 + box.offsetHeight <= window.innerHeight - 6;
    box.style.top = `${Math.round((under ? row.bottom + 6 : row.top - 6 - box.offsetHeight) - panel.top)}px`;
  }
  $("help").addEventListener("click", (e) => {
    e.stopPropagation();
    setHelp($("help-text").hidden);
  });
  document.addEventListener("click", (e) => {
    if (!$("help-text").hidden && !e.target.closest?.("#help-text")) setHelp(false);
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") setHelp(false);
  });
  $("clear").addEventListener("click", () => setScramble([]));
  $("undo").addEventListener("click", () => setScramble(state.scramble.moves.slice(0, -1)));

  // ---------------------------------------------------- turning by hand

  // Follow a solution by hand, or edit the cube when there is no solution to follow.
  function turned(face, layer, quarters) {
    const { model } = state;
    const { moves, rotate } = model.handTurn(face, layer, quarters);
    // The middle layer of an odd cube: the stickers say "both halves turned
    // the other way"; turning the picture along keeps it where the hand left it.
    if (rotate) view.spin(face, quarters);
    if (state.input === "scramble") {
      if (state.scramble.problem) return;
      // What led to the cube shown: the scramble and, of a solution, the part played.
      const done = state.solution && model.isSolved(player.facelets);
      if (player.position < player.length && !done) {
        const slice = model.handSlice(face, layer, quarters);
        player.turnByHand(slice === null ? moves : [slice]);
        return;
      }
      const played = model.plain([...player.moves.slice(0, player.position), ...player.manualMoves]);
      const before = done ? [] : [...state.scramble.moves, ...played];
      setScramble(model.simplify([...before, ...moves]));
    } else {
      const facelets = model.applyAll(player.facelets, moves);
      state.painted.set(state.size, { ...painted(), facelets, doubt: null });
      paint.set(state.size, facelets, painted().palette, null);
      show();
    }
  }
  view.onturn = turned;

  // U R F D L B turn the faces; with Shift, the other way.
  function keyTurn(face, quarters) {
    if (player.moving || view.turn || view.mode === "look") return;
    view.animateTurn({ face, from: 0, to: 1 }, 0, (quarters * Math.PI) / 2, 170).then(() => {
      view.setTurn(null);
      turned(face, 0, quarters);
    });
  }

  // ---------------------------------------------------------------- paint

  const paint = new PaintEditor($("paint"), {
    onchange: (facelets) => {
      const cube = painted();
      cube.facelets = facelets;
      if (cube.palette === defaultPalette) cube.palette = defaultPalette.slice();
      show();
    },
    onface: (face) => view.look(face),
  });
  view.onpaint = (i) => paint.paint(i);

  function repaint(facelets) {
    state.painted.set(state.size, { facelets, palette: defaultPalette, doubt: null });
    const p = painted();
    paint.set(state.size, p.facelets, p.palette, null);
    $("paint-note").textContent = PAINT_NOTE;
    show();
  }
  $("paint-reset").addEventListener("click", () => repaint(state.model.solved()));
  $("paint-blank").addEventListener("click", () => repaint(new Uint8Array(state.model.count).fill(BLANK)));

  // --------------------------------------------------------------- camera

  let guiding = 0;
  const scanner = new CameraScanner($("camera-frame"), $("camera-side"), {
    // The user sees the captured side come round to their left as the
    // next face turns towards the camera. Show both moves when needed.
    onstep: (face, via) => {
      clearTimeout(guiding);
      view.setFrame(face);
      if (via === undefined) {
        view.lookForCapture(face, state.scan ? 450 : 0);
      } else {
        view.lookForCapture(via, 420);
        guiding = setTimeout(() => view.lookForCapture(face, 520), 560);
      }
    },
    onface: (face, colours) => {
      const nn = state.size * state.size;
      for (let i = 0; i < nn; i++) {
        state.scan[face * nn + i] = colours ? `rgb(${colours[i].map(Math.round).join(",")})` : null;
      }
      view.setRaw(state.scan);
    },
    ondone: (samples) => {
      const n = state.size;
      // The colours are sorted without regard to where each picture belongs;
      // then the pictures are put together the way they make a cube.
      const sorted = sortColours(samples, n);
      const made = assemble(n, sorted);
      const lying = placed(n, made.labels, made);
      const doubt = placed(n, sorted.doubt, made);
      // Two stickers exchanged to make a cube are shown as doubtful, to be looked at.
      if (made.swapped) {
        const lies = placed(n, Int32Array.from(samples, (_, k) => k), made);
        for (const i of made.swapped) doubt[lies.indexOf(i)] = 0.7;
      }
      const { facelets, palette } = named(n, sorted, lying, doubt);
      state.painted.set(n, { facelets, palette, doubt });
      const unsure = doubt.filter((d) => d > 0.55).length;
      const faces = (list) => list.map((f) => FACE_NAMES[f].toLowerCase()).join(", ").replace(/, ([^,]*)$/, " and $1");
      const ringed = unsure > 0 ? ` ${count(unsure, "ringed sticker")} may be another colour: check ${unsure > 1 ? "them" : "it"}.` : " Check it against your cube.";
      // Short enough for the panel to hold everything without a scroll.
      let note = `Read from the camera.${ringed}`;
      if (made.kind === "placed" && made.moved.length === 0) {
        note = `The ${faces(made.turned)} ${made.turned.length > 1 ? "faces were" : "face was"} held turned round: put right.${ringed}`;
      } else if (made.kind === "placed") {
        note = `${made.moved.length > 2 ? "Some" : "Two"} faces were shown out of turn: put in their places.${ringed}`;
      } else if (made.kind === "recoloured") {
        note = "Two stickers that looked alike were exchanged to make a cube: the ringed ones. Check them.";
      } else if (made.kind === "several") {
        note = "These pictures fit together as more than one cube. Scan again, or correct it here.";
      } else if (made.kind === "none") {
        note = `The faces do not fit together as a cube: a colour was misread.${unsure > 0 ? " Check the ringed stickers." : " Correct it here, or scan again."}`;
      }
      $("paint-note").textContent = note;
      // The cube just read goes to where it can be looked over and corrected.
      setTab("paint");
    },
    onstop: () => {
      // What was read of an unfinished scan is dropped; the painted cube is as it was.
      clearTimeout(guiding);
      view.setFrame(null);
      state.scan = null;
      placeCube();
      setTouch();
      show();
      view.look(null);
    },
  });

  $("camera-start").addEventListener("click", async () => {
    const n = state.size;
    try {
      await scanner.start(n);
    } catch (error) {
      state.detail = CAMERA_PROBLEMS[error.name] ?? "The camera could not be started. Paint the cube instead.";
      restStatus();
      return;
    }
    // An empty cube that fills in as faces are captured; the painted cube is left alone until all six are.
    state.scan = new Array(6 * n * n).fill(null);
    view.setMarks(null);
    placeCube();
    setTouch();
    show();
    scanner.capture.focus({ preventScroll: true });
  });

  // ---------------------------------------------------------------- solve

  // 1st, 2nd, 3rd, 4th, 11th, 21st.
  const ordinal = (k) => k + (k % 100 >= 11 && k % 100 <= 13 ? "th" : ["th", "st", "nd", "rd"][k % 10] ?? "th");

  function onJob(job) {
    if (job.status === "browser") {
      drawStatus("busy", "Solving in your browser…");
      drawWait("Solving in your browser…");
    } else if (job.status === "queued") {
      // The line as it stands: those ahead, this one, those behind.
      const line = () => {
        const row = el("div", { class: "line", "aria-hidden": "true" });
        for (let k = 1; k <= job.waiting; k++) row.append(el("i", { class: k < job.position ? "ahead" : k === job.position ? "you" : "" }));
        return row;
      };
      // The place is the server's own number: 1 is the next cube to be solved.
      const place = job.position === 1 ? "next" : ordinal(job.position);
      const ahead = job.position === 1 ? "The cube before yours is being solved." : `${job.position - 1} waiting ahead of you, and one being solved.`;
      drawStatus("busy", ["You are ", el("b", { text: place }), " in line"], `${ahead} One cube is solved at a time.`, line());
      drawWait(["You are ", el("b", { text: place }), " in line"], `${ahead} One cube is solved at a time.`, line());
    } else if (job.status === "running" && job.phase === "loading") {
      // The first solve of a size builds its tables: what the solver says it is doing is shown as it goes.
      const what = job.progress || "The first time a size is solved its tables are built, which takes minutes.";
      drawStatus("busy", `Loading the tables for the ${name(job.size)}…`, what);
      drawWait(`Loading the tables for the ${name(job.size)}…`, what);
    } else if (job.status === "running") {
      const since = performance.now() - job.for_ms;
      // The seconds are for the eye only: a screen reader hears "Solving" once.
      const clocks = [el("span", { "aria-hidden": "true" }), el("span", { "aria-hidden": "true" })];
      const say = () => {
        const s = (performance.now() - since) / 1000;
        for (const clock of clocks) clock.textContent = ` ${s.toFixed(s < 10 ? 1 : 0)} s`;
      };
      const cap = solver.config.time_cap_s ? `A search may take ${solver.config.time_cap_s} s here.` : job.progress || "";
      drawStatus("busy", ["Solving…", clocks[0]], cap);
      drawWait(["Solving…", clocks[1]], cap);
      say();
      state.ticker = setInterval(say, 100);
    }
  }

  // Solve is one button in one place: it solves, and while a solve waits
  // on the server it cancels.
  function setBusy(busy) {
    const button = $("solve");
    const cancellable = busy && !solver.inBrowser(state.size);
    button.textContent = cancellable ? "Cancel" : "Solve";
    button.classList.toggle("primary", !cancellable);
    button.disabled = cancellable ? false : busy || !state.valid || !solvable();
    button.title = button.disabled && !busy ? $("valid").textContent : "";
  }

  async function solve() {
    if (state.job) {
      state.job.abort();
      state.job = null;
      setBusy(false);
      restStatus();
      return;
    }
    if (!state.valid || !solvable()) return;
    let cube = entered();
    if (!cube.kind) return;
    const { size, mode, model } = state;
    const shown = player.facelets;
    let newScramble;
    let newPaint;
    // Take the cube on screen, including any turns made by hand.
    if (state.solution && (player.position > 0 || player.manualMoves.length)) {
      if (model.isSolved(shown)) {
        state.result = ["good", "This cube is solved.", "Scramble it, or turn a layer, to go again."];
        restStatus();
        return;
      }
      if (cube.kind === "moves") {
        const played = model.plain([...player.moves.slice(0, player.position), ...player.manualMoves]);
        newScramble = model.simplify([...state.scramble.moves, ...played]);
        if (newScramble.length > solver.config.max_scramble_moves) {
          state.result = ["bad", `The current cube needs ${newScramble.length} scramble moves. The limit is ${solver.config.max_scramble_moves}.`];
          restStatus();
          return;
        }
        cube = { ...cube, start: shown, input: model.names(newScramble) };
      } else {
        newPaint = { ...painted(), facelets: shown, doubt: null };
        cube = { ...cube, start: shown, input: model.toText(shown) };
      }
    }
    const job = new AbortController();
    state.job = job;
    state.result = null;
    setBusy(true);
    const first = solver.inBrowser(size) ? "Solving in your browser…" : solver.config.local ? "Solving…" : "Sending the cube to the server…";
    drawStatus("busy", first);
    drawWait(first);
    let result = null;
    let failure = null;
    try {
      result = await solver.solve({ size, kind: cube.kind, input: cube.input, mode }, onJob, job.signal);
    } catch (error) {
      failure = error;
    }
    if (state.job !== job) return; // cancelled, or the cube changed meanwhile
    state.job = null;
    setBusy(false);
    if (player.facelets !== shown || player.moving || view.turn) {
      restStatus();
      return;
    }
    if (failure) {
      if (failure.name === "AbortError") {
        state.result = null;
      } else if (failure.impossible) {
        state.detail = problemNodes(failure);
        setValid("bad", `Cannot be solved: ${(IMPOSSIBLE[failure.code]?.[0] ?? failure.message).toLowerCase()}`);
      } else {
        const title = FAILURES[failure.code];
        state.result = ["bad", title ? [el("b", { text: `${title}. ` }), failure.message] : failure.message || "Something went wrong."];
      }
      restStatus();
      return;
    }
    const moves = state.model.parse(result.solution);
    // The solver checks its answer on its own model; this is the page's model agreeing.
    if (!state.model.isSolved(state.model.applyAll(cube.start, moves))) {
      state.result = ["bad", "The answer that came back does not solve this cube. That is a bug; nothing is shown."];
      restStatus();
      return;
    }
    if (newScramble) {
      $("scramble").value = cube.input;
      state.scramble = { moves: newScramble, problem: null };
      $("undo").disabled = newScramble.length === 0;
      judge(entered());
    } else if (newPaint) {
      state.painted.set(size, newPaint);
      paint.set(size, newPaint.facelets, newPaint.palette, null);
      view.setMarks(null);
    }
    state.solution = { ...result, moves, label: size === 2 ? "the fewest there are" : `${mode} search` };
    state.shows = state.input === "scramble" ? "scramble" : "painted";
    renderSolution(state.solution);
    player.load(model, cube.start, moves);
    const secs = result.ms / 1000;
    const took = secs < 1 ? `${Math.max(1, Math.round(result.ms))} ms` : `${secs.toFixed(1)} s`;
    if (moves.length === 0) {
      state.result = ["good", "This cube is already solved."];
    } else {
      const where = { browser: "in your browser", computer: "on this computer", server: "on the server" }[result.where];
      state.result = [
        "good",
        [el("b", { text: count(moves.length, "move") }), ` in ${took}, found ${where}.`],
        result.optimal ? "No shorter solution exists. Press play, or step through it." : "Press play, or step through it.",
      ];
      if (state.input !== "scramble") view.look(null);
      if (narrow.matches) setTab("moves");
    }
    restStatus();
  }

  $("solve").addEventListener("click", solve);

  // ------------------------------------------------------------- playback

  function renderSolution(solution) {
    const list = $("moves");
    const copy = $("copy");
    const none = !solution || solution.moves.length === 0;
    $("placeholder").hidden = !none;
    copy.hidden = none;
    if (none) {
      list.replaceChildren();
      $("summary").textContent = "Solution";
      $("placeholder").textContent = solution
        ? "Already solved: nothing to do."
        : solvable()
          ? "The moves are listed here once the cube is solved. Nothing plays until you press play or step."
          : "A cube to turn by hand: drag a layer, or press U R F D L B.";
      return;
    }
    const { moves, parts } = solution;
    const names = moves.map((m) => state.model.moveName(m));
    const groups = parts?.length ? parts : [{ name: "Solution", moves: moves.length }];
    let k = 0;
    list.replaceChildren(
      ...groups.map((part) => {
        const ol = el("ol");
        for (let i = 0; i < part.moves; i++, k++) {
          ol.append(el("li", {}, [el("button", { type: "button", class: "move", "data-k": k, text: names[k] })]));
        }
        const head = el("h4", { text: part.name }, [el("span", { text: String(part.moves) })]);
        return el("div", { class: "part" }, groups.length > 1 ? [head, ol] : [ol]);
      }),
    );
    $("summary").replaceChildren(el("b", { text: count(solution.found ?? moves.length, "move") }), solution.label ?? "");
    copy.textContent = "Copy";
  }

  $("moves").addEventListener("click", (e) => {
    const chip = e.target.closest(".move");
    if (chip) player.seek(Number(chip.dataset.k) + 1);
  });

  $("copy").addEventListener("click", async () => {
    if (!state.solution) return;
    try {
      await navigator.clipboard.writeText(state.model.names(state.solution.moves));
      $("copy").textContent = "Copied";
    } catch {
      $("copy").textContent = "Not copied";
    }
  });

  let chips = [];
  let marked = -1;
  const fraction = el("small", { class: "move-progress" });
  player.onchange = () => {
    const { position, length, playing } = player;
    // Under Paint with the scramble's cube still on screen, the cube to
    // paint is the cube on screen, wherever its solution has been played to.
    if (state.input !== "scramble" && state.shows === "scramble" && !player.moving && !state.scan && state.size <= PAINT_MAX) {
      const p = painted();
      if (state.model.toText(p.facelets) !== state.model.toText(player.facelets)) {
        state.painted.set(state.size, { facelets: player.facelets.slice(), palette: defaultPalette, doubt: null });
        paint.set(state.size, painted().facelets, painted().palette, null);
      }
    }
    $("at").textContent = String(position);
    $("total").textContent = String(length);
    const scrub = $("scrub");
    scrub.max = String(length);
    scrub.value = String(position);
    scrub.disabled = length === 0;
    const play = $("play");
    play.classList.toggle("playing", playing);
    play.setAttribute("aria-label", playing ? "Pause" : "Play");
    play.disabled = length === 0;
    $("to-start").disabled = $("step-back").disabled = !player.canStepBack;
    $("to-end").disabled = $("step-on").disabled = position >= length;
    // Once the solution is being followed, the line that announced it is in the way.
    if (state.result?.[0] === "good" && !state.job && (playing || player.canStepBack)) {
      state.result = null;
      restStatus();
    }
    const detour = player.detour.length > 0;
    const progress = player.partial?.fraction ?? "";
    const next = player.cue && position < length
      ? state.model.moveName(detour ? player.nextMove : player.moves[position])
      : null;
    $("next").hidden = next === null;
    $("next-label").textContent = detour ? "back" : "next";
    $("next-move").textContent = next && progress && !detour ? `${next} ${progress}` : next ?? "";
    const off = $("detour");
    off.hidden = !detour;
    $("solution").classList.toggle("off-solution", detour);
    const correction = detour ? `Back to the solution · ${state.model.moveName(player.nextMove)}` : "";
    const correctionChanged = off.textContent !== correction;
    if (correctionChanged) off.textContent = correction;
    // The moves done so far, and the one that is next.
    const list = $("moves");
    if (chips.length !== length || chips[0]?.isConnected === false) {
      chips = [...list.querySelectorAll(".move")];
      marked = -1;
    }
    const moved = marked !== position;
    if (moved && chips.length) {
      chips.forEach((chip, k) => {
        chip.classList.toggle("done", k < position);
        chip.classList.toggle("now", k === position);
        if (k === position) chip.setAttribute("aria-current", "step");
        else chip.removeAttribute("aria-current");
      });
      marked = position;
    }
    const fractionText = progress ? ` ${progress}` : "";
    const fractionChanged = fraction.textContent !== fractionText;
    if (fractionChanged) fraction.textContent = fractionText;
    if (progress && chips[position] && fraction.parentNode !== chips[position]) {
      chips[position].append(fraction);
    } else if (!progress) {
      fraction.remove();
    }
    if (chips.length && (moved || fractionChanged || correctionChanged)) {
      const now = chips[Math.min(position, chips.length - 1)];
      const [top, bottom] = [now.offsetTop, now.offsetTop + now.offsetHeight];
      if (top < list.scrollTop) list.scrollTop = top - 6;
      else if (bottom > list.scrollTop + list.clientHeight) list.scrollTop = bottom - list.clientHeight + 6;
    }
    keepLater();
  };

  $("play").addEventListener("click", () => player.toggle());
  $("step-back").addEventListener("click", () => player.step(-1));
  $("step-on").addEventListener("click", () => player.step(1));
  $("to-start").addEventListener("click", () => player.seek(0));
  $("to-end").addEventListener("click", () => player.seek(player.length));
  $("scrub").addEventListener("input", (e) => player.seek(Number(e.target.value)));

  for (const b of root.querySelectorAll(".speed button")) {
    b.addEventListener("click", () => {
      for (const other of root.querySelectorAll(".speed button")) other.setAttribute("aria-checked", String(other === b));
      player.setSpeed(Number(b.dataset.speed));
      keepLater();
    });
  }

  for (const b of root.querySelectorAll(".modes button")) {
    b.addEventListener("click", () => {
      for (const other of root.querySelectorAll(".modes button")) other.setAttribute("aria-checked", String(other === b));
      state.mode = b.dataset.mode;
      keepLater();
    });
  }

  // The arrows for the next move: on unless switched off, and remembered on this device.
  function setArrows(on, remember = true) {
    $("arrows").setAttribute("aria-pressed", String(on));
    player.setCue(on);
    player.onchange();
    if (!remember) return;
    try {
      localStorage.setItem("anycube.arrows", on ? "on" : "off");
    } catch {
      // Private windows and the like: the choice lasts for this visit.
    }
  }
  $("arrows").addEventListener("click", () => setArrows($("arrows").getAttribute("aria-pressed") !== "true"));
  $("reset-view").addEventListener("click", () => view.look(null));

  const settingsOptions = $("settings-options");
  function setTopColour(colour) {
    settingsOptions.querySelector(`input[value="${colour}"]`).checked = true;
    const previous = defaultPalette;
    defaultPalette = colour === "yellow" ? YELLOW_TOP : STICKERS;
    if (previous === defaultPalette) return;
    for (const cube of state.painted.values()) {
      if (cube.palette === previous) cube.palette = defaultPalette;
    }
    const current = state.painted.get(state.size);
    if (current?.palette === defaultPalette && paint.palette === previous) {
      paint.set(state.size, current.facelets, current.palette, current.doubt);
    }
    if (view.palette === previous) showPalette(defaultPalette);
    keepLater();
  }

  $("open-settings").addEventListener("click", () => $("settings").showModal());
  let savingSettings = Promise.resolve();
  settingsOptions.addEventListener("change", (event) => {
    if (!event.target.matches('input[name="top-colour"]')) return;
    const colour = event.target.value;
    setTopColour(colour);
    const status = $("settings-status");
    status.hidden = true;
    savingSettings = savingSettings.then(async () => {
      if (solver.config.local && solver.config.top_colour) {
        const response = await fetch("/api/settings", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ top_colour: colour }),
        });
        if (!response.ok) throw new Error("Could not save settings");
        try { localStorage.setItem(TOP_COLOUR, colour); } catch { /* The local server already saved it. */ }
      } else {
        localStorage.setItem(TOP_COLOUR, colour);
      }
    }).catch(() => {
      if (!settingsOptions.querySelector(`input[value="${colour}"]`).checked) return;
      status.textContent = "Could not save the preference. It will apply for this visit.";
      status.hidden = false;
    });
  });

  // Space plays and pauses, the arrows step, the face letters turn the cube,
  // wherever the keys are not needed for something else.
  document.addEventListener("keydown", (e) => {
    if (e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey) return;
    const t = e.target;
    if (t.isContentEditable || t.closest?.("dialog, textarea, input, select, [role=tab]")) return;
    if (e.key === " " && scanner.running) {
      if (t !== scanner.capture && t.closest?.("button, [role=button]")) return;
      if (!e.repeat) scanner.capture.click();
      e.preventDefault();
      return;
    }
    const face = FACE_LETTERS.indexOf(e.key.toUpperCase());
    if (face >= 0 && e.key.length === 1) {
      if (view.mode === "turn") keyTurn(face, e.shiftKey ? -1 : 1);
      e.preventDefault();
      return;
    }
    if (!player.length || t.closest?.("canvas")) return;
    if (e.key === " " && !t.closest?.("button")) {
      player.toggle();
      e.preventDefault();
    } else if (e.key === "ArrowLeft") {
      player.step(-1);
      e.preventDefault();
    } else if (e.key === "ArrowRight") {
      player.step(1);
      e.preventDefault();
    }
  });

  // The page put back as it was kept.
  function restore(was) {
    setSize(was.size);
    $("scramble").value = String(was.scramble ?? "");
    if (was.painted && was.size <= PAINT_MAX) {
      const { facelets, palette, followsDefault, doubt, note } = was.painted;
      state.painted.set(was.size, { facelets, palette: followsDefault ? defaultPalette : palette, doubt: doubt ? Float32Array.from(doubt) : null });
      if (note) $("paint-note").textContent = note;
    }
    state.carried = typeof was.carried === "string" ? was.carried : null;
    for (const b of root.querySelectorAll(".modes button")) {
      if (b.dataset.mode !== was.mode || b.disabled) continue;
      for (const other of root.querySelectorAll(".modes button")) other.setAttribute("aria-checked", String(other === b));
      state.mode = was.mode;
    }
    for (const b of root.querySelectorAll(".speed button")) {
      if (Number(b.dataset.speed) !== was.speed) continue;
      for (const other of root.querySelectorAll(".speed button")) other.setAttribute("aria-checked", String(other === b));
      player.setSpeed(was.speed);
    }
    state.input = was.input;
    showTab(was.tab === "moves" && narrow.matches ? "moves" : was.input);
    setTouch();
    if (was.input !== "scramble" && was.size <= PAINT_MAX) {
      const p = painted();
      paint.set(was.size, p.facelets, p.palette, p.doubt);
    }
    readScramble();
    if (was.input !== "scramble") show();
    if (was.solution && was.moves.length) {
      // The cube the solution is of: the scramble's even under Paint, if it was that cube on screen.
      const ofScramble = was.shows === "scramble" && !state.scramble.problem;
      const start = ofScramble ? state.model.applyAll(state.model.solved(), state.scramble.moves) : entered().start;
      // Once more against the cube the page itself arrived at.
      if (state.model.isSolved(state.model.applyAll(start, was.moves))) {
        state.solution = { moves: was.moves, parts: was.solution.parts, label: was.solution.label, optimal: was.solution.optimal, found: was.solution.found };
        player.load(state.model, start, was.moves);
        state.shows = ofScramble ? "scramble" : "painted";
        showPalette(ofScramble ? defaultPalette : entered().palette);
        renderSolution(state.solution);
        player.seek(was.position, was.turns);
      }
    }
    if (was.rot) view.rot = was.rot;
    // The arrows of the move that was next are there as they were, not coming in afresh.
    view.cueSince = -1e9;
    view.draw();
  }

  // ---------------------------------------------------------------- start

  narrow.addEventListener("change", arrange);
  arrange();
  let arrows = true;
  try {
    arrows = localStorage.getItem("anycube.arrows") !== "off";
  } catch {
    // No storage: the arrows are on.
  }
  setArrows(arrows, false);
  // The kept cube is on screen already: with the arrows of the move that was next.
  if (page && arrows && page.position < page.moves.length && !page.turns.partial.length && !page.turns.detour.length) {
    const { face, from, to, quarters } = page.model.info(page.moves[page.position]);
    view.setCue({ face, from, to, quarters }, true);
  }
  renderSolution(null);
  await solver.loadConfig();
  const storedColour = defaultPalette === YELLOW_TOP ? "yellow" : "white";
  const preferred = solver.config.top_colour ?? storedColour;
  setTopColour(preferred === "yellow" ? "yellow" : "white");
  settingsOptions.disabled = false;
  // Whether the browser can solve by itself decides what the page says about each size.
  await solver.startBrowser();
  renderSizes();
  if (page) {
    try {
      restore(page);
    } catch {
      page = null;
    }
  }
  if (!page) {
    // A first visit: a 3x3x3, scrambled, and seen only once it is.
    setSize(Math.min(3, solver.config.max_size));
    await randomScramble();
    view.waiting = false;
    view.draw();
  }
  keeping = true;
  keep();
  // Leaving the page (a reload is that too) keeps it as it stands at that moment.
  window.addEventListener("pagehide", keep);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") keep();
  });

  // For the host page, the curious, and testing from the console.
  const app = { view, player, solver, scanner, state, config: solver.config };
  window.anycube = app;
  return app;
}
