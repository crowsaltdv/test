//! The allocator in a browser (Emscripten, docs/web.md): Emscripten's own malloc, which the
//! link picks (`-sMALLOC=mimalloc`, its build of mimalloc with a heap per thread). The two
//! mimalloc crates build their own copy from C for the host system, so they aren't
//! dependencies there; these modules carry the names the rest of this crate uses of them.

pub mod mimalloc {
    /// Rust's system allocator: Emscripten's `malloc` and `free`.
    pub use std::alloc::System as MiMalloc;
}

/// The calls into mimalloc's own interface. Emscripten's build keeps its options and
/// statistics to itself, so these change and report nothing.
#[allow(non_camel_case_types, dead_code, clippy::missing_safety_doc)]
pub mod libmimalloc_sys {
    use std::ffi::{c_char, c_long, c_void};

    pub type mi_option_t = i32;

    pub unsafe fn mi_option_set(_option: mi_option_t, _value: c_long) {}

    pub unsafe fn mi_option_get(_option: mi_option_t) -> c_long {
        0
    }

    pub unsafe fn mi_collect(_force: bool) {}

    /// Leaves its eight counters as the caller set them.
    pub unsafe fn mi_process_info(
        _elapsed_msecs: *mut usize,
        _user_msecs: *mut usize,
        _system_msecs: *mut usize,
        _current_rss: *mut usize,
        _peak_rss: *mut usize,
        _current_commit: *mut usize,
        _peak_commit: *mut usize,
        _page_faults: *mut usize,
    ) {
    }

    /// No statistics: a null string, as mimalloc returns when it can't allocate one.
    pub unsafe fn mi_stats_get_json(_size: usize, _buffer: *mut c_char) -> *mut c_char {
        std::ptr::null_mut()
    }

    pub unsafe fn mi_malloc(size: usize) -> *mut c_void {
        // SAFETY: the caller frees it with `mi_free`.
        unsafe { libc_malloc(size) }
    }

    pub unsafe fn mi_free(block: *mut c_void) {
        // SAFETY: a block of `mi_malloc`, or null.
        unsafe { libc_free(block) }
    }

    unsafe extern "C" {
        #[link_name = "malloc"]
        fn libc_malloc(size: usize) -> *mut c_void;
        #[link_name = "free"]
        fn libc_free(block: *mut c_void);
    }
}
