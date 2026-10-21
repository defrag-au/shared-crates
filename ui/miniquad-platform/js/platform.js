// Platform service plugin — browser-API surfaces that Rust crates need
// but which the miniquad WASM runtime doesn't provide out of the box.
//
// Currently exposes: the page's query string, a page reload,
// cryptographically-secure random bytes (backed by
// `crypto.getRandomValues()`), and small-value persistence over
// `localStorage`. Rust's `getrandom` crate routes through this plugin via
// `register_custom_getrandom!` so pallas-crypto + bip39 + everything else that
// wants a CSPRNG works on miniquad WASM without pulling in wasm-bindgen.
//
// The browser's `crypto.getRandomValues()` is the same primitive that
// `wasm-bindgen`'s `js-sys` path would call — equivalent security, just
// reached through miniquad's plugin protocol instead of wasm-bindgen's
// ABI.

(function () {
    function platform_random_bytes(len) {
        // 32 KiB is the per-call cap defined by Web Crypto. Plenty for
        // anything pallas/bip39 will ever ask for (typically 32 bytes
        // for seed material), but we batch defensively just in case.
        const out = new Uint8Array(len);
        let offset = 0;
        while (offset < len) {
            const chunk = Math.min(len - offset, 32768);
            const view = out.subarray(offset, offset + chunk);
            crypto.getRandomValues(view);
            offset += chunk;
        }
        return js_object(out);
    }

    // One parameter from the page's query string, or an empty string when
    // absent. Under miniquad there is no wasm-bindgen and therefore no
    // `web_sys::window()`, so a Rust crate that needs to read how the page was
    // launched — a handoff token, an environment switch — has no other route.
    //
    // Empty string rather than null: the bridge marshals a JsObject, and
    // "absent" and "present but empty" are the same non-answer to every caller
    // that has asked so far.
    function platform_query_param(key_js) {
        const key = consume_js_object(key_js);
        const value = new URLSearchParams(window.location.search).get(key);
        return js_object(value === null ? '' : value);
    }

    // Reload the page, query string and all: how a game rejoins after its
    // connection died while the tab was hidden or the phone locked.
    function platform_reload() {
        window.location.reload();
    }

    // ── Small-value persistence ─────────────────────────────────────────────
    //
    // `localStorage` is the only durable, synchronous store a page has, and
    // miniquad reaches no browser API without a plugin. Used for a handful of
    // preference-shaped values (which wallet the reader picked, a locale) — not
    // for anything that must not be readable by another script on the origin.
    //
    // Every call is wrapped: a private window, an embedder's sandbox, disabled
    // site data and a full quota all THROW out of `localStorage`, and none of
    // them is a reason for the page to fail. A miss reads as '' — the same
    // "absent" convention `platform_query_param` uses.

    function platform_storage_get(key_js) {
        const key = consume_js_object(key_js);
        try {
            const value = window.localStorage.getItem(key);
            return js_object(value === null ? '' : value);
        } catch (e) {
            console.warn('platform: localStorage is unavailable', e);
            return js_object('');
        }
    }

    function platform_storage_set(key_js, value_js) {
        const key = consume_js_object(key_js);
        const value = consume_js_object(value_js);
        try {
            window.localStorage.setItem(key, value);
        } catch (e) {
            console.warn('platform: could not persist ' + key, e);
        }
    }

    function platform_storage_remove(key_js) {
        const key = consume_js_object(key_js);
        try {
            window.localStorage.removeItem(key);
        } catch (e) {
            console.warn('platform: could not forget ' + key, e);
        }
    }

    register_plugin = function (importObject) {
        importObject.env.platform_random_bytes = platform_random_bytes;
        importObject.env.platform_query_param = platform_query_param;
        importObject.env.platform_reload = platform_reload;
        importObject.env.platform_storage_get = platform_storage_get;
        importObject.env.platform_storage_set = platform_storage_set;
        importObject.env.platform_storage_remove = platform_storage_remove;
    };

    // v2 added the storage calls. A consumer whose committed `platform.js`
    // predates it fails the handshake at instantiate rather than calling a
    // stubbed function — which is why the Rust export and this number move
    // together.
    miniquad_add_plugin({ register_plugin, version: 2, name: 'platform' });
})();
