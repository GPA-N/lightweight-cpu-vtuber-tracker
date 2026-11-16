// Port of crates/capture/src/auto_brightness.rs (on by default in the app: it corrects every camera frame before the
// face and body models). The Rust parity fixtures were dumped without it, so this is checked by unit tests only.
const TARGET = 0.45, SMOOTH_S = 0.8, DEADBAND = 0.04, GAMMA_MIN = 0.4, GAMMA_MAX = 1.8, BLACK = 0.02, SAMPLES = 16384;

export function meanBrightness(rgb, w, h) {
  const step = Math.max(Math.ceil(Math.sqrt(Math.floor((w * h) / SAMPLES))), 1);
  const [x0, x1, y0, y1] = [Math.floor(w / 5), Math.floor((w * 4) / 5), Math.floor(h / 5), Math.floor((h * 4) / 5)];
  let sum = 0, weight = 0;
  for (let y = step >> 1; y < h; y += step) {
    for (let x = step >> 1; x < w; x += step) {
      const i = (y * w + x) * 3;
      const luma = Math.fround(Math.fround(Math.fround(299 * rgb[i] + 587 * rgb[i + 1] + 114 * rgb[i + 2]) / 1000) / 255);
      const k = x >= x0 && x < x1 && y >= y0 && y < y1 ? 2 : 1;
      sum += luma * k;
      weight += k;
    }
  }
  return weight === 0 ? 0 : Math.fround(sum / weight);
}

export const wishGamma = (mean) => (mean < BLACK ? 1 : Math.min(Math.max(Math.log(TARGET) / Math.log(Math.min(mean, 0.98)), GAMMA_MIN), GAMMA_MAX));

export const gammaTable = (g) => Uint8Array.from({ length: 256 }, (_, i) => Math.min(Math.max(Math.fround(Math.pow(i / 255, g) * 255 + 0.5), 0), 255));

export class AutoBrightness {
  constructor() { this.gamma = 1; this.last = null; }
  /** Meters `rgb` and corrects it in place. `now` in seconds. */
  process(rgb, w, h, now) {
    if (!w || !h || rgb.length !== w * h * 3) return;
    const wish = wishGamma(meanBrightness(rgb, w, h));
    const dt = this.last === null ? Infinity : now - this.last;
    this.last = now;
    const alpha = Number.isFinite(dt) ? 1 - Math.exp(-Math.min(dt, 1) / SMOOTH_S) : 1;
    this.gamma += (wish - this.gamma) * alpha;
    if (Math.abs(this.gamma - 1) < DEADBAND) return;
    const lut = gammaTable(this.gamma);
    for (let i = 0; i < rgb.length; i++) rgb[i] = lut[rgb[i]];
  }
}
