import { FACE_LETTERS, FACES } from "./cube-model.js";
import { FACE_NAMES } from "./paint.js";

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

export class ViewCube {
  constructor(root, look) {
    const cube = document.createElement("div");
    cube.className = "view-cube-body";
    root.append(cube);

    this.faces = FACES.map(({ normal, right, down }, face) => {
      const letter = FACE_LETTERS[face];
      const side = document.createElement("div");
      side.className = "view-cube-face";
      side.style.setProperty("--face-colour", `var(--${letter.toLowerCase()})`);
      for (const row of [-1, 0, 1]) {
        for (const col of [-1, 0, 1]) {
          const direction = normal.map((v, k) => v + col * right[k] + row * down[k]);
          const button = document.createElement("button");
          button.type = "button";
          const centre = row === 0 && col === 0;
          if (centre) {
            button.className = "view-cube-centre";
            button.textContent = letter;
            button.title = `${FACE_NAMES[face]} view (${letter})`;
          } else {
            const letters = FACES.map((f, k) => (dot(f.normal, direction) > 0 ? FACE_LETTERS[k] : "")).join("");
            button.title = `${letters} ${row !== 0 && col !== 0 ? "corner" : "edge"} view`;
          }
          button.setAttribute("aria-label", button.title);
          button.addEventListener("click", () => look(direction));
          side.append(button);
        }
      }
      cube.append(side);
      return side;
    });
  }

  update(rot) {
    const screenX = rot.slice(0, 3);
    const screenY = rot.slice(3, 6).map((v) => -v);
    const toward = rot.slice(6);
    // Project each face onto the screen. The translation places its centre and
    // the matrix maps its columns and rows to the projected right and down axes.
    for (const [i, side] of this.faces.entries()) {
      const { normal, right, down } = FACES[i];
      side.hidden = dot(toward, normal) < 0.001;
      if (side.hidden) continue;
      const half = "var(--view-cube-half)";
      const x = `calc(${dot(screenX, normal)} * ${half})`;
      const y = `calc(${dot(screenY, normal)} * ${half})`;
      side.style.transform = `translate(${x},${y}) matrix(${dot(screenX, right)},${dot(screenY, right)},${dot(screenX, down)},${dot(screenY, down)},0,0)`;
    }
  }
}
