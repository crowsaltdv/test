//! Generates the native mod API, and gives the Windows executable Horizon's icon and its stack
//! (`packaging/windows/exe.rs`, shared with the official executable's build script).
#[path = "../../packaging/windows/exe.rs"]
mod windows_exe;

fn main() {
    let root =
        std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    horizon_mod_api_gen::generate(&root, &out_dir, horizon_mod_api_gen::Target::Client)
        .expect("generating native mod API");
    // A workspace horizon-mixins changed holds native mods, whichever tool ran it (the launcher,
    // or a mod's own script with no `native-mods.json` beside its game): `HORIZON_MIXIN_MODS`
    // names them for `game::mods::built_with_native_mods`. Watched only once it exists: a
    // bundle applied in place also edits `game/`, which `generate` watches.
    let stage = root.join(horizon_mod_api_gen::MIXINS_STAGE);
    if stage.is_file() {
        println!("cargo:rerun-if-changed={}", stage.display());
    }
    let staged = horizon_mod_api_gen::staged_mods(&root);
    if !staged.is_empty() {
        println!("cargo:rustc-env=HORIZON_MIXIN_MODS={}", staged.join(","));
    }
    // Whether a native mod can run in a mob's turn (`game/creatures/mob_islands.rs`). The
    // launcher adds trait packages to the workspace's members.
    println!(
        "cargo:rerun-if-changed={}",
        root.join("Cargo.toml").display()
    );
    for dir in ["native-api", "native-crates"] {
        if root.join(dir).is_dir() {
            println!("cargo:rerun-if-changed={}", root.join(dir).display());
        }
    }
    if let Some(found) = horizon_mod_api_gen::mob_turn(&root) {
        println!("cargo:rustc-env=HORIZON_MOB_TURN={found}");
    }
    windows_exe::resources("horizon-client");
    web_link();
}

/// What Emscripten's linker is told for the browser build (docs/web.md), where the executable
/// is `horizon-client.js` and `horizon-client.wasm`; nothing on any other target.
fn web_link() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("emscripten") {
        return;
    }
    const ARGS: &[&str] = &[
        // Let Emscripten use a faster post-link pass than Cargo's `fast` profile requests. Rust
        // has already optimized the code at level 3; re-optimizing the 70 MB module at -O3 is
        // prohibitively slow on the browser build machine.
        "-O1",
        // The game's threads are Web Workers on one shared memory. A worker starts only once
        // the page's main thread is back in its event loop, and the game waits for its threads
        // from that thread, so the workers it will ask for are started with the page.
        "-pthread",
        "-sPTHREAD_POOL_SIZE=64",
        "-sPTHREAD_POOL_SIZE_STRICT=0",
        // Memory grows up to what a 32-bit program can address.
        "-sALLOW_MEMORY_GROWTH=1",
        "-sINITIAL_MEMORY=536870912",
        "-sMAXIMUM_MEMORY=4294967296",
        // The main thread's 8 MB, as on every other system (packaging/windows/exe.rs).
        "-sSTACK_SIZE=8388608",
        // Emscripten's build of mimalloc: a heap per thread (horizon-threads' `web_alloc.rs`).
        "-sMALLOC=mimalloc",
        // Files in the program's own memory, read and written by any thread without a trip to
        // the main one; the page puts the prepared resources there before `main` runs.
        "-sWASMFS=1",
        "-sFORCE_FILESYSTEM=1",
        // wgpu's OpenGL ES backend, on WebGL 2 through Emscripten's EGL.
        "-sMIN_WEBGL_VERSION=2",
        "-sMAX_WEBGL_VERSION=2",
        "-sFULL_ES3=1",
        "-sGL_ENABLE_GET_PROC_ADDRESS=1",
        // The page starts `main` itself, with the command line, once the files are in place;
        // the program stays alive after `main` leaves for the browser's frames.
        "-sINVOKE_RUN=0",
        "-sEXIT_RUNTIME=0",
        "-sENVIRONMENT=web,worker",
        "-sEXPORTED_RUNTIME_METHODS=callMain,FS,ENV",
        "-sASSERTIONS=1",
    ];
    for arg in ARGS {
        println!("cargo:rustc-link-arg-bin=horizon-client={arg}");
    }
}
