import assert from "node:assert/strict";
import test from "node:test";
import { CameraFeed } from "../js/camera.js";

const input = (deviceId, label) => ({ deviceId, label, kind: "videoinput" });
const mac = input("builtin", "FaceTime HD Camera");
const desk = input("desk", "Horia’s iPhone Desk View Camera");
const phone = input("rear", "Horia’s iPhone Camera");
const usb = input("usb", "USB Video Camera");

function setup(t, devices, { hidden = false, native = true } = {}) {
  const requests = [], tracks = [], previews = [];
  const media = {
    devices,
    async enumerateDevices() {
      return hidden && !tracks.length ? [input("", "")] : this.devices;
    },
    async getUserMedia({ video }) {
      const id = video.deviceId?.exact;
      requests.push(id || video.facingMode.ideal);
      const device = id ? this.devices.find(d => d.deviceId === id) : this.devices[0];
      assert.ok(device);
      const track = { label: device.label, getSettings: () => ({ deviceId: device.deviceId }),
        stop() { this.stopped = true; } };
      tracks.push(track);
      return { getTracks: () => [track], getVideoTracks: () => [track] };
    },
  };
  Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: media });
  globalThis.anycubeNativeMac = native;
  t.after(() => { delete navigator.mediaDevices; delete globalThis.anycubeNativeMac; });
  const video = { set srcObject(stream) { if (stream) previews.push(stream.getTracks()[0].label); }, async play() {} };
  return { feed: new CameraFeed(video), requests, previews, tracks, media };
}

test("automatic and remembered Desk View both open the real iPhone by exact ID", async t => {
  const { feed, requests } = setup(t, [mac, desk, phone]);
  await feed.open();
  await feed.open(desk.deviceId, { remembered: true });
  assert.deepEqual(requests, ["rear", "rear"]);
  assert.deepEqual(feed.cameras, [mac, phone]);
});

test("permission probe is stopped before the iPhone preview is attached", async t => {
  const { feed, tracks, previews, requests } = setup(t, [mac, desk, phone], { hidden: true });
  await feed.open();
  assert.deepEqual(requests, ["environment", "rear"]);
  assert.equal(tracks[0].stopped, true);
  assert.deepEqual(previews, [phone.label]);
});

test("missing iPhone leaves FaceTime available for deliberate selection and supports reconnect", async t => {
  const { feed, requests, media } = setup(t, [mac, desk]);
  for (const id of ["rear", "desk", "builtin", undefined]) {
    await assert.rejects(feed.open(id, { remembered: true }), { name: "CameraChoiceError" });
  }
  assert.deepEqual(requests, []);
  await feed.open("builtin");
  media.devices = [desk, usb, phone];
  await feed.refresh();
  assert.deepEqual(feed.cameras, [usb, phone]);
  await feed.open();
  await feed.open("usb");
  assert.deepEqual(requests, ["builtin", "rear", "usb"]);
});

test("hidden labels with only FaceTime and Desk View leave the preview off", async t => {
  const { feed, tracks, previews } = setup(t, [mac, desk], { hidden: true });
  await assert.rejects(feed.open(), { name: "CameraChoiceError" });
  assert.equal(tracks[0].stopped, true);
  assert.deepEqual(previews, []);
  assert.deepEqual(feed.cameras, [mac]);
});

test("ordinary web cameras retain the browser facing-mode default and explicit selection", async t => {
  const { feed, requests } = setup(t, [mac, usb], { native: false });
  await feed.open();
  await feed.open("usb");
  assert.deepEqual(requests, ["environment", "usb"]);
});

test("closing a pending native start releases its token and rejects a late grant", async t => {
  const { feed, requests, previews } = setup(t, [mac, desk]);
  const messages = [];
  let grant, started;
  const pending = new Promise(resolve => { started = resolve; });
  const external = { id: "external-phone", label: phone.label };
  globalThis.webkit = { messageHandlers: { anycubeCamera: { async postMessage(message) {
    messages.push(message);
    if (message.op === "list") return [external];
    if (message.op === "start") {
      started();
      return new Promise(resolve => { grant = () => resolve(external); });
    }
    assert.equal(message.op, "stop");
  } } } };
  t.after(() => { delete globalThis.webkit; });
  const open = feed.open("desk", { remembered: true });
  await pending;
  feed.close();
  grant();
  await assert.rejects(open, { name: "AbortError" });
  assert.deepEqual(messages.map(m => m.op), ["list", "start", "stop"]);
  assert.equal(messages[1].id, external.id);
  assert.equal(messages[1].token, messages[2].token);
  assert.deepEqual(requests, []);
  assert.deepEqual(previews, []);
});

test("denied native permission leaves both camera paths stopped", async t => {
  const { feed, requests, previews } = setup(t, [mac, desk]);
  const ops = [];
  globalThis.webkit = { messageHandlers: { anycubeCamera: { async postMessage({ op }) {
    ops.push(op);
    if (op === "list") return [{ id: "external-phone", label: phone.label }];
    if (op === "start") throw new Error("Camera permission denied");
  } } } };
  t.after(() => { delete globalThis.webkit; });
  await assert.rejects(feed.open(), { name: "NotAllowedError" });
  assert.deepEqual(ops, ["list", "start", "stop"]);
  assert.deepEqual(requests, []);
  assert.deepEqual(previews, []);
  assert.equal(feed.running, false);
});
