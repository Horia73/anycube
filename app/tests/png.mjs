// Just enough PNG for the tests: 8-bit RGB or RGBA, not interlaced, to
// RGBA bytes and back; and a picture made smaller by averaging, as a
// canvas's drawImage does when the page grabs a frame.

import { readFileSync } from "node:fs";
import { crc32, deflateSync, inflateSync } from "node:zlib";

export function readPng(path) {
  const file = readFileSync(path);
  let at = 8;
  let width = 0;
  let height = 0;
  let channels = 0;
  const parts = [];
  while (at < file.length) {
    const length = file.readUInt32BE(at);
    const type = file.toString("ascii", at + 4, at + 8);
    const body = file.subarray(at + 8, at + 8 + length);
    if (type === "IHDR") {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      const [depth, colour, , , interlace] = body.subarray(8, 13);
      channels = { 2: 3, 6: 4 }[colour];
      if (depth !== 8 || !channels || interlace) throw new Error(`${path}: only 8-bit RGB(A), not interlaced`);
    } else if (type === "IDAT") {
      parts.push(body);
    } else if (type === "IEND") {
      break;
    }
    at += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(parts));
  const stride = width * channels;
  const rows = new Uint8Array(height * stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    const out = rows.subarray(y * stride, (y + 1) * stride);
    const up = y ? rows.subarray((y - 1) * stride, y * stride) : new Uint8Array(stride);
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? out[x - channels] : 0;
      const b = up[x];
      const c = x >= channels ? up[x - channels] : 0;
      let v = line[x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      out[x] = v & 255;
    }
  }
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0, j = 0; i < width * height; i++, j += channels) {
    data[i * 4] = rows[j];
    data[i * 4 + 1] = rows[j + 1];
    data[i * 4 + 2] = rows[j + 2];
    data[i * 4 + 3] = channels === 4 ? rows[j + 3] : 255;
  }
  return { width, height, data };
}

/** The square (sx, sy, side) of a picture made `size` pixels a side, each pixel the mean of those it covers. */
export function shrink({ width, data }, sx, sy, side, size) {
  const out = new Uint8ClampedArray(size * size * 4);
  const scale = side / size;
  for (let y = 0; y < size; y++) {
    const [y0, y1] = [Math.floor(sy + y * scale), Math.max(Math.floor(sy + y * scale) + 1, Math.floor(sy + (y + 1) * scale))];
    for (let x = 0; x < size; x++) {
      const [x0, x1] = [Math.floor(sx + x * scale), Math.max(Math.floor(sx + x * scale) + 1, Math.floor(sx + (x + 1) * scale))];
      let [r, g, b, k] = [0, 0, 0, 0];
      for (let yy = y0; yy < y1; yy++) {
        for (let xx = x0; xx < x1; xx++) {
          const i = (yy * width + xx) * 4;
          r += data[i];
          g += data[i + 1];
          b += data[i + 2];
          k++;
        }
      }
      const o = (y * size + x) * 4;
      out[o] = r / k;
      out[o + 1] = g / k;
      out[o + 2] = b / k;
      out[o + 3] = 255;
    }
  }
  return { width: size, height: size, data: out };
}

/** RGBA bytes to a PNG file's bytes. */
export function writePng({ width, height, data }) {
  const chunk = (type, body) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(body.length, 0);
    head.write(type, 4, "ascii");
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), body])) >>> 0, 0);
    return Buffer.concat([head, body, crc]);
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header.set([8, 6, 0, 0, 0], 8);
  const rows = Buffer.alloc(height * (width * 4 + 1));
  for (let y = 0; y < height; y++) rows.set(data.subarray(y * width * 4, (y + 1) * width * 4), y * (width * 4 + 1) + 1);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header), chunk("IDAT", deflateSync(rows)), chunk("IEND", Buffer.alloc(0))]);
}
