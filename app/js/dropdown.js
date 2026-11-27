// A dropdown of the page's own: a button that shows what is chosen, on as
// many lines as it needs, and a list of its own that opens under it (over
// it, where there is no room under). Not the browser's <select>, whose
// list is the system's and cuts a long name.
//
// To a keyboard and a screen reader it is a combobox that only selects
// (the ARIA pattern): the focus stays on the button, the arrows, Home, End
// and letters move through the list, Enter or Space choose, Escape closes.
// A choice that cannot be made is in the list all the same, with why.

/**
 * Where the option is that a key moves to, from `index` among `count`
 * (-1: none yet). Null for a key that does not move.
 */
export function stepIndex(key, index, count) {
  if (!count) return null;
  switch (key) {
    case "ArrowDown": return Math.min(count - 1, index + 1);
    case "ArrowUp": return Math.max(0, index < 0 ? count - 1 : index - 1);
    case "Home": return 0;
    case "End": return count - 1;
    case "PageDown": return Math.min(count - 1, Math.max(0, index) + 10);
    case "PageUp": return Math.max(0, index - 10);
    default: return null;
  }
}

/**
 * The option that what was typed means: the next after `from` whose text
 * begins with `typed`, whatever the case; one letter typed again and again
 * goes through those that begin with it. -1 if there is none.
 */
export function typeAhead(texts, typed, from = -1) {
  const want = typed.toLowerCase();
  if (!want || !texts.length) return -1;
  const same = [...want].every((c) => c === want[0]);
  const find = (prefix, start) => {
    for (let k = 0; k < texts.length; k++) {
      const i = (start + k + texts.length) % texts.length;
      if (String(texts[i]).toLowerCase().startsWith(prefix)) return i;
    }
    return -1;
  };
  // What is typed further stays on the option it is on while it still fits.
  const whole = find(want, want.length > 1 ? Math.max(0, from) : from + 1);
  if (whole >= 0 || !same) return whole;
  return find(want[0], from + 1);
}

/**
 * Where a list `natural` pixels tall goes against what opened it (`anchor`:
 * top and bottom, in the window's own pixels) in a window `height` tall:
 * under it if it fits or there is more room there, else over it; never
 * taller than the room, so that it stays in the window and scrolls.
 * `{up, maxHeight, top}` or `{up, maxHeight, bottom}` (from the window's bottom).
 */
export function placement(anchor, natural, height, { gap = 4, margin = 8 } = {}) {
  const below = height - anchor.bottom - gap - margin;
  const above = anchor.top - gap - margin;
  const up = natural > below && above > below;
  const maxHeight = Math.max(0, Math.floor(up ? above : below));
  return up ? { up, maxHeight, bottom: height - anchor.top + gap } : { up, maxHeight, top: anchor.bottom + gap };
}

/**
 * Lays `popup` (position: fixed) against `anchor`, as wide as `across`
 * (the anchor itself if not said), `gap` pixels from it, and inside the window.
 */
export function place(popup, anchor, across = anchor, gap = 4) {
  const win = popup.ownerDocument.defaultView;
  const root = popup.ownerDocument.documentElement;
  const [vw, vh] = [root.clientWidth || win.innerWidth, root.clientHeight || win.innerHeight];
  const [a, w] = [anchor.getBoundingClientRect(), across.getBoundingClientRect()];
  const width = Math.min(w.width, vw - 16);
  const s = popup.style;
  s.maxHeight = "none";
  s.width = `${width}px`;
  s.left = `${Math.max(8, Math.min(w.left, vw - width - 8))}px`;
  const at = placement(a, popup.offsetHeight, vh, { gap });
  s.maxHeight = `${at.maxHeight}px`;
  s.top = at.up ? "auto" : `${at.top}px`;
  s.bottom = at.up ? `${at.bottom}px` : "auto";
  popup.classList.toggle("up", at.up);
  return at;
}

let made = 0;

