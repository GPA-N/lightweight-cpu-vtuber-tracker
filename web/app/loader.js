// Model downloads with a progress callback and the Cache API: a second visit does not download the large files again.
// Every source ends up as one blob URL that all workers can fetch without copying the bytes through postMessage.
const CACHE = 'vt-models-v1';

async function fetchWithProgress(url, onBytes) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  const reader = res.body.getReader();
  const parts = [];
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    parts.push(value);
    onBytes(value.length);
  }
  return new Blob(parts);
}

/**
 * `files`: [{ label, urls: [url, ...], size }] (several urls are chunks, joined in order). `size` is the expected total in
 * bytes (for the progress bar only). `onProgress({ label, loaded, total, fromCache })` is called as bytes arrive.
 * Returns { [label]: blobUrl }. Stores each url in the Cache API; a cached url is read from there.
 */
export async function loadModels(files, onProgress) {
  const cache = 'caches' in globalThis ? await caches.open(CACHE) : null;
  const total = files.reduce((n, f) => n + f.size, 0);
  let loaded = 0;
  const report = (label, fromCache) => onProgress?.({ label, loaded, total, fromCache });
  const out = {};
  await Promise.all(files.map(async (file) => {
    const blobs = [];
    for (const url of file.urls) {
      const hit = await cache?.match(url);
      if (hit) {
        const blob = await hit.blob();
        loaded += blob.size;
        report(file.label, true);
        blobs.push(blob);
        continue;
      }
      const blob = await fetchWithProgress(url, (n) => { loaded += n; report(file.label, false); });
      // A failed cache write (quota, private mode) only costs the next visit a download.
      try { await cache?.put(url, new Response(blob)); } catch { /* ignore */ }
      blobs.push(blob);
    }
    out[file.label] = URL.createObjectURL(blobs.length === 1 ? blobs[0] : new Blob(blobs));
  }));
  return out;
}
