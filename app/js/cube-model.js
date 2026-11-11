// A cube of any size as a picture of its stickers: the page's own model,
// used to show a cube and to play a solution on it.
//
// It is the model of the solver (src/cuben.rs) written again: facelets in
// URFDLB order, each face row by row, and move
// `(face + 6 * (layers - 1)) * 3 + power` turning the outer `layers` layers
// of `face`. A test holds the two together.
//
// No DOM here, so that the tests can load it under Node.

export const FACE_LETTERS = "URFDLB";

// Outward normal, column direction and row direction of each face, in a
// frame with x to the right, y up and z to the front.
export const FACES = [
  { normal: [0, 1, 0], right: [1, 0, 0], down: [0, 0, 1] },
  { normal: [1, 0, 0], right: [0, 0, -1], down: [0, -1, 0] },
  { normal: [0, 0, 1], right: [1, 0, 0], down: [0, -1, 0] },
  { normal: [0, -1, 0], right: [1, 0, 0], down: [0, 0, -1] },
  { normal: [-1, 0, 0], right: [0, 0, 1], down: [0, -1, 0] },
  { normal: [0, 0, -1], right: [-1, 0, 0], down: [0, -1, 0] },
];

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];

// A quarter turn clockwise, seen from outside along `axis`.
function turn(axis, v) {
  const c = cross(axis, v);
  const d = dot(axis, v);
  return [axis[0] * d - c[0], axis[1] * d - c[1], axis[2] * d - c[2]];
}

export class ScrambleError extends Error {}

// The face whose outward normal is along `axis` (0 = x, 1 = y, 2 = z) with that sign.
const FACE_OF = [
  [4, 1],
  [3, 0],
  [5, 2],
];

export class CubeModel {
  constructor(n) {
    if (!Number.isInteger(n) || n < 2) throw new Error("size must be 2 or more");
    this.n = n;
    this.count = 6 * n * n;
    this.moveCount = 18 * Math.floor(n / 2);
    this.perms = new Map();
    // Where every facelet is, once: position (doubled, so that it is integral) and face.
    this.pos = new Int16Array(this.count * 3);
    for (let i = 0; i < this.count; i++) {
      const { p } = this.place(i);
      this.pos.set(p, i * 3);
    }
  }

  solved() {
    const nn = this.n * this.n;
    return Uint8Array.from({ length: this.count }, (_, i) => Math.floor(i / nn));
  }

  // Position (doubled, so that it is integral) and face of facelet `i`.
  place(i) {
    const n = this.n;
    const face = Math.floor(i / (n * n));
    const r = Math.floor(i / n) % n;
    const c = i % n;
    const { normal, right, down } = FACES[face];
    const a = 2 * c - (n - 1);
    const b = 2 * r - (n - 1);
    return { face, p: [0, 1, 2].map((k) => normal[k] * n + right[k] * a + down[k] * b) };
  }

  // The facelet at doubled position (x, y, z); its face is the one the position sticks out of.
  indexAt(x, y, z) {
    const n = this.n;
    const axis = Math.abs(x) === n ? 0 : Math.abs(y) === n ? 1 : 2;
    const face = FACE_OF[axis][(axis === 0 ? x : axis === 1 ? y : z) > 0 ? 1 : 0];
    const { right, down } = FACES[face];
    const c = (x * right[0] + y * right[1] + z * right[2] + n - 1) / 2;
    const r = (x * down[0] + y * down[1] + z * down[2] + n - 1) / 2;
    return face * n * n + r * n + c;
  }

  // What a move is: its face, the layers it turns (`from` up to but not
  // including `to`, counted from the face: a move of the solver's starts at
  // 0 and turns `layers` of them) and the turn.
  info(m) {
    const power = m % 3;
    const quarters = [1, 2, -1][power];
    const n = this.n;
    if (m >= 18 * n) {
      const k = Math.floor((m - 18 * n) / 3);
      const [to, from, face] = [k % (n + 1), Math.floor(k / (n + 1)) % n, Math.floor(k / (n + 1) / n)];
      return { face, from, to, layers: to - from, power, quarters };
    }
    const layers = Math.floor(m / 18) + 1;
    return { face: Math.floor(m / 3) % 6, from: 0, to: layers, layers, power, quarters };
  }

  // Inner layers alone, which the solver has no move for but a hand makes:
  // layers `from` up to `to` of a face, numbered past all the solver's
  // moves. From 0 it is the solver's own move.
  slice(face, from, to, power) {
    if (from === 0) return this.move(face, to, power);
    const n = this.n;
    return 18 * n + ((face * n + from) * (n + 1) + to) * 3 + power;
  }

