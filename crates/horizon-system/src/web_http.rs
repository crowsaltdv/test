//! HTTP GET in a browser (Emscripten, docs/web.md), for the code that has no socket to use: the
//! in-game mod browser. It is a synchronous request in `web/http.js`, linked into the page, so
//! it works on the game's own threads (a worker may block; the page's thread may not) and goes
//! through the page's `/mod-proxy`, which only passes the registry's addresses.
use std::ffi::{CString, c_char, c_void};

unsafe extern "C" {
    /// `web/http.js`: the status (or -1 for no answer), the body in `*body` (to `free`) and its
    /// length in `*length`.
    fn horizon_http_get(url: *const c_char, body: *mut *mut u8, length: *mut usize) -> i32;
    fn free(pointer: *mut c_void);
}

/// The body at `url`, at most `max` bytes.
pub fn get(url: &str, max: u64) -> Result<Vec<u8>, String> {
    let url = CString::new(url).map_err(|_| "the address has a NUL in it".to_owned())?;
    let (mut body, mut length) = (std::ptr::null_mut(), 0usize);
    // SAFETY: the address is NUL-terminated; the call fills both out values or leaves them null
    // and 0.
    let status = unsafe { horizon_http_get(url.as_ptr(), &mut body, &mut length) };
    let bytes = if body.is_null() {
        Vec::new()
    } else {
        // SAFETY: `body` holds `length` bytes of `malloc`'d memory, which is freed once here.
        let copy = unsafe { std::slice::from_raw_parts(body, length) }.to_vec();
        unsafe { free(body.cast()) };
        copy
    };
    match status {
        200 => {
            if bytes.len() as u64 > max {
                Err("the answer is larger than expected".to_owned())
            } else {
                Ok(bytes)
            }
        }
        -1 => Err("the mod registry can't be reached".to_owned()),
        code => Err(format!("the mod registry answered {code}")),
    }
}
