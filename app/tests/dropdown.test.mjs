// The dropdown's reckoning, away from any page: where a key goes, what
// typing means, and where the list is laid. node --test app/tests/

import assert from "node:assert/strict";
import { test } from "node:test";
import { placement, stepIndex, typeAhead } from "../js/dropdown.js";

test("the arrows, Home and End move through the list and stop at its ends", () => {
  assert.equal(stepIndex("ArrowDown", 0, 3), 1);
  assert.equal(stepIndex("ArrowDown", 2, 3), 2);
  assert.equal(stepIndex("ArrowUp", 1, 3), 0);
  assert.equal(stepIndex("ArrowUp", 0, 3), 0);
  // From nowhere: down to the first, up to the last.
  assert.equal(stepIndex("ArrowDown", -1, 3), 0);
  assert.equal(stepIndex("ArrowUp", -1, 3), 2);
  assert.equal(stepIndex("Home", 2, 3), 0);
  assert.equal(stepIndex("End", 0, 3), 2);
  assert.equal(stepIndex("PageDown", 1, 30), 11);
  assert.equal(stepIndex("PageUp", 4, 30), 0);
  // Any other key moves nothing, and an empty list has nowhere to go.
  assert.equal(stepIndex("a", 0, 3), null);
  assert.equal(stepIndex("ArrowDown", 0, 0), null);
});

test("typing goes to the next option that begins so, whatever the case", () => {
  const names = ["MultiCuber · cu.usbmodem21301", "cu.Bluetooth-Incoming-Port", "Simulated MultiCuber", "None / disconnect"];
  assert.equal(typeAhead(names, "s", 0), 2);
  assert.equal(typeAhead(names, "n", 0), 3);
  assert.equal(typeAhead(names, "N", -1), 3);
  // The same letter again: the next that begins with it, round and round.
  const cams = ["FaceTime HD Camera", "iPhone Camera", "iPad Camera", "OBS Virtual Camera"];
  assert.equal(typeAhead(cams, "i", -1), 1);
  assert.equal(typeAhead(cams, "i", 1), 2);
  assert.equal(typeAhead(cams, "ii", 2), 1);
  // More letters narrow it, and stay on the option they are on while it fits.
  assert.equal(typeAhead(cams, "ip", 1), 1);
  assert.equal(typeAhead(cams, "ipa", 1), 2);
  assert.equal(typeAhead(cams, "x", 0), -1);
  assert.equal(typeAhead(cams, "", 0), -1);
  assert.equal(typeAhead([], "a"), -1);
});

test("the list opens under its button, over it where there is no room, and never leaves the window", () => {
  // Room under: there, as tall as it is.
  assert.deepEqual(placement({ top: 100, bottom: 136 }, 200, 800), { up: false, maxHeight: 652, top: 140 });
  // Near the bottom of the window: over the button, from the window's bottom.
  assert.deepEqual(placement({ top: 700, bottom: 736 }, 200, 800), { up: true, maxHeight: 688, bottom: 104 });
  // It fits under even near the bottom: it stays under.
  assert.equal(placement({ top: 700, bottom: 736 }, 40, 800).up, false);
  // Too tall for either side: the side with more room, and no taller than that (it scrolls).
  assert.deepEqual(placement({ top: 150, bottom: 186 }, 900, 400), { up: false, maxHeight: 202, top: 190 });
  assert.deepEqual(placement({ top: 250, bottom: 286 }, 900, 400), { up: true, maxHeight: 238, bottom: 154 });
  // Right against what opened it when no gap is asked for.
  assert.equal(placement({ top: 10, bottom: 34 }, 100, 800, { gap: 0 }).top, 34);
});
