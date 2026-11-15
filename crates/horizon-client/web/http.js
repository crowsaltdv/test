// Emscripten JS library (build.rs, `--js-library`): the browser build's HTTP GET for the in-game
// mod browser (crates/horizon-system/src/web_http.rs). A synchronous request, so game threads can
// wait for it; it goes through the page's own `/mod-proxy`, which passes only the registry's
// addresses (web/mod-proxy.mjs), because GitHub's release downloads send no CORS headers.
addToLibrary({
  // The Mods page's upload button: the page (web/index.html) opens its file picker. Only the
  // page's own thread has the page; a click is the user activation a picker needs.
  horizon_upload_hmod: () => {
    try {
      if (typeof globalThis.horizonUploadHmod === 'function') globalThis.horizonUploadHmod();
    } catch (e) { /* the page isn't there */ }
  },
  horizon_http_get__deps: ['malloc'],
  horizon_http_get: (urlPtr, bodyOut, lengthOut) => {
    const url = UTF8ToString(urlPtr);
    HEAPU32[bodyOut >> 2] = 0;
    HEAPU32[lengthOut >> 2] = 0;
    try {
      const request = new XMLHttpRequest();
      request.open('GET', '/mod-proxy?url=' + encodeURIComponent(url), false);
      // Synchronous requests can't ask for an ArrayBuffer; read the bytes as a binary string.
      request.overrideMimeType('text/plain; charset=x-user-defined');
      request.send();
      const text = request.responseText || '';
      const length = text.length;
      if (length > 0) {
        const pointer = _malloc(length);
        if (!pointer) return -1;
        for (let i = 0; i < length; i++) HEAPU8[pointer + i] = text.charCodeAt(i) & 0xff;
        HEAPU32[bodyOut >> 2] = pointer;
        HEAPU32[lengthOut >> 2] = length;
      }
      return request.status || -1;
    } catch (e) {
      return -1;
    }
  },
});
