import { createServer } from 'node:http';
import { createReadStream, readFileSync, statSync } from 'node:fs';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { modProxy } from './mod-proxy.mjs';
import { locatePrepared, preparedResources } from './prepared-resources.mjs';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const types = new Map([
  ['.html', 'text/html; charset=utf-8'], ['.js', 'text/javascript; charset=utf-8'],
  ['.wasm', 'application/wasm'], ['.data', 'application/octet-stream'],
  ['.json', 'application/json; charset=utf-8'], ['.css', 'text/css; charset=utf-8'],
  ['.png', 'image/png'], ['.svg', 'image/svg+xml'],
  ['.webmanifest', 'application/manifest+json; charset=utf-8'],
]);
// The prepared game files the page loads by itself, so running the browser build needs no folder
// picker: the `26.3` folder in this repository (see preparedResources). HORIZON_WEB_RESOURCES names
// another folder (absolute, or relative to the repository).
const prepared = preparedResources(root, process.env.HORIZON_WEB_RESOURCES);
const preparedFolder = locatePrepared(root, process.env.HORIZON_WEB_RESOURCES);
// The page is served from the repository root, but the app's own files (manifest, service worker,
// icons) sit in web/ and packaging/: the paths the manifest declares are mapped here, as
// web/dist.ps1 lays them out when packing.
const wasm = resolve(root, 'target/wasm32-unknown-emscripten/fast/horizon_client.wasm');
const appFiles = new Map([
  ['/manifest.webmanifest', 'web/manifest.webmanifest'],
  ['/favicon.svg', 'packaging/icons/hicolor/scalable/apps/horizon.svg'],
  ['/icons/icon-256.png', 'packaging/icons/hicolor/256x256/apps/horizon.png'],
  ['/icons/icon-512.png', 'packaging/icons/hicolor/512x512/apps/horizon.png'],
]);

createServer((req, res) => {
  res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
  res.setHeader('Cross-Origin-Resource-Policy', 'same-origin');
  let pathname;
  try { pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname); }
  catch { res.writeHead(400).end('Bad request'); return; }
  if (pathname === '/mod-proxy') { modProxy(req, res); return; }
  if (prepared(req, res, pathname)) return;
  if (pathname === '/sw.js') { serveServiceWorker(res); return; }
  const appFile = appFiles.get(pathname);
  if (appFile) pathname = `/${appFile}`;
  if (pathname === '/') pathname = '/web/index.html';
  const path = resolve(root, `.${pathname}`);
  if (path !== root && !path.startsWith(root + sep)) { res.writeHead(403).end('Forbidden'); return; }
  let info;
  try { info = statSync(path); } catch { res.writeHead(404).end('Not found'); return; }
  if (!info.isFile()) { res.writeHead(404).end('Not found'); return; }
  res.setHeader('Content-Type', types.get(extname(path)) || 'application/octet-stream');
  res.setHeader('Content-Length', info.size);
  createReadStream(path).pipe(res);
}).listen(Number(process.env.PORT || 8000), '127.0.0.1', () => {
  const where = preparedFolder ? preparedFolder.replace(root, '.') : 'none (the page shows its picker)';
  console.log(`Horizon web client: http://127.0.0.1:${process.env.PORT || 8000}`);
  console.log(`Prepared game files: ${where}`);
});

// web/sw.js with its cache version set from the wasm this server hands out (web/dist.ps1 does the
// same with the wasm's hash when packing): a rebuilt game means a new worker and a new cache.
function serveServiceWorker(res) {
  let version = 'dev';
  try {
    const info = statSync(wasm);
    version = `${Math.floor(info.mtimeMs)}-${info.size}`;
  } catch { /* no build yet: the worker still installs, and a later build replaces it */ }
  const body = readFileSync(resolve(root, 'web/sw.js'), 'utf8')
    .replace(/const VERSION = '[^']*';/, `const VERSION = '${version}';`);
  res.writeHead(200, {
    'Content-Type': 'text/javascript; charset=utf-8',
    'Content-Length': Buffer.byteLength(body),
    // The browser must see a new worker on the next visit, not a day-old copy.
    'Cache-Control': 'no-store',
  });
  res.end(body);
}
