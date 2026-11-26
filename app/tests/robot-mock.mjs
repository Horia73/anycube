// A mock of the anycube server's robot API (robots-spec/API.md) as a
// fetch function: for the tests, and behind tests/robot-mock-server.mjs
// for the page in a browser.
//
// The robot as the server shows it: a scan takes six photos, waiting for
// photo-taken after each (which face and which turn each is, only its event
// says: the server plans the scan, and the order differs with where the
// robot stands); a solve plans, then runs its ops one after another. A
// command must come as JSON from this page, as the server wants it.
// `script` changes what it does:
//
//   moves          the solution the planner finds (names, as anycube writes them)
//   stepMs         time between steps (2: the tests are quick)
//   settleMs       from turning a face to the camera to its `photo` event
//   opMs(op)       how long an op takes (stepMs unless given)
//   computeMs      how long the planner takes
//   maxWaitMs      the longest an events request is held
//   failAfterPhoto an error instead of the photo after this one
//   down(path)     true: that request fails as with no server
//   ports          the serial ports plugged in to begin with (the robot's one unless given)
//   identify       true: the server says of each port what it is (`ports` of /api/robots)
//   connectFails   how many times connecting fails before it works
//   quietUnplug    true: an unplugged robot is not announced (a server without hot-plug)
//   nextMs         given with each photo as the time to the next (`next_ms`)
//   photos         the scan's pictures, [face, rotate] in order (PHOTOS unless given)
//   origin         the Origin the page's requests carry (none unless given; not HOST's is refused)
//
// `plug(path, about)` and `unplug(path)` are the cable: `about` is what an
// identifying server says of the port (`{driver}`, `{driver: null}`, `{busy: true}`).

export const json = (status, body) => ({ ok: status < 400, status, json: async () => body });

// The server's scan of a 3x3x3 from the parked robot.
export const PHOTOS = [["L", 0], ["U", 90], ["R", 180], ["B", 180], ["D", 0], ["F", 0]];

export const HOST = "http://127.0.0.1";

export const DRIVERS = [{ id: "multicuber", name: "MultiCuber", sizes: [2, 3, 4, 5], ports: ["/dev/cu.usbmodem21301"] }];

