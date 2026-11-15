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

## Hosting on Render

`render.yaml` is a blueprint for a static site that publishes `web/dist` with the
cross-origin isolation headers the page needs (without them the worker threads never start). The
game is not built on Render: a fresh build takes far longer than a free instance allows.

1. Build and pack on your machine: `./web/dist.ps1` (`-SkipBuild` packs the last build).
2. Commit `web/dist` (the wasm is about 70 MB, under GitHub's 100 MB limit) and push.
3. In Render, choose **New, Blueprint** and pick the repository.

`web/dist` holds the page, the loader and the wasm only. It has no Minecraft files, and must
not: each visitor loads their own prepared game files in the page, which keeps them in their
browser. Repack and push for every new build.

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

## Performance

A frame counter that averages over a second hides stalls: a burst of 7 ms frames and one 300 ms
freeze still read as a high rate. Measure frame times (the longest, not the mean), as the numbers
below were: sprint-flying into new terrain in a new world for 14 seconds.

| | Frames over 20 ms | Median frame | Worst frame |
|---|---|---|---|
| Render distance 12, before | 61 to 84 | 8 ms | 450 to 870 ms |
| Render distance 8, after | 0 | 2 ms | 12 ms |

What changed, and why:

- **Render distance 8 by default** (`DEFAULT_RENDER_DISTANCE`, simulation distance too). In a
  browser every chunk of view costs WebGL calls, and at 12 the page stalled for hundreds of
  milliseconds at a time while terrain loaded. The page moves options saved with the old 12 to 8
  once (`migrateWebDefaults`, remembered in `localStorage`); what you set in the game's options
  afterwards stays. Higher still works, at the cost of stalls on slower machines.
- **No software frame limiter.** The page's animation frame paces rendering once per refresh;
  the desktop limiter sleeps and spins on the window's thread, which in a browser holds the page
  for the rest of every frame and misses the next refresh by a little.
- **Fewer GL calls per draw** (`third_party/wgpu-hal`, `gles/queue.rs`). WebGL has no base
  vertex, so the renderer's per-chunk draws point every vertex attribute further into the buffer
  first. It used to point them back after each draw, bind the buffer once per attribute and say
  "enable" and "divisor" again for the per-chunk origin: about 23 GL calls per draw. Now the
  pointers stay where the last draw left them until something needs them at base 0, the buffer is
  bound once and unchanged state isn't repeated: about 10.

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
