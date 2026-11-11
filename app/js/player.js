// Plays a sequence of moves on a CubeView: play, pause, step either way,
// jump to any point, at a chosen speed. With the cue on, the move that
// comes next is drawn on the cube as arrows, which turn with its layers
// and then give way to those of the move after it; in play each move waits
// a moment under its arrows before it turns, so that a person with a cube
// in their hands can do it too.

import { nextFrame } from "./cube-view.js";

const QUARTER_MS = 1320; // one quarter turn at speed 1
const CUE_SHARE = 0.75; // how long the arrows stand before a move, as a share of a quarter turn

export class Player {
  constructor(view) {
    this.view = view;
    this.model = null;
    this.moves = [];
    this.facelets = null; // the cube after `position` moves
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
    this.facelets = start;
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

  setSpeed(speed) {
    this.speed = speed;
  }

  setCue(on) {
    this.cue = on;
    this.#showCue();
  }

  // The next move, as the view draws it.
  #showCue() {
    const next = this.cue && !this.anim && this.position < this.length ? this.moves[this.position] : undefined;
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
  seek(k) {
    k = Math.max(0, Math.min(this.length, Math.round(k)));
    this.stop();
    const m = this.model;
    // From wherever is nearer: the start, or where the cube is now.
    if (k < this.position - k) {
      this.facelets = this.start;
      this.position = 0;
    }
    while (this.position < k) this.facelets = m.apply(this.facelets, this.moves[this.position++]);
    while (this.position > k) this.facelets = m.apply(this.facelets, m.inverse(this.moves[--this.position]));
    this.view.setFacelets(this.facelets);
    this.#showCue();
    this.onchange();
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
    if ((direction > 0 && this.position >= this.length) || (direction < 0 && this.position <= 0)) {
      this.playing = false;
      this.#showCue();
      this.onchange();
      return;
    }
    const move = this.moves[direction > 0 ? this.position : this.position - 1];
    const { face, from, to, quarters } = m.info(move);
    // Backwards, the cube is first put as it was before the move, and the
    // layers start fully turned and come back.
    const before = direction > 0 ? this.facelets : m.apply(this.facelets, m.inverse(move));
    const full = (quarters * Math.PI) / 2;
    const ms = (QUARTER_MS * (Math.abs(quarters) === 2 ? 1.4 : 1)) / this.speed;
    // In play, with the cue on, the arrows of this move stand for a moment first.
    const lead = wait && this.cue ? (QUARTER_MS * CUE_SHARE) / this.speed : 0;
    const anim = { direction, move, before, start: performance.now() + lead, ms };
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
      this.view.setFacelets(before);
      this.view.setTurn({ face, from, to, angle: full * (direction > 0 ? eased : 1 - eased) });
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
    if (anim.direction > 0) {
      this.facelets = this.model.apply(anim.before, anim.move);
      this.position++;
    } else {
      this.facelets = anim.before;
      this.position--;
    }
    this.view.setTurn(null);
    this.view.setFacelets(this.facelets);
    this.#showCue();
    this.onchange();
  }
}
