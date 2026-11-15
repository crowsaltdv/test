// Horizon's service worker: keeps the page, Emscripten's loader and the wasm on disk, so the site
// installs as an app and starts again without a network. The shell is cache-first. The prepared
// game files and the mod proxy always go to the network: they are live data, not part of the app.
//
// VERSION is rewritten when the build is packed (web/dist.ps1: the wasm's hash) and when the
// development server serves this file (web/serve.mjs: the wasm's time and size). A rebuilt game
// therefore installs a new worker and drops the old cache; nobody has to remember to bump it.
const VERSION = 'dev';
const CACHE = `horizon-web-${VERSION}`;
const SHELL = ['/', '/manifest.webmanifest', '/icons/icon-256.png', '/icons/icon-512.png'];

self.addEventListener('install', event => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    await cache.addAll(SHELL);
    await self.skipWaiting();
  })());
});

self.addEventListener('activate', event => {
  event.waitUntil((async () => {
    const names = await caches.keys();
    await Promise.all(names.filter(name => name !== CACHE).map(name => caches.delete(name)));
    await self.clients.claim();
  })());
});

// The loader and the wasm are fetched before a freshly installed worker controls the page, so the
// page asks for them once it has started. A later launch then needs no network at all.
self.addEventListener('message', event => {
  if (event.data?.type !== 'warm') return;
  event.waitUntil(warm(event.data.urls || []));
});

async function warm(urls) {
  const cache = await caches.open(CACHE);
  for (const url of urls) {
    if (await cache.match(url)) continue;
    try {
      const response = await fetch(url, { cache: 'no-store' });
      if (cacheable(response)) await cache.put(url, response);
    } catch { /* the next start tries again */ }
  }
}

self.addEventListener('fetch', event => {
  const { request } = event;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.origin !== location.origin) return;
  if (url.pathname === '/mod-proxy' || url.pathname.startsWith('/prepared-resources')) return;
  event.respondWith(request.mode === 'navigate' ? networkFirst(request) : cacheFirst(request));
});

// A navigation may carry a query string (`/?x`), and the app is one page: the cached page answers.
// A reachable but broken server (a proxy's 502 while the game machine is off, say) counts as no
// server: an installed app shows its cached page instead of the error page.
async function networkFirst(request) {
  try {
    const response = await fetch(request);
    if (cacheable(response)) await store('/', response.clone());
    if (response.status < 500) return response;
    const cached = await caches.match('/');
    return cached || response;
  } catch (error) {
    const cached = await caches.match('/');
    if (cached) return cached;
    throw error;
  }
}

async function cacheFirst(request) {
  const cached = await caches.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (cacheable(response)) await store(request, response.clone());
  return response;
}

function cacheable(response) {
  return response.status === 200 && response.type === 'basic';
}

async function store(key, response) {
  const cache = await caches.open(CACHE);
  await cache.put(key, response);
}
