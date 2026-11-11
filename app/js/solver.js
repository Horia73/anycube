// Where a cube gets solved: in this browser for the sizes it has a solver
// for, on the server for the rest, and on the server for everything if the
// browser's solver cannot start.

export class SolveError extends Error {
  constructor({ code, message, impossible = false }, retryAfter = null) {
    super(message);
    this.code = code;
    this.impossible = impossible;
    this.retryAfter = retryAfter;
  }
}

const DEFAULTS = { max_size: 3, browser_sizes: [2, 3], modes: ["fast", "normal"], queue: 0, time_cap_s: 60, repo_url: null, offline: true };

export class Solver {
  constructor() {
    this.config = DEFAULTS;
    this.worker = null;
    this.calls = new Map();
    this.nextCall = 1;
    // null until tried; then whether this browser can solve by itself.
    this.browser = null;
  }

  // What the server accepts. Without a server the page still solves what the browser can.
  async loadConfig() {
    try {
      const response = await fetch("/api/config");
      if (response.ok) this.config = { ...DEFAULTS, ...(await response.json()), offline: false };
    } catch {
      // Offline, or the files are served by something that is not the solver's server.
    }
    return this.config;
  }

  #call(message) {
    return new Promise((resolve, reject) => {
      const id = this.nextCall++;
      this.calls.set(id, { resolve, reject });
      this.worker.postMessage({ id, ...message });
    });
  }

  // Starts the browser's solver. Resolves to whether it works here.
  startBrowser() {
    if (!this.starting) {
      this.starting = (async () => {
        try {
          this.worker = new Worker(new URL("./solver-worker.js", import.meta.url), { type: "module" });
          this.worker.onmessage = ({ data }) => {
            const call = this.calls.get(data.id);
            this.calls.delete(data.id);
            if (!call) return;
            if (data.crashed) call.reject(new Error(data.crashed));
            else call.resolve(data.result);
          };
          this.worker.onerror = (event) => {
            event.preventDefault?.();
            for (const call of this.calls.values()) call.reject(new Error("the solver module could not be loaded"));
            this.calls.clear();
          };
          await this.#call({ op: "start" });
          this.browser = true;
        } catch {
          this.browser = false;
          this.worker?.terminate();
          this.worker = null;
        }
        return this.browser;
      })();
    }
    return this.starting;
  }

  // Builds the tables of a size ahead of the first solve.
  async warm(size) {
    if ((await this.startBrowser()) && this.config.browser_sizes.includes(size)) {
      await this.#call({ op: "prepare", size }).catch(() => {});
    }
  }

  /**
   * A scramble to a position picked at random from all there are, for the
   * sizes the browser solves. Resolves to null where that cannot be done.
   */
  async randomState(size) {
    if (!this.config.browser_sizes.includes(size)) return null;
    try {
      if (await this.startBrowser()) {
        const seed = [...crypto.getRandomValues(new Uint32Array(2))];
        const r = await this.#call({ op: "scramble", size, seed });
        return r.scramble || null;
      }
      // No solver in the browser: a server on this machine has one.
      if (!this.config.local) return null;
      const response = await fetch(`/api/scramble?size=${size}`);
      return response.ok ? (await response.json()).scramble || null : null;
    } catch {
      return null;
    }
  }

  // Where a size is solved, in words.
  where(size) {
    return this.inBrowser(size) ? "solved in your browser" : this.config.local ? "solved on this computer" : "solved on the server";
  }

  inBrowser(size) {
    return this.browser !== false && this.config.browser_sizes.includes(size);
  }

  /**
   * Checks a cube of any size without solving it. Resolves to null when it
   * is fine or cannot be checked here (the server checks again in any case),
   * and to `{ code, message, impossible }` when something is wrong with it.
   */
  async check(size, kind, input) {
    try {
      if (await this.startBrowser()) {
        const r = await this.#call({ op: "check", size, kind, input });
        return r.ok ? null : { code: r.code, message: r.message, impossible: r.impossible };
      }
      // No solver in the browser: a server on this machine answers at once.
      if (!this.config.local) return null;
      const response = await fetch("/api/check", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ size, [kind]: input }),
      });
      const r = await response.json();
      return r.ok ? null : r.error;
    } catch {
      return null;
    }
  }

  /**
   * Solves a cube. `onStatus` hears where a server job stands (`queued` with
   * its position, `running` with its phase); `signal` cancels it.
   * Resolves to `{ solution, length, parts, optimal, ms, where }`.
   */
  async solve({ size, kind, input, mode }, onStatus = () => {}, signal = undefined) {
    if (this.config.browser_sizes.includes(size) && (await this.startBrowser())) {
      onStatus({ status: "browser" });
      try {
        const r = await this.#call({ op: "solve", size, kind, input, mode });
        if (!r.ok) throw new SolveError(r);
        const length = r.solution ? r.solution.split(" ").length : 0;
        return { solution: r.solution, length, parts: [], optimal: r.optimal, ms: r.ms, where: "browser" };
      } catch (error) {
        if (error instanceof SolveError) throw error;
        // The module broke: from here on these sizes go to the server too.
        this.browser = false;
      }
    }
    if (this.config.offline) {
      throw new SolveError({
        code: "offline",
        message: "This page has no solver behind it, and the browser's own solver could not start.",
      });
    }
    return this.#server({ size, kind, input, mode }, onStatus, signal);
  }

  async #server({ size, kind, input, mode }, onStatus, signal) {
    const unreachable = () => new SolveError({ code: "network", message: "The server could not be reached. Check the connection and try again." });
    let job;
    try {
      const response = await fetch("/api/solve", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ size, mode, [kind]: input }),
        signal,
      });
      const body = await response.json().catch(() => null);
      if (!response.ok) {
        const error = body?.error ?? { code: "http", message: `The server answered ${response.status}.` };
        throw new SolveError(error, Number(response.headers.get("retry-after")) || null);
      }
      job = body;
    } catch (error) {
      if (error instanceof SolveError || error.name === "AbortError") throw error;
      throw unreachable();
    }
    const cancel = () => fetch(`/api/solve/${job.id}`, { method: "DELETE", keepalive: true }).catch(() => {});
    const onHide = () => cancel();
    window.addEventListener("pagehide", onHide);
    try {
      let failures = 0;
      for (;;) {
        onStatus(job);
        if (job.status === "done") {
          return { solution: job.solution, length: job.length, parts: job.parts, optimal: job.optimal, ms: job.time_ms, where: this.config.local ? "computer" : "server" };
        }
        if (job.status === "failed") throw new SolveError(job.error);
        try {
          const response = await fetch(`/api/solve/${job.id}?since=${job.epoch}&wait=10`, { signal });
          const body = await response.json().catch(() => null);
          if (response.status === 404) throw new SolveError(body?.error ?? { code: "not_found", message: "The server no longer has this solve." });
          if (!response.ok || !body) throw new Error("bad answer");
          job = body;
          failures = 0;
        } catch (error) {
          if (error instanceof SolveError) throw error;
          if (error.name === "AbortError") {
            cancel();
            throw error;
          }
          // A dropped connection: try a few times before giving up.
          if (++failures > 3) throw unreachable();
          await new Promise((r) => setTimeout(r, 1000 * failures));
        }
      }
    } finally {
      window.removeEventListener("pagehide", onHide);
    }
  }
}