export function mockServer(script = {}) {
  const events = [];
  const calls = [];
  let seq = 0;
  let runs = 0;
  let run = null;
  let began = 0;
  let times = {};
  let stopped = false;
  let waiting = [];
  let connected = script.connected ?? null;
  const server = { events, calls, holding: -1 };
  // The ports plugged in: path -> what an identifying server knows of it.
  const plugged = new Map((script.ports ?? DRIVERS[0].ports).map((path) => [path, { driver: DRIVERS[0].id }]));
  let refusals = script.connectFails ?? 0;
  let pulled = false; // the robot connected was unplugged
  let moving = false; // a scan or a solve is under way
  const listing = () => ({
    drivers: DRIVERS.map((d) => ({ ...d, ports: [...plugged.keys()] })),
    ...(script.identify ? { ports: [...plugged].map(([path, { stranger, ...about }]) => ({ path, busy: false, ...about })) } : {}),
    connected,
  });
  server.plug = (path = DRIVERS[0].ports[0], about = { driver: DRIVERS[0].id }) => plugged.set(path, about);
  server.unplug = (path = DRIVERS[0].ports[0]) => {
    plugged.delete(path);
    if (connected?.port !== path) return;
    connected = null;
    pulled = true;
    stopped = true;
    if (script.quietUnplug) return;
    // As the server does: the run that was going on ends, then the robot is said to be gone.
    if (moving) push("error", { code: "disconnected", message: "The robot was pulled out." });
    moving = false;
    run = null;
    push("disconnected", { message: "The robot's cable was pulled." });
  };
  Object.defineProperty(server, "connected", { get: () => connected });
  const push = (type, body = {}) => {
    events.push({ seq: ++seq, type, ...(run && type !== "log" ? { run, t_ms: Math.round(Date.now() - began) } : {}), ...body });
    for (const wake of waiting.splice(0)) wake();
  };
  const after = (ms, fn) => setTimeout(() => !stopped && fn(), ms);
  const step = (fn) => after(script.stepMs ?? 2, fn);
  // The robot turns a face to the camera, waits for it to settle, asks for the photo.
  const photos = script.photos ?? PHOTOS;
  const photo = (index) => {
    server.holding = index;
    after(script.settleMs ?? 0, () => push("photo", { index, face: photos[index][0], rotate: photos[index][1], ...(script.nextMs === undefined ? {} : { next_ms: script.nextMs }) }));
  };
  server.push = push;
  server.fetch = async (url, init = {}) => {
    const method = init.method ?? "GET";
    const body = init.body ? JSON.parse(init.body) : undefined;
    const u = new URL(url, "http://127.0.0.1");
    calls.push({ method, path: u.pathname, body });
    if (script.down?.(u.pathname)) throw new TypeError("Failed to fetch");
    if (method === "POST" && u.pathname.startsWith("/api/robot/")) {
      const header = (name) => Object.entries(init.headers ?? {}).find(([k]) => k.toLowerCase() === name)?.[1];
      const origin = header("origin") ?? script.origin;
      if (origin !== undefined && origin !== HOST) return json(403, { ok: false, error: { code: "forbidden", message: "robot commands must come from this app" } });
      if (String(header("content-type") ?? "").split(";")[0].trim().toLowerCase() !== "application/json") {
        return json(415, { ok: false, error: { code: "bad_request", message: "robot commands must be sent as JSON (Content-Type: application/json)" } });
      }
    }
    switch (u.pathname) {
      case "/api/robots":
        return json(200, listing());
      case "/api/robot/connect": {
        const path = body.port ?? plugged.keys().next().value;
        if (!plugged.has(path)) return json(200, { ok: false, error: { code: "no_port", message: "No such serial port." } });
        // A board that went by its name is asked now, and is no robot: the server knows it from here on.
        if (script.identify && plugged.get(path).stranger) {
          plugged.set(path, { driver: null });
          return json(200, { ok: false, error: { code: "no_answer", message: "the board does not answer as MultiCuber" } });
        }
        if (refusals-- > 0) return json(200, { ok: false, error: { code: "no_answer", message: "The board did not answer." } });
        connected = { driver: body.driver, port: path, size: body.size ?? script.size ?? 3, info: { firmware: "2.2" } };
        pulled = false;
        return json(200, { ok: true, size: connected.size, info: connected.info, port: connected.port });
      }
      case "/api/robot/disconnect":
        connected = null;
        return json(200, { ok: true });
      case "/api/robot/events": {
        const since = Number(u.searchParams.get("since"));
        const wait = Number(u.searchParams.get("wait"));
        if (!events.some((e) => e.seq > since) && wait > 0) {
          await new Promise((resolve, reject) => {
            waiting.push(resolve);
            setTimeout(resolve, Math.min(wait * 1000, script.maxWaitMs ?? 30));
            init.signal?.addEventListener("abort", () => reject(Object.assign(new Error("aborted"), { name: "AbortError" })));
          });
        }
        return json(200, { seq, events: events.filter((e) => e.seq > since) });
      }
      case "/api/robot/scan": {
        if (pulled) return json(200, { ok: false, error: { code: "not_connected", message: "No robot is connected." } });
        moving = true;
        run = `run-${++runs}`;
        began = Date.now();
        stopped = false;
        times = {};
        step(() => {
          push("scan-start", { steps: 22, eta_ms: 9000 });
          photo(0);
        });
        return json(200, { ok: true, run });
      }
      case "/api/robot/photo-taken": {
        if (body.run !== run) return json(409, { ok: false, error: { code: "stale_run", message: "Not the current run." } });
        const next = body.index + 1;
        server.holding = -1; // turning to the next face
        if (script.failAfterPhoto === body.index) {
          step(() => push("error", { code: "jam", message: "The lift did not reach its position." }));
        } else if (next < 6) {
          step(() => photo(next));
        } else {
          step(() => {
            times.scan_ms = Date.now() - began;
            push("scan-done", { scan_ms: times.scan_ms });
            moving = false; // waiting for the page's cube
          });
        }
        return json(200, { ok: true });
      }
      case "/api/robot/solve": {
        if (body.run !== run) return json(409, { ok: false, error: { code: "stale_run", message: "Not the current run." } });
        const moves = script.moves;
        moving = true;
        const ops = moves.flatMap((m, k) => [
          { cmd: "LIFT", arg: "1", est_ms: 425, move: null },
          { cmd: "ROT", arg: m.endsWith("2") ? "2" : "1", est_ms: m.endsWith("2") ? 672 : 569, move: k },
        ]);
        const solving = Date.now();
        after(script.computeMs ?? script.stepMs ?? 2, () => {
          times.compute_ms = Date.now() - solving;
          push("plan", { moves, ops, eta_ms: ops.reduce((a, o) => a + o.est_ms, 0), compute_ms: times.compute_ms, candidates: 6, metric: "robot-ms" });
          const executing = Date.now();
          let i = 0;
          const next = () => {
            if (i === ops.length) {
              moving = false;
              push("done", { times: { scan_ms: times.scan_ms, analysis_ms: body.analysis_ms, compute_ms: times.compute_ms, exec_ms: Date.now() - executing, total_ms: Date.now() - began } });
              return;
            }
            const op = ops[i];
            const ms = script.opMs?.(op) ?? script.stepMs ?? 2;
            if (op.move !== null) push("move", { move: op.move, est_ms: op.est_ms });
            after(ms, () => {
              push("op", { i, cmd: op.cmd, arg: op.arg, actual_ms: script.opMs ? ms : op.est_ms * 1.1 });
              i++;
              next();
            });
          };
          next();
        });
        return json(200, { ok: true });
      }
      case "/api/robot/stop":
        stopped = true;
        moving = false;
        setTimeout(() => push("error", { code: "stopped", message: "Stopped: the motors are off." }), 0);
        return json(200, { ok: true });
      default:
        return json(404, { ok: false, error: { code: "not_found", message: "no such endpoint" } });
    }
  };
  return server;
}
