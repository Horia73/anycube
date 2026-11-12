// What painting a cube needs beside the cube itself: the six colours to
// pick from, with how many stickers of each are placed, and the cube
// unfolded, small, to choose which face to look at. The painting is done
// on the cube in three dimensions (cube-view.js); this is also where a
// scan is corrected.

import { BLANK } from "./cube-view.js";

export const FACE_NAMES = ["Top", "Right", "Front", "Bottom", "Left", "Back"];

// Where each face goes in the unfolded cube: column and row of a 4 by 3 grid.
const UNFOLDED = [
  [2, 1],
  [3, 2],
  [2, 2],
  [2, 3],
  [1, 2],
  [4, 2],
];

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

export class PaintEditor {
  /**
   * @param {HTMLElement} root an empty element to build in
   * @param {{ onchange: (facelets: Uint8Array) => void, onface: (face: number) => void }} hooks
   */
  constructor(root, { onchange, onface }) {
    this.root = root;
    this.onchange = onchange;
    this.onface = onface;
    this.n = 3;
    this.facelets = new Uint8Array(54);
    this.palette = [];
    this.colour = 0;
    this.face = 2;
    this.swatches = el("div", { class: "swatches", role: "radiogroup", "aria-label": "Colour to paint with" });
    this.net = el("div", { class: "net", role: "group", "aria-label": "The unfolded cube: choose a face to look at" });
    root.append(this.swatches, this.net);
  }

  /** Shows a cube. */
  set(n, facelets, palette) {
    this.n = n;
    this.facelets = facelets;
    this.palette = palette;
    const nn = n * n;
    this.root.style.setProperty("--n", n);
    this.swatches.replaceChildren(
      ...palette.map((colour, k) => {
        const b = el("button", { type: "button", class: "swatch", role: "radio", "aria-checked": String(k === this.colour), "data-k": k }, [
          el("span", { class: "chip" }),
          el("span", { class: "count" }),
        ]);
        b.firstChild.style.background = colour;
        b.setAttribute("aria-label", `${FACE_NAMES[k]} face colour`);
        b.addEventListener("click", () => {
          this.colour = k;
          this.#counts();
        });
        return b;
      }),
    );
    this.net.replaceChildren(
      ...UNFOLDED.map(([col, row], face) => {
        const b = el("button", { type: "button", class: "net-face", "data-face": face, "aria-label": `Look at the ${FACE_NAMES[face].toLowerCase()} face`, title: FACE_NAMES[face] });
        b.style.gridColumn = col;
        b.style.gridRow = row;
        // A large cube is drawn as its faces' colours only: 49 cells are plenty to look at.
        for (let k = 0; k < Math.min(nn, 49); k++) b.append(el("i"));
        b.addEventListener("click", () => this.select(face));
        return b;
      }),
    );
    this.net.style.setProperty("--cells", Math.min(n, 7));
    this.#colours();
    this.#counts();
  }

  /** Marks a face as the one being looked at. */
  select(face, tell = true) {
    this.face = face;
    for (const b of this.net.children) b.classList.toggle("current", Number(b.dataset.face) === face);
    if (tell) this.onface(face);
  }

  #locked(i) {
    const nn = this.n * this.n;
    return this.n % 2 === 1 && i % nn === (nn - 1) / 2;
  }

  /** Paints one sticker with the colour picked; the middle of an odd cube's face stays. */
  paint(i) {
    if (this.#locked(i)) return;
    const colour = this.colour;
    if (this.facelets[i] === colour) return;
    this.facelets = Uint8Array.from(this.facelets);
    this.facelets[i] = colour;
    this.#colours();
    this.#counts();
    this.onchange(this.facelets);
  }

  // The unfolded cube. Up to 7 by 7 every sticker is shown; above that, a 7 by 7 sample of each face.
  #colours() {
    const { n, facelets, palette } = this;
    const nn = n * n;
    const shown = Math.min(n, 7);
    for (const face of this.net.children) {
      const base = Number(face.dataset.face) * nn;
      [...face.children].forEach((cell, k) => {
        const [r, c] = [Math.floor(k / shown), k % shown];
        const i = base + Math.floor((r * n) / shown) * n + Math.floor((c * n) / shown);
        const colour = facelets[i];
        cell.style.background = colour === BLANK ? "" : palette[colour];
      });
      face.classList.toggle("current", Number(face.dataset.face) === this.face);
    }
  }

  // How many stickers of each colour are placed, against how many there must be.
  #counts() {
    const nn = this.n * this.n;
    const count = new Array(6).fill(0);
    for (const c of this.facelets) if (c !== BLANK) count[c]++;
    [...this.swatches.children].forEach((b, k) => {
      b.setAttribute("aria-checked", String(k === this.colour));
      b.lastChild.textContent = `${count[k]}/${nn}`;
      b.classList.toggle("over", count[k] > nn);
      b.classList.toggle("full", count[k] === nn);
    });
  }
}
