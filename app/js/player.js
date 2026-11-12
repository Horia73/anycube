// Plays a sequence of moves on a CubeView: play, pause, step either way,
// jump to any point, at a chosen speed. With the cue on, the move that
// comes next is drawn on the cube as arrows, which turn with its layers
// and then give way to those of the move after it; in play each move waits
// a moment under its arrows before it turns, so that a person with a cube
// in their hands can do it too.

import { nextFrame } from "./cube-view.js";

const QUARTER_MS = 1320; // one quarter turn at speed 1
const CUE_SHARE = 0.75; // how long the arrows stand before a move, as a share of a quarter turn
export const MIN_SPEED = 0.25;
export const MAX_SPEED = 24;
const same = (a, b) => a.every((colour, i) => colour === b[i]);

export class Player {
  constructor(view) {
    this.view = view;
    this.model = null;
    this.moves = [];
    this.facelets = null;
    this.base = null; // the cube after `position` whole solution moves
    this.partial = null;
    this.detour = [];
    this.position = 0;
    this.playing = false;
    this.speed = 1;
    this.cue = true;
    this.anim = null;
    this.onchange = () => {};
  }

  // A cube and the moves to play on it, at the start and paused.
  load(model, start, moves) {
    this.stop();
    this.model = model;
    this.start = start;
    this.moves = moves;
    this.base = this.facelets = start;
    this.partial = null;
    this.detour = [];
    this.position = 0;
    this.view.setCube(model.n, start);
    this.#showCue();
    this.onchange();
  }

  get length() {
    return this.moves.length;
  }

  get moving() {
    return !!this.anim;
  }

  get manualMoves() {
    return [...(this.partial?.made ?? []), ...this.detour];
  }

  get nextMove() {
    if (this.detour.length) return this.model.inverse(this.detour.at(-1));
    return this.partial?.remaining[0] ?? this.moves[this.position];
  }

  get canStepBack() {
    return this.position > 0 || !!this.partial || this.detour.length > 0;
  }

  setSpeed(speed) {
    if (!Number.isFinite(speed)) return;
    speed = Math.max(MIN_SPEED, Math.min(MAX_SPEED, speed));
    if (this.anim && speed !== this.speed) {
      // Keep the same progress through the cue or turn at the new rate.
      const now = performance.now();
      const scale = this.speed / speed;
      this.anim.start = now + (this.anim.start - now) * scale;
      this.anim.ms *= scale;
    }
    this.speed = speed;
  }

  setCue(on) {
    this.cue = on;
    this.#showCue();
  }

