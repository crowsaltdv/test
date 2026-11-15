// `/mod-proxy?url=...`: the browser build's way to the mod registry. GitHub's release downloads
// send no CORS headers (and redirect), so the page can't fetch them itself; this passes the
// registry's own addresses only, follows its redirects between them, and caps the size.
const HOSTS = new Set([
  'horizon-rust.github.io',
  'github.com',
  'raw.githubusercontent.com',
  'objects.githubusercontent.com',
  'release-assets.githubusercontent.com',
]);
const MAX_BYTES = 64 * 1024 * 1024;
const MAX_REDIRECTS = 5;

function allowed(url) {
  if (url.protocol !== 'https:' || url.username || url.password || url.port) return false;
  if (!HOSTS.has(url.hostname)) return false;
  // On github.com only the registry's organisation.
  if (url.hostname === 'github.com' && !url.pathname.toLowerCase().startsWith('/horizon-rust/')) return false;
  return true;
}

export async function modProxy(req, res) {
  const fail = (code, text) => { res.writeHead(code, { 'Content-Type': 'text/plain; charset=utf-8' }).end(text); };
  if (req.method !== 'GET') return fail(405, 'GET only');
  let url;
  try { url = new URL(new URL(req.url, 'http://localhost').searchParams.get('url') ?? ''); }
  catch { return fail(400, 'Bad address'); }
  try {
    for (let hop = 0; ; hop++) {
      if (!allowed(url)) return fail(403, 'Not a registry address');
      const upstream = await fetch(url, {
        redirect: 'manual',
        headers: { 'User-Agent': 'horizon-web-mod-proxy' },
        signal: AbortSignal.timeout(60_000),
      });
      if (upstream.status >= 300 && upstream.status < 400 && upstream.headers.get('location')) {
        if (hop >= MAX_REDIRECTS) return fail(502, 'Too many redirects');
        url = new URL(upstream.headers.get('location'), url);
        continue;
      }
      if (!upstream.ok) return fail(upstream.status, `The registry answered ${upstream.status}`);
      const declared = Number(upstream.headers.get('content-length') || 0);
      if (declared > MAX_BYTES) return fail(413, 'Too large');
      const body = Buffer.from(await upstream.arrayBuffer());
      if (body.length > MAX_BYTES) return fail(413, 'Too large');
      res.writeHead(200, {
        'Content-Type': 'application/octet-stream',
        'Content-Length': body.length,
        'Cache-Control': 'public, max-age=300',
      }).end(body);
      return;
    }
  } catch (e) {
    return fail(502, 'The registry can\'t be reached');
  }
}
