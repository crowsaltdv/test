# Browser build

The experimental browser client runs the Rust game through Emscripten, pthreads and WebGL 2.
It has been built and launched in Chromium through the missing prepared game files screen.

## Build and run

Install Rust nightly with the `wasm32-unknown-emscripten` target and `rust-src`, and install
Emscripten 7.0.0 in `../emsdk` beside the repository. From the repository root in PowerShell:

```powershell
./web/build.ps1
node ./web/serve.mjs
```

Open <http://127.0.0.1:8000>. The server binds to loopback and sends the cross-origin isolation
headers required for shared-memory WebAssembly threads. Opening the page as a `file:` URL or
serving it without those headers will prevent the worker pool from starting.

Choose **Start without game files** to reach Horizon's missing-files screen. To continue past it,
choose **Load prepared game files** and select the prepared Minecraft 26.3 resources directory
produced by `horizon-prepare`. The browser copies those files into the virtual `/resources`
filesystem before starting the client and saves a copy in IndexedDB. On later visits, **Play** loads
that saved copy; **Clear files** removes it. **Fullscreen** expands the game to the whole screen and
hides the page's top bar; press Escape to leave fullscreen.
The generated files are not included in this repository.

The build script uses `cargo +nightly -Z build-std=std,panic_abort`, because the distributed
Emscripten Rust standard library is not compiled for shared-memory atomics. It builds with no
default features: no audio, controllers or Horizon extras. Emscripten emits
`target/wasm32-unknown-emscripten/fast/horizon-client.js` and `horizon_client.wasm`.

## Mouse and fullscreen

The game turns its view with the browser's pointer lock, which hides the cursor and keeps it from
reaching the edge of the screen. A browser grants the lock only to a click or a key press, so:

- When the game asks for the pointer and the page has had a click or key in the last few seconds
  (Back to Game, for one), it locks at once.
- Otherwise (a world that finished loading on its own, say) the cursor stays visible until the
  next click or key. That click only takes the pointer: the game does not hear it, so it does not
  break a block.
- Escape releases the lock, as in every browser, and pauses the game. The page's fullscreen has
  the same lock; Escape again leaves fullscreen.

## Browser limits

- WebGL 2 is required. Rendering stays on the browser main thread; game worker threads use the
  shared Emscripten filesystem and CPU work.
- Sound, controllers, online sign-in and multiplayer sockets are unavailable in this build.
- Saves and options in `/game` are restored from this site's IndexedDB before launch and copied
  back there every three seconds, when the page is hidden, and before using **Reload game**. They
  are stored in this browser profile rather than the desktop Minecraft folder; clearing this site's
  browser data removes them. Browser shutdown can interrupt the last background copy, and browser
  storage quotas still apply.
- Copying prepared files into memory can take time and several hundred megabytes of memory.
- The browser build caps Horizon's worker pool at four threads to leave room for the browser event
  loop while creating, loading and saving worlds. The network pool has one thread (the only
  connection is the integrated server's own player) and the tick's helper pool at most three.
- Every thread is a Web Worker out of a pool of 64 that the page starts with. A new thread beyond
  those begins only once the window's thread is back in the browser's event loop, so code that runs
  on that thread (a frame) must never start threads and wait for them in one call: it never
  returns. Leaving a world did exactly that (the title panorama's six decoder threads) and froze
  the page; the browser build keeps the decoded panorama instead (`gfx/panorama.rs`).
- Fullscreen (the page's button) and the browser's own (F11) both hide the page's top bar.
- Browser GPU drivers may expose fewer features than desktop GPUs. The browser path uses the
  regular SDR renderer when HDR output is unavailable.
