import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { modProxy } from './mod-proxy.mjs';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const types = new Map([
  ['.html', 'text/html; charset=utf-8'], ['.js', 'text/javascript; charset=utf-8'],
  ['.wasm', 'application/wasm'], ['.data', 'application/octet-stream'],
  ['.json', 'application/json; charset=utf-8'], ['.css', 'text/css; charset=utf-8'],
]);

createServer((req, res) => {
  res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
  res.setHeader('Cross-Origin-Resource-Policy', 'same-origin');
  let pathname;
  try { pathname = decodeURIComponent(new URL(req.url, 'http://localhost').pathname); }
  catch { res.writeHead(400).end('Bad request'); return; }
  if (pathname === '/mod-proxy') { modProxy(req, res); return; }
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
  console.log(`Horizon web client: http://127.0.0.1:${process.env.PORT || 8000}`);
});
