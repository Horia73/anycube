// Pixel-side fixture for tests/solution_link.rs. Uses the app's real vision.
import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { assembleFaces, readFace, rotateSquare } from "../js/robot.js";
import { readPng, shrink } from "./png.mjs";

const directory = process.argv[2];
const faces = [];
for await (const line of createInterface({ input: process.stdin })) {
  const event = JSON.parse(line);
  if (event.type === "photo") {
    const path = join(directory, `${event.index}.png`);
    // In app-camera mode the emulator writes what the camera would see.
    if (event.png) writeFileSync(path, Buffer.from(event.png, "base64"));
    const frame = readPng(path);
    const image = shrink(frame, 0, 0, frame.width, 240);
    image.data = rotateSquare(image.data, image.width, event.rotate);
    const { cells, source } = readFace(image, 3);
    assert.ok(cells && source.found, `face ${event.face} not detected`);
    faces["URFDLB".indexOf(event.face)] = cells;
    console.log("ACK");
  } else {
    const result = assembleFaces(3, faces.flat());
    assert.ok(result.ok, result.note);
    console.log(result.state);
  }
}
