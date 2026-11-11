// The WebAssembly build of the 2x2x2 and 3x3x3 solvers (wasm/), wrapped.
//
// No binding generator is used: text goes in and out through the module's
// memory. This file has no dependencies and does no I/O, so the page's
// worker and the tests under Node use it alike.

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class WasmSolver {
  /** @param {BufferSource} bytes the contents of anycube_wasm.wasm */
  static async create(bytes) {
    const module = await WebAssembly.compile(bytes);
    // The module asks nothing of its host; anything it did ask for would be a build mistake.
    const wanted = WebAssembly.Module.imports(module);
    if (wanted.length > 0) {
      throw new Error("unexpected imports: " + wanted.map((i) => `${i.module}.${i.name}`).join(", "));
    }
    const instance = await WebAssembly.instantiate(module, {});
    return new WasmSolver(instance.exports);
  }

  constructor(exports) {
    this.x = exports;
  }

  #write(bytes) {
    const ptr = this.x.alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer, ptr, bytes.length).set(bytes);
    return ptr;
  }

  #read(ptr) {
    const len = new DataView(this.x.memory.buffer).getUint32(ptr, true);
    const text = decoder.decode(new Uint8Array(this.x.memory.buffer, ptr + 4, len));
    this.x.free_text(ptr);
    return text;
  }

  /** Builds the tables of a size (2 or 3); a solve does it by itself when needed. */
  prepare(size) {
    this.x.prepare(size);
  }

  #call(run, input) {
    const bytes = encoder.encode(input);
    const ptr = this.#write(bytes);
    let out;
    try {
      out = this.#read(run(ptr, bytes.length));
    } finally {
      this.x.dealloc(ptr, bytes.length);
    }
    const lines = out.split("\n");
    if (lines[0] === "ok") {
      return { ok: true, solution: lines[1], facelets: lines[2], optimal: lines[3] === "1" };
    }
    return { ok: false, code: lines[1], impossible: lines[2] === "1", message: lines.slice(3).join(" ") };
  }

  /**
   * A scramble that leads to a 2x2x2 or 3x3x3 picked at random from all
   * its positions. `seed` is two 32-bit numbers of the caller's randomness.
   */
  randomState(size, seed) {
    return this.#read(this.x.random_state_scramble(size, seed[0] >>> 0, seed[1] >>> 0));
  }

  /** Checks a cube of any size. `kind` is "moves" or "facelets". */
  check(size, kind, input) {
    return this.#call((p, n) => this.x.check_cube(size, kind === "moves" ? 0 : 1, p, n), input);
  }

  /** Solves a 2x2x2 or 3x3x3. `mode` is "fast" or "normal". */
  solve(size, kind, mode, input) {
    return this.#call(
      (p, n) => this.x.solve_cube(size, kind === "moves" ? 0 : 1, mode === "fast" ? 0 : 1, p, n),
      input,
    );
  }
}
