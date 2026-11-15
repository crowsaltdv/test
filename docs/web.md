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

`render.yaml` is a blueprint for a small Node web service (`web/server.mjs`, no dependencies) that
serves `web/dist` with the cross-origin isolation headers the page needs (without them the worker
threads never start) and the `/mod-proxy` route the mod browser uses. The game is not built on
Render: a fresh build takes far longer than a free instance allows.

1. Build and pack on your machine: `./web/dist.ps1` (`-SkipBuild` packs the last build).
2. Commit `web/dist` (the wasm is about 70 MB, under GitHub's 100 MB limit) and push.
3. In Render, choose **New, Blueprint** and pick the repository.

`web/dist` holds the page, the loader and the wasm only. It has no Minecraft files, and must
not: each visitor loads their own prepared game files in the page, which keeps them in their
browser. Repack and push for every new build.

## Mods (`.hmod`)

The browser build runs content mods (`.hmod`: data and assets, no code), the same files as on
desktop. Mods are off until switched on.

- **Mods button.** The title screen has a Mods button under Realms; it opens Options, Mods, the
  game's own mod browser. Options also lists Mods where the desktop build lists telemetry.
- **Mod browser.** It lists and installs from the Horizon mod registry as on desktop (size,
  SHA-256 and signature checks included). A browser can't fetch the registry's GitHub downloads
  itself (no CORS headers), so the game asks the page's own `/mod-proxy` (`web/mod-proxy.mjs`,
  through `web/http.js` and `horizon-system`'s `web_http`). It passes only the registry's hosts,
  follows redirects between them and caps a download at 64 MB. `web/serve.mjs` and
  `web/server.mjs` both have it; a plain static host does not, and the browser then says the
  registry can't be reached.
- **Icons.** Each project's icon is fetched with the list (through the same proxy), scaled to
  128 x 128 and drawn beside its row, in atlas slots the pack screen doesn't share a frame with.
- **What is offered.** Everything the registry has for this game is listed, installable ones
  first. The 11 content mods (`.hmod` with data and assets) and the resource packs install: a pack
  goes to `resourcepacks/<id>.zip` (size and SHA-256 checked against the index) and is switched on
  in Options > Resource Packs. The rest are listed as "(desktop only)", with the reason on their
  page and an inactive button. The registry's native mods (`.hnmod`) are patches to the game's
  own Rust source, compiled into the desktop game, so a browser, which runs one prebuilt game,
  can't load them; shader packs need the desktop renderer, modpacks hold native mods, and a
  `.hmod` with code is one this game doesn't run.
- **After an install,** restart the game (Reload game): the mod's blocks and items appear in the
  creative tabs and in `/give`. A mod that adds blocks makes the restart take longer, since the
  game rebuilds its block tables once per start.
- **Upload Mod.** The Mods page's folder button is **Upload Mod...** in the browser (a folder
  can't be opened): it opens a file picker for a `.hmod`. The page reads the mod's id from its
  `horizon-mod.toml`, saves it as `horizon-mods/<id>.hmod`, turns it on in `horizon-mods.txt` and
  offers to reload the game, which reads its mods once at start. A file that isn't a Horizon mod
  is refused with the reason. Dropping a `.hmod` on the page does the same.

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
- **Loading runs one step at a time.** A thread started from another thread only begins once the
  page's own thread is back in the browser's event loop, and that thread can be busy waiting for
  a lock one of those threads holds. Loading with a content mod that adds blocks (a rebuilt block
  table, many more textures) deadlocked on this, so in the browser the title screen's panorama
  and atlas, the texture decoding and the mod content build run on the loading thread itself
  (`resource_load.rs`, `resource_tables.rs`, `hud.rs`, `mod_content.rs`), and `worker_count`
  stops at 4. Desktop keeps its threads.
- A browser has no executable file (`current_exe` fails): the mod content cache is keyed on the
  game's version there instead (`mod_content.rs`, `stamp`). Before that, a mod's content was never
  loaded in the browser (`content mods not loaded: no /proc/self/exe`).
- Fullscreen (the page's button) and the browser's own (F11) both hide the page's top bar.
- Browser GPU drivers may expose fewer features than desktop GPUs. The browser path uses the
  regular SDR renderer when HDR output is unavailable.