  // The next move, as the view draws it.
  #showCue() {
    const next = this.cue && !this.anim ? this.nextMove : undefined;
    if (next === undefined) {
      this.view.setCue(null);
      return;
    }
    const { face, from, to, quarters } = this.model.info(next);
    this.view.setCue({ face, from, to, quarters });
  }

  stop() {
    this.playing = false;
    this.anim = null;
    this.view.setTurn(null);
  }

  // Jumps to the cube after `k` moves, without animation.
  seek(k, { partial = [], detour = [] } = {}) {
    k = Math.max(0, Math.min(this.length, Math.round(k)));
    this.stop();
    const m = this.model;
    // Start from the nearer point on the solution.
    if (k < this.position - k) {
      this.base = this.start;
      this.position = 0;
    }
    while (this.position < k) this.base = m.apply(this.base, this.moves[this.position++]);
    while (this.position > k) this.base = m.apply(this.base, m.inverse(this.moves[--this.position]));
    this.facelets = this.base;
    this.partial = null;
    this.detour = [];
    for (const turns of [partial, detour]) {
      if (turns.length && this.position < this.length) this.#follow(turns);
    }
    this.view.setFacelets(this.facelets);
    this.#showCue();
    this.onchange();
  }

  turnByHand(moves) {
    this.stop();
    this.#follow(moves);
    this.view.setFacelets(this.facelets);
    this.#showCue();
    this.onchange();
  }

  #at(position, facelets) {
    this.position = position;
    this.base = this.facelets = facelets;
    this.partial = null;
    this.detour = [];
  }

  #follow(made) {
    const m = this.model;
    const next = this.moves[this.position];
    const after = m.applyAll(this.facelets, made);
    this.facelets = after;
    let goal = m.apply(this.base, next);
    if (same(after, goal)) {
      this.#at(this.position + 1, after);
      return;
    }
    if (same(after, this.base)) {
      this.#at(this.position, after);
      return;
    }
    const partial = m.progress(next, this.base, after);
    if (partial) {
      this.partial = partial.made.length ? partial : null;
      this.detour = [];
      return;
    }
    if (this.position > 0 && same(after, m.apply(this.base, m.inverse(this.moves[this.position - 1])))) {
      this.#at(this.position - 1, after);
      return;
    }
    // Moves can commute. If a detour reaches a later step, resume there.
    const end = Math.min(this.length, this.position + this.detour.length + made.length);
    for (let k = this.position + 1; k < end; k++) {
      goal = m.apply(goal, this.moves[k]);
      if (same(after, goal)) {
        this.#at(k + 1, after);
        return;
      }
    }
    this.detour = m.simplify([...this.detour, ...made]);
  }

  play() {
    if (this.playing || !this.length) return;
    if (this.position >= this.length) this.seek(0);
    this.playing = true;
    this.onchange();
    if (!this.anim) this.#turn(1, true);
  }

  pause() {
    this.playing = false;
    if (this.anim && performance.now() < this.anim.start) {
      // Still waiting under its arrows: the move does not start.
      this.anim = null;
      this.#showCue();
    }
    // A move already turning is finished first.
    this.onchange();
  }

  toggle() {
    if (this.playing) this.pause();
    else this.play();
  }

  // One move forwards (1) or backwards (-1), animated.
  step(direction) {
    this.playing = false;
    // Impatient stepping: finish the move under way at once.
    if (this.anim) this.#finish();
    this.#turn(direction, false);
    this.onchange();
  }

  #turn(direction, wait) {
    const m = this.model;
    const back = direction < 0 && !this.partial && this.detour.length === 0;
    let move = this.nextMove;
    if (back) move = this.position > 0 ? m.inverse(this.moves[this.position - 1]) : undefined;
    else if (direction < 0 && !this.detour.length) move = m.inverse(this.partial.made.at(-1));
    if (move === undefined) {
      this.playing = false;
      this.#showCue();
      this.onchange();
      return;
    }
    const { face, from, to, quarters } = m.info(move);
    const full = (quarters * Math.PI) / 2;
    const ms = (QUARTER_MS * (Math.abs(quarters) === 2 ? 1.4 : 1)) / this.speed;
    // In play, with the cue on, the arrows of this move stand for a moment first.
    const lead = wait && this.cue ? (QUARTER_MS * CUE_SHARE) / this.speed : 0;
    const anim = { back, move, start: performance.now() + lead, ms };
    this.anim = anim;
    // The arrows of a move stay on its layers while they turn, and give way
    // to those of the next move when it ends. A move taken back has none.
    if (this.cue && direction > 0) this.view.setCue({ face, from, to, quarters });
    else this.view.setCue(null);
    const frame = (now) => {
      if (this.anim !== anim) return;
      if (now < anim.start) {
        nextFrame(frame);
        return;
      }
      const t = Math.min(1, (now - anim.start) / anim.ms);
      const eased = t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
      this.view.setTurn({ face, from, to, angle: full * eased });
      if (t < 1) {
        nextFrame(frame);
        return;
      }
      this.#finish();
      if (this.playing) this.#turn(1, true);
      else this.onchange();
    };
    nextFrame(frame);
  }

  #finish() {
    const anim = this.anim;
    if (!anim) return;
    this.anim = null;
    if (anim.back) {
      this.#at(this.position - 1, this.model.apply(this.base, anim.move));
    } else if (!this.partial && !this.detour.length) {
      this.#at(this.position + 1, this.model.apply(this.base, anim.move));
    } else {
      this.#follow([anim.move]);
    }
    this.view.setTurn(null);
    this.view.setFacelets(this.facelets);
    this.#showCue();
    this.onchange();
  }
}
