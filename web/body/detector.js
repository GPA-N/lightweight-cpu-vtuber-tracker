// Main-thread handle of the RF-DETR worker. One job at a time: `detect` while busy is refused (the Rust drops it too).
export class PersonDetectorWorker {
  constructor({ source, ortUrl, wasmPaths }) {
    this.source = source;
    this.ortUrl = ortUrl;
    this.wasmPaths = wasmPaths;
    this.busy = false;
    this.nextId = 1;
    this.pending = null;
  }

  init() {
    this.worker = new Worker(new URL('./person-worker.js', import.meta.url), { type: 'module' });
    return new Promise((resolve, reject) => {
      this.worker.onmessage = ({ data: m }) => {
        if (m.type === 'ready') {
          this.worker.onmessage = (e) => this.#on(e.data);
          resolve();
        } else if (m.type === 'error') reject(new Error(m.message));
      };
      this.worker.onerror = (e) => reject(new Error(e.message));
      this.worker.postMessage({ type: 'init', ortUrl: this.ortUrl, wasmPaths: this.wasmPaths, source: this.source });
    });
  }

  #on(m) {
    const p = this.pending;
    if (!p || p.id !== m.id) return;
    this.pending = null;
    this.busy = false;
    if (m.type === 'error') p.reject(new Error(m.message));
    else p.resolve({ box: m.box, ms: m.ms });
  }

  /** Resolves `{ box | null, ms }`, or null at once when the worker is busy. The pixels are copied to the worker. */
  detect(frame) {
    if (this.busy) return Promise.resolve(null);
    this.busy = true;
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending = { id, resolve, reject };
      this.worker.postMessage({ type: 'detect', id, rgb: frame.rgb, w: frame.w, h: frame.h });
    });
  }

  close() {
    this.worker?.terminate();
  }
}
