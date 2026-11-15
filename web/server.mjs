// Production server (Render, render.yaml): serves web/dist with the cross-origin isolation
// headers and the registry proxy the in-game mod browser uses. `node web/server.mjs`.
// It also serves this repository's prepared 26.3 folder when there is one (web/prepared-resources.mjs,
// locatePrepared), so a local run of the packed page starts by itself; the deployed site finds none,
// and holds and serves none of Minecraft's files.
import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { modProxy } from './mod-proxy.mjs';
import { locatePrepared, preparedResources } from './prepared-resources.mjs';

const root = resolve(fileURLToPath(new URL('./dist', import.meta.url)));
const repository = resolve(fileURLToPath(new URL('..', import.meta.url)));
const types = new Map([
  ['.html', 'text/html; charset=utf-8'], ['.js', 'text/javascript; charset=utf-8'],
  ['.wasm', 'application/wasm'], ['.json', 'application/json; charset=utf-8'],
  ['.css', 'text/css; charset=utf-8'], ['.png', 'image/png'], ['.svg', 'image/svg+xml'],
  ['.webmanifest', 'application/manifest+json; charset=utf-8'],
]);
const prepared = preparedResources(locatePrepared(repository, process.env.HORIZON_WEB_RESOURCES));

createServer((req, res) => {
  res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
  res.setHeader('Cross-Origin-Resource-Policy', 'same-origin');
  let pathname;
  try { pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname); }
  catch { res.writeHead(400).end('Bad request'); return; }
  if (pathname === '/mod-proxy') { modProxy(req, res); return; }
  if (prepared(req, res, pathname)) return;
  if (pathname === '/') pathname = '/index.html';
  const path = resolve(root, `.${pathname}`);
  if (path !== root && !path.startsWith(root + sep)) { res.writeHead(403).end('Forbidden'); return; }
  let info;
  try { info = statSync(path); } catch { res.writeHead(404).end('Not found'); return; }
  if (!info.isFile()) { res.writeHead(404).end('Not found'); return; }
  res.setHeader('Content-Type', types.get(extname(path)) || 'application/octet-stream');
  res.setHeader('Content-Length', info.size);
  createReadStream(path).pipe(res);
}).listen(Number(process.env.PORT || 10000), '0.0.0.0', () => {
  console.log(`Horizon web client on port ${process.env.PORT || 10000}`);
});