/**
 * A dropdown in `root` (an empty element). `id` is the button's, so that
 * the page finds it as it would a select; `labelledby` the id of the words
 * that say what is chosen here.
 *
 * Its options are `{value, text, note?, disabled?, ...}`: `note` is said
 * beside the text (why an option cannot be chosen: "busy", "not a robot"),
 * and whatever else an option carries is the page's own. `change` is
 * dispatched on `root` when the person chooses another option, not when
 * the page sets one.
 */
export class Dropdown {
  constructor(root, { id, labelledby, options = [], value } = {}) {
    const doc = root.ownerDocument;
    this.root = root;
    this.id = id ?? `dropdown-${++made}`;
    this.options = [];
    this.index = -1; // the option chosen
    this.active = -1; // the option the keyboard or the pointer is on, while open
    this.open = false;
    this.typed = "";
    this.typedAt = 0;
    root.classList.add("dropdown");

    const button = (this.button = doc.createElement("button"));
    button.type = "button";
    button.id = this.id;
    button.className = "dropdown-button";
    button.setAttribute("role", "combobox");
    button.setAttribute("aria-haspopup", "listbox");
    button.setAttribute("aria-expanded", "false");
    button.setAttribute("aria-controls", `${this.id}-list`);
    if (labelledby) button.setAttribute("aria-labelledby", `${labelledby} ${this.id}-value`);
    this.shown = doc.createElement("span");
    this.shown.id = `${this.id}-value`;
    this.shown.className = "dropdown-value";
    button.append(this.shown);

    const list = (this.list = doc.createElement("ul"));
    list.id = `${this.id}-list`;
    list.className = "dropdown-list";
    list.setAttribute("role", "listbox");
    list.tabIndex = -1;
    if (labelledby) list.setAttribute("aria-labelledby", labelledby);
    list.hidden = true;
    root.replaceChildren(button, list);

    button.addEventListener("click", () => this.toggle());
    button.addEventListener("keydown", (e) => this.#key(e));
    button.addEventListener("blur", () => this.close());
    // The focus stays on the button while the list is pressed and scrolled.
    list.addEventListener("mousedown", (e) => e.preventDefault());
    list.addEventListener("click", (e) => {
      const li = e.target.closest?.("[role=option]");
      if (li) this.#choose(Number(li.dataset.index));
    });
    list.addEventListener("pointermove", (e) => {
      const li = e.target.closest?.("[role=option]");
      if (li) this.#setActive(Number(li.dataset.index), false);
    });
    // Anything pressed elsewhere closes it; what the window does under it moves it along.
    doc.addEventListener("pointerdown", (e) => {
      if (this.open && !root.contains(e.target)) this.close();
    }, true);
    const again = (e) => {
      if (this.open && !(e?.target && e.target !== doc && list.contains(e.target))) place(list, button);
    };
    doc.defaultView.addEventListener("resize", again);
    doc.addEventListener("scroll", again, true);

    this.set(options, value);
  }

  /** The options anew; `value` (what was chosen, if not said) stays chosen if it is among them, else the first. */
  set(options, value = this.value) {
    const activeValue = this.options[this.active]?.value;
    this.options = options.map((o) => ({ ...o, value: String(o.value ?? ""), text: String(o.text ?? "") }));
    const doc = this.root.ownerDocument;
    this.list.replaceChildren(
      ...this.options.map((option, k) => {
        const li = doc.createElement("li");
        li.id = `${this.id}-option-${k}`;
        li.dataset.index = String(k);
        li.setAttribute("role", "option");
        if (option.disabled) li.setAttribute("aria-disabled", "true");
        const text = doc.createElement("span");
        text.className = "dropdown-text";
        text.textContent = option.text;
        li.append(text);
        if (option.note) {
          const note = doc.createElement("span");
          note.className = "dropdown-note";
          note.textContent = option.note;
          li.append(note);
        }
        return li;
      }),
    );
    const at = this.options.findIndex((o) => o.value === String(value ?? ""));
    this.#show(at >= 0 ? at : this.options.length ? 0 : -1);
    if (this.open) {
      const active = this.options.findIndex((o) => o.value === activeValue);
      this.#setActive(active >= 0 ? active : this.index, false);
      place(this.list, this.button);
    }
  }

  /** The option chosen (with all the page gave it), or undefined. */
  get selected() {
    return this.options[this.index];
  }

  get value() {
    return this.selected?.value ?? "";
  }

  /** Chooses the option of that value, if there is one; no `change` is told. */
  set value(value) {
    const at = this.options.findIndex((o) => o.value === String(value ?? ""));
    if (at >= 0) this.#show(at);
  }

  get disabled() {
    return this.button.disabled;
  }

  /** A dropdown that is not to be changed now: it does not open. */
  set disabled(off) {
    const doc = this.root.ownerDocument;
    if (off) this.close();
    // A button that is switched off lets go of the keyboard: it has it back
    // when it is on again, if the keyboard went nowhere else meanwhile.
    if (off && !this.button.disabled) this.hadFocus = doc.activeElement === this.button;
    this.button.disabled = !!off;
    this.root.classList.toggle("disabled", !!off);
    if (!off && this.hadFocus) {
      this.hadFocus = false;
      if (!doc.activeElement || doc.activeElement === doc.body) this.button.focus();
    }
  }

  /** Why it does not open, where the pointer is. */
  set title(text) {
    this.root.title = text;
  }

  on(type, listener) {
    this.root.addEventListener(type, listener);
  }

  #show(index) {
    this.index = index;
    this.shown.textContent = this.selected?.text ?? "";
    [...this.list.children].forEach((li, k) => li.setAttribute("aria-selected", String(k === index)));
  }