  // The same sequence in the solver's moves only: inner layers alone are
  // the layers down to them turned, and those above turned back.
  plain(moves) {
    const out = [];
    for (const m of moves) {
      const { face, from, to, power } = this.info(m);
      out.push(this.move(face, to, power));
      if (from > 0) out.push(this.move(face, from, 2 - power));
    }
    return out;
  }

  inverse(m) {
    return m - (m % 3) + (2 - (m % 3));
  }

  // `perm[j]` is the facelet whose sticker move `m` brings to facelet `j`.
  perm(m) {
    const { face, from, to, power } = this.info(m);
    return this.slab(face, from, to, power);
  }

  // The same for any run of layers of a face, counted from the face (0 is
  // the face itself): layers `from` up to but not including `to`. A move
  // is the run that starts at 0; a single inner layer is what a hand turns.
  slab(face, from, to, power) {
    const key = ((face * 512 + from) * 512 + to) * 4 + power;
    let perm = this.perms.get(key);
    if (perm) return perm;
    const n = this.n;
    const [ax, ay, az] = FACES[face].normal;
    const pos = this.pos;
    const quarter = new Uint32Array(this.count);
    for (let i = 0; i < this.count; i++) quarter[i] = i;
    for (let i = 0; i < this.count; i++) {
      const [x, y, z] = [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]];
      // How deep the sticker's little cube is under the face.
      const along = x * ax + y * ay + z * az;
      const layer = (n - 1 - Math.max(-(n - 1), Math.min(n - 1, along))) / 2;
      if (layer < from || layer >= to) continue;
      // A quarter turn clockwise seen from outside: v -> a (a . v) - a x v.
      const tx = ax * along - (ay * z - az * y);
      const ty = ay * along - (az * x - ax * z);
      const tz = az * along - (ax * y - ay * x);
      quarter[this.indexAt(tx, ty, tz)] = i;
    }
    perm = quarter;
    for (let k = 0; k < power; k++) perm = perm.map((j) => quarter[j]);
    // A large cube has hundreds of moves of tens of thousands of entries: keep the recent ones.
    if (this.perms.size >= 96) this.perms.delete(this.perms.keys().next().value);
    this.perms.set(key, perm);
    return perm;
  }

  // The stickers after a permutation from `perm` or `slab`, as a new array.
  permute(facelets, perm) {
    const out = new Uint8Array(this.count);
    for (let j = 0; j < this.count; j++) out[j] = facelets[perm[j]];
    return out;
  }

  // The stickers after move `m`, as a new array.
  apply(facelets, m) {
    return this.permute(facelets, this.perm(m));
  }

  // One layer turned by hand, `layer` deep under `face` (0 is the face
  // itself), by `quarters` (1, 2 or -1, clockwise seen from the face), as
  // the moves of the solver's notation that do the same. A layer past the
  // middle is turned from the other side; the middle layer of an odd cube
  // is both halves turned the other way and the whole cube turned along,
  // so `rotate` says to turn the picture by that much to keep it in place.
  handTurn(face, layer, quarters) {
    const n = this.n;
    const half = Math.floor(n / 2);
    const power = quarters === 1 ? 0 : quarters === -1 ? 2 : 1;
    const back = 2 - power;
    const opposite = (face + 3) % 6;
    if (n % 2 === 1 && layer === half) {
      return { moves: [this.move(face, half, back), this.move(opposite, half, power)], rotate: true };
    }
    if (layer >= half) return { ...this.handTurn(opposite, n - 1 - layer, -quarters || 2), rotate: false };
    const moves = [this.move(face, layer + 1, power)];
    if (layer > 0) moves.push(this.move(face, layer, back));
    return { moves, rotate: false };
  }

  // One layer turned by hand as one move, named from the face it is nearer
  // to: the solver's move for an outer layer, a `slice` for an inner one.
  // Nothing for the middle layer of an odd cube, which is not a move of a
  // cube whose middles stay where they are.
  handSlice(face, layer, quarters) {
    const n = this.n;
    if (n % 2 === 1 && layer === (n - 1) / 2) return null;
    if (layer >= n / 2) return this.handSlice((face + 3) % 6, n - 1 - layer, Math.abs(quarters) === 2 ? 2 : -quarters);
    return this.slice(face, layer, layer + 1, quarters === 1 ? 0 : quarters === -1 ? 2 : 1);
  }

  // What is left of move `next` when `made`, a turn of some of its layers
  // but not all, has been made: the other layers still to be turned as
  // `next` says, and those turned, if the turn was not all of theirs, the
  // rest of the way. Nothing (null) when `made` is not such a turn.
  remainder(next, made) {
    const [a, b] = [this.info(next), this.info(made)];
    if (a.face !== b.face || b.from < a.from || b.to > a.to || b.to - b.from === a.to - a.from) return null;
    const still = (((a.quarters - b.quarters) % 4) + 4) % 4;
    const out = [];
    if (b.from > a.from) out.push(this.slice(a.face, a.from, b.from, a.power));
    if (b.to < a.to) out.push(this.slice(a.face, b.to, a.to, a.power));
    if (still) out.push(this.slice(a.face, b.from, b.to, still - 1));
    return out;
  }

  // The same stickers with the middle of each face of an odd cube set to
  // the face's own colour: those six never move, so they are never asked
  // for. The array itself when nothing needs setting.
  withCentres(facelets) {
    const nn = this.n * this.n;
    if (this.n % 2 === 0) return facelets;
    let out = facelets;
    for (let face = 0; face < 6; face++) {
      const i = face * nn + (nn - 1) / 2;
      if (out[i] === face) continue;
      if (out === facelets) out = Uint8Array.from(facelets);
      out[i] = face;
    }
    return out;
  }

  // A sequence with moves of the same layers of the same face that follow
  // each other made one, or none.
  simplify(moves) {
    const out = [];
    for (const m of moves) {
      const last = out[out.length - 1];
      if (last !== undefined && Math.floor(last / 3) === Math.floor(m / 3)) {
        const turns = ((last % 3) + 1 + (m % 3) + 1) % 4;
        out.pop();
        if (turns) out.push(m - (m % 3) + turns - 1);
      } else {
        out.push(m);
      }
    }
    return out;
  }

  // What is left of a solution after moves it did not ask for: those moves
  // undone, then the rest, with moves of the same layers that meet made one
  // or none. `kept` says how many moves at the end are the rest's own, as
  // they were; when all of the result is that, the moves made were the ones
  // the solution asked for next.
  rejoin(rest, made) {
    const undo = made.map((m) => this.inverse(m)).reverse();
    const moves = this.simplify([...undo, ...rest]);
    let kept = 0;
    while (kept < rest.length && kept < moves.length && moves[moves.length - 1 - kept] === rest[rest.length - 1 - kept]) kept++;
    return { moves, kept };
  }

  applyAll(facelets, moves) {
    let f = facelets;
    for (const m of moves) f = this.apply(f, m);
    return f;
  }

  isSolved(facelets) {
    const nn = this.n * this.n;
    for (let i = 0; i < this.count; i++) if (facelets[i] !== facelets[i - (i % nn)]) return false;
    return true;
  }

  moveName(m) {
    const { face, from, to, layers, power } = this.info(m);
    const letter = FACE_LETTERS[face];
    // Inner layers alone are named by their number: 2F the second layer, 2-3Fw the second and third.
    const inner = to - from === 1 ? `${to}${letter}` : `${from + 1}-${to}${letter}w`;
    const body = from > 0 ? inner : layers === 1 ? letter : layers === 2 ? `${letter}w` : `${layers}${letter}w`;
    return body + ["", "2", "'"][power];
  }

  names(moves) {
    return moves.map((m) => this.moveName(m)).join(" ");
  }

  move(face, layers, power) {
    return (face + 6 * (layers - 1)) * 3 + power;
  }

  toText(facelets) {
    let s = "";
    for (const c of facelets) s += FACE_LETTERS[c];
    return s;
  }

  fromText(text) {
    if (text.length !== this.count) throw new Error("wrong number of facelets");
    return Uint8Array.from(text, (ch) => {
      const c = FACE_LETTERS.indexOf(ch);
      if (c < 0) throw new Error("bad facelet");
      return c;
    });
  }

  // Reads moves as the solver writes them (`U`, `Rw2`, `3Fw'`); nothing else.
  parse(text) {
    return text
      .split(/\s+/)
      .filter(Boolean)
      .map((tok) => {
        const m = /^(\d*)([URFDLB])(w?)(2|')?$/.exec(tok);
        const layers = m ? (m[1] ? Number(m[1]) : m[3] ? 2 : 1) : 0;
        if (!m || layers < 1 || layers > this.n / 2 || (m[1] && !m[3])) throw new ScrambleError(`'${tok}' is not a move of this cube`);
        return this.move(FACE_LETTERS.indexOf(m[2]), layers, m[4] === "2" ? 1 : m[4] === "'" ? 2 : 0);
      });
  }

  // Reads what people type and paste: besides the solver's notation, lower
  // case for wide turns (`r`), whole-cube turns (`x y z`), the middle
  // slices of odd cubes (`M E S`), wide turns of more than half the cube,
  // typographic apostrophes, brackets and commas. The result is the same
  // cube in the solver's notation: every whole-cube turn is folded into the
  // names of the moves after it, so the cube is never turned as a whole.
  normalise(text) {
    const n = this.n;
    const half = Math.floor(n / 2);
    // `at[f]` is the face of the cube as it started that is now at place `f`.
    let at = [0, 1, 2, 3, 4, 5];
    // The four faces around each face, in the order a clockwise turn carries stickers.
    const ring = [
      [2, 4, 5, 1],
      [2, 0, 5, 3],
      [0, 1, 3, 4],
      [2, 1, 5, 4],
      [2, 3, 5, 0],
      [0, 4, 3, 1],
    ];
    const rotate = (face, power) => {
      for (let k = 0; k <= power; k++) {
        const next = at.slice();
        for (let i = 0; i < 4; i++) next[ring[face][(i + 1) % 4]] = at[ring[face][i]];
        at = next;
      }
    };
    const moves = [];
    const push = (face, layers, power) => moves.push(this.move(at[face], layers, power));
    const tokens = text
      .replace(/[’′`´]/g, "'")
      .replace(/[(),;\[\]]/g, " ")
      .split(/\s+/)
      .filter(Boolean);
    for (const tok of tokens) {
      const m = /^(\d*)([URFDLBurfdlbxyzMES])(w?)(2'|2|'|)$/.exec(tok);
      if (!m) throw new ScrambleError(`'${tok.slice(0, 12)}' is not a move`);
      const power = m[4] === "" ? 0 : m[4] === "'" ? 2 : 1;
      const back = 2 - power;
      const letter = m[2];
      if ("xyz".includes(letter)) {
        if (m[1] || m[3]) throw new ScrambleError(`'${tok}' is not a move`);
        rotate([1, 0, 2]["xyz".indexOf(letter)], power);
        continue;
      }
      if ("MES".includes(letter)) {
        if (m[1] || m[3]) throw new ScrambleError(`'${tok}' is not a move`);
        if (n % 2 === 0) throw new ScrambleError(`${letter} is the middle slice, and a ${n}x${n}x${n} has none`);
        // M turns with L, E with D, S with F: the whole cube turned that
        // way, and the two outer blocks turned back.
        const face = [4, 3, 2]["MES".indexOf(letter)];
        const opposite = (face + 3) % 6;
        push(face, half, back);
        push(opposite, half, power);
        rotate(face, power);
        continue;
      }
      const face = FACE_LETTERS.indexOf(letter.toUpperCase());
      const wide = m[3] === "w" || letter !== letter.toUpperCase();
      if (m[1] && !wide) throw new ScrambleError(`'${tok}': a number goes with a wide turn, as in 3Rw`);
      const layers = m[1] ? Number(m[1]) : wide ? 2 : 1;
      if (layers < 1 || layers > n) throw new ScrambleError(`'${tok}' turns more layers than a ${n}x${n}x${n} has`);
      if (layers <= half) {
        push(face, layers, power);
      } else {
        // More than half the cube: the same as turning the rest the same
        // way round and the whole cube with it.
        if (layers < n) push((face + 3) % 6, n - layers, power);
        rotate(face, power);
      }
    }
    return { moves, text: this.names(moves) };
  }

  // A random-move scramble: no face twice in a row, nor a face, its
  // opposite and the face again.
  randomScramble(length, random = Math.random) {
    const half = Math.floor(this.n / 2);
    const moves = [];
    let last = -1;
    let before = -1;
    while (moves.length < length) {
      const face = Math.floor(random() * 6);
      if (face === last || (face === before && face % 3 === last % 3)) continue;
      moves.push(this.move(face, 1 + Math.floor(random() * half), Math.floor(random() * 3)));
      before = last;
      last = face;
    }
    return moves;
  }
}

// How many random moves a scramble has: the lengths competitions use for
// the sizes they scramble by random moves (40 for the 4x4x4, then 20 more
// per size), carried on at the same rate above 7x7x7. The 2x2x2 and 3x3x3
// figures are for when no solver is at hand to scramble them from a random state.
export function scrambleLength(n) {
  return n === 2 ? 11 : n === 3 ? 25 : 20 * (n - 2);
}
