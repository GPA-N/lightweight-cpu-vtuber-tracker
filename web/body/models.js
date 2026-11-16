// Model sources are `{ url } | { chunks: [url, ...] } | { bytes }`; this turns one into the file's bytes.
export async function loadBytes(src) {
  if (src.bytes) return src.bytes instanceof Uint8Array ? src.bytes : new Uint8Array(src.bytes);
  const get = async (url) => {
    const r = await fetch(url);
    if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
    return new Uint8Array(await r.arrayBuffer());
  };
  if (src.url) return get(src.url);
  if (src.chunks) {
    const parts = await Promise.all(src.chunks.map(get));
    const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
    let at = 0;
    for (const p of parts) {
      out.set(p, at);
      at += p.length;
    }
    return out;
  }
  throw new Error('model source needs url, chunks or bytes');
}