  #setActive(index, scroll = true) {
    this.active = index;
    [...this.list.children].forEach((li, k) => li.classList.toggle("active", k === index));
    const li = this.list.children[index];
    if (li) this.button.setAttribute("aria-activedescendant", li.id);
    else this.button.removeAttribute("aria-activedescendant");
    if (li && scroll) li.scrollIntoView?.({ block: "nearest" });
  }

  toggle() {
    if (this.open) this.close();
    else this.show();
  }

  show() {
    if (this.open || this.disabled || !this.options.length) return;
    this.open = true;
    this.list.hidden = false;
    this.root.classList.add("open");
    this.button.setAttribute("aria-expanded", "true");
    place(this.list, this.button);
    this.#setActive(this.index);
  }

  close() {
    if (!this.open) return;
    this.open = false;
    this.list.hidden = true;
    this.root.classList.remove("open");
    this.button.setAttribute("aria-expanded", "false");
    this.#setActive(-1);
  }

  #choose(index) {
    const option = this.options[index];
    if (!option || option.disabled) return;
    const changed = index !== this.index;
    this.#show(index);
    this.close();
    if (changed) this.root.dispatchEvent(new Event("change", { bubbles: true }));
  }

  #key(e) {
    if (e.ctrlKey || e.metaKey) return;
    const done = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    if (e.key === "Escape") {
      if (!this.open) return;
      this.close();
      return done();
    }
    if (e.key === "Tab") return this.close();
    if (e.key === "Enter" || e.key === " ") {
      if (e.key === " " && this.typed && Date.now() - this.typedAt < 600) return this.#type(e.key, done);
      if (this.open) this.#choose(this.active);
      else this.show();
      return done();
    }
    if (e.altKey && e.key === "ArrowUp" && this.open) {
      this.#choose(this.active);
      return done();
    }
    const to = stepIndex(e.key, this.open ? this.active : this.index, this.options.length);
    if (to !== null) {
      const was = this.open;
      this.show();
      // The key that opens the list is on what is chosen; Home and End go where they say.
      if (was || e.key === "Home" || e.key === "End") this.#setActive(to);
      return done();
    }
    if (e.key.length === 1 && !e.altKey) this.#type(e.key, done);
  }

  #type(key, done) {
    const now = Date.now();
    this.typed = now - this.typedAt < 600 ? this.typed + key : key;
    this.typedAt = now;
    const at = typeAhead(this.options.map((o) => o.text), this.typed, this.open ? this.active : this.index);
    if (at >= 0) {
      this.show();
      this.#setActive(at);
    }
    done();
  }
}
