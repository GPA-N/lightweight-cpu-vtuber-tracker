// Port of crates/smoothing/src/one_euro.rs (Casiez et al. 2012). Single precision, like the Rust.
const f = Math.fround;

function alpha(cutoff, dt) {
  const tau = f(1 / f(2 * f(Math.PI) * cutoff));
  return f(1 / f(1 + f(tau / dt)));
}

export class OneEuro {
  constructor(minCutoff, beta) {
    this.minCutoff = f(minCutoff);
    this.beta = f(beta);
    this.dCutoff = f(1);
    this.prev = null; // [last raw x, last filtered dx]
    this.last = 0;
  }

  reset() {
    this.prev = null;
  }

  // One sample taken `dt` seconds after the previous one.
  filter(x, dt) {
    x = f(x);
    dt = f(dt);
    if (this.prev === null) {
      this.prev = [x, 0];
      this.last = x;
      return x;
    }
    const [xPrev, dxPrev] = this.prev;
    const dx = f(f(x - xPrev) / dt);
    const aD = alpha(this.dCutoff, dt);
    const dxHat = f(f(aD * dx) + f(f(1 - aD) * dxPrev));
    const cutoff = f(this.minCutoff + f(this.beta * Math.abs(dxHat)));
    const a = alpha(cutoff, dt);
    const xHat = f(f(a * x) + f(f(1 - a) * this.last));
    this.prev = [x, dxHat];
    this.last = xHat;
    return xHat;
  }
}
