// AVFoundation IDs and WebKit device IDs are separate namespaces.
const PREFIX = "anycube-av:";
export const nativeId = id => typeof id === "string" && id.startsWith(PREFIX);

function bridge() {
  return globalThis.anycubeNativeMac && globalThis.webkit?.messageHandlers?.anycubeCamera;
}

export async function nativeInputs() {
  const handler = bridge();
  if (!handler) return [];
  const devices = await handler.postMessage({ op: "list" });
  return devices.map(({ id, label }) => ({ kind: "videoinput", deviceId: PREFIX + id, label }));
}

export const hasNativeCamera = () => !!bridge();

// Draw only new AVFoundation samples, at their decoded dimensions. Capturing
// the canvas feeds the very same video element used by both scanners.
export class NativeCameraStream {
  constructor(onEnded) {
    this.handler = bridge();
    this.token = crypto.getRandomValues(new Uint32Array(4)).join("-");
    this.onEnded = onEnded;
    this.closed = false;
    this.waiters = new Set();
  }

  request(op, args) {
    return this.handler.postMessage({ op, token: this.token, ...args });
  }

  check() {
    if (this.closed) throw new DOMException("Camera choice changed.", "AbortError");
  }

  async draw() {
    const pending = [...this.waiters];
    const jpeg = await this.request("frame");
    this.check();
    if (!jpeg) return false;
    const image = new Image();
    image.src = "data:image/jpeg;base64," + jpeg;
    await image.decode();
    this.check();
    if (this.canvas.width !== image.naturalWidth || this.canvas.height !== image.naturalHeight) {
      this.canvas.width = image.naturalWidth;
      this.canvas.height = image.naturalHeight;
    }
    this.canvas.getContext("2d").drawImage(image, 0, 0);
    for (const resolve of pending) resolve(this.canvas);
    return true;
  }

  async start(deviceId) {
    const device = await this.request("start", nativeId(deviceId) ? { id: deviceId.slice(PREFIX.length) } : {});
    this.check();
    this.deviceId = PREFIX + device.id;
    this.canvas = document.createElement("canvas");
    const deadline = performance.now() + 10000;
    while (!(await this.draw())) {
      if (performance.now() > deadline) throw new Error("The iPhone camera did not deliver a picture.");
      await new Promise(resolve => setTimeout(resolve, 50));
      this.check();
    }
    this.stream = this.canvas.captureStream();
    this.lastFrame = performance.now();
    this.schedule();
    return this.stream;
  }

  schedule() {
    this.timer = setTimeout(async () => {
      try {
        if (await this.draw()) this.lastFrame = performance.now();
        else if (performance.now() - this.lastFrame > 3000) throw new Error("iPhone camera disconnected");
        this.check();
        this.schedule();
      } catch {
        if (!this.closed) this.onEnded();
      }
    }, 65);
  }

  // A robot hold needs a new sample. Discard the first pull after the
  // request: AVFoundation may still have a pre-hold sample buffered.
  async freshFrame() {
    for (let i = 0; i < 2; i++) {
      if (this.closed) return null;
      const frame = await new Promise(resolve => {
        const finish = value => {
          clearTimeout(timeout);
          this.waiters.delete(finish);
          resolve(value);
        };
        const timeout = setTimeout(() => finish(null), 1000);
        this.waiters.add(finish);
      });
      if (!frame) return null;
    }
    return this.canvas;
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    clearTimeout(this.timer);
    for (const resolve of this.waiters) resolve(null);
    this.stream?.getTracks().forEach(track => track.stop());
    void this.request("stop").catch(() => {});
  }
}
