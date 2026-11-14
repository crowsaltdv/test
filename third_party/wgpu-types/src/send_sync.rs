// Horizon: under Emscripten the GPU's objects are numbers of a C library's OpenGL ES, as on
// any other system, not JavaScript objects bound to their thread: they are `Send` and `Sync`
// there too. wgpu, wgpu-core and wgpu-hal take the same from `--cfg send_sync`
// (`.cargo/config.toml`); see THIRD_PARTY_NOTICES.md.
pub trait WasmNotSendSync: WasmNotSend + WasmNotSync {}
impl<T: WasmNotSend + WasmNotSync> WasmNotSendSync for T {}
#[cfg(any(
    not(target_arch = "wasm32"),
    target_os = "emscripten",
    all(
        feature = "fragile-send-sync-non-atomic-wasm",
        not(target_feature = "atomics")
    )
))]
pub trait WasmNotSend: Send {}
#[cfg(any(
    not(target_arch = "wasm32"),
    target_os = "emscripten",
    all(
        feature = "fragile-send-sync-non-atomic-wasm",
        not(target_feature = "atomics")
    )
))]
impl<T: Send> WasmNotSend for T {}
#[cfg(not(any(
    not(target_arch = "wasm32"),
    target_os = "emscripten",
    all(
        feature = "fragile-send-sync-non-atomic-wasm",
        not(target_feature = "atomics")
    )
)))]
pub trait WasmNotSend {}
#[cfg(not(any(
    not(target_arch = "wasm32"),
    target_os = "emscripten",
    all(
        feature = "fragile-send-sync-non-atomic-wasm",
        not(target_feature = "atomics")
    )
)))]
impl<T> WasmNotSend for T {}

#[cfg(any(
    not(target_arch = "wasm32"),
    target_os = "emscripten",
    all(
        feature = "fragile-send-sync-non-atomic-wasm",
        not(target_feature = "atomics")
    )
))]
pub trait WasmNotSync: Sync {}
#[cfg(any(
    not(target_arch = "wasm32"),
    target_os = "emscripten",
    all(
        feature = "fragile-send-sync-non-atomic-wasm",
        not(target_feature = "atomics")
    )
))]
impl<T: Sync> WasmNotSync for T {}
#[cfg(not(any(
    not(target_arch = "wasm32"),
    target_os = "emscripten",
    all(
        feature = "fragile-send-sync-non-atomic-wasm",
        not(target_feature = "atomics")
    )
)))]
pub trait WasmNotSync {}
#[cfg(not(any(
    not(target_arch = "wasm32"),
    target_os = "emscripten",
    all(
        feature = "fragile-send-sync-non-atomic-wasm",
        not(target_feature = "atomics")
    )
)))]
impl<T> WasmNotSync for T {}
