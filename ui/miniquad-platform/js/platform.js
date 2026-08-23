// Platform service plugin — browser-API surfaces that Rust crates need
// but which the miniquad WASM runtime doesn't provide out of the box.
//
// Currently exposes: cryptographically-secure random bytes, backed by
// `crypto.getRandomValues()`. Rust's `getrandom` crate routes through
// this plugin via `register_custom_getrandom!` so pallas-crypto + bip39
// + everything else that wants a CSPRNG works on miniquad WASM without
// pulling in wasm-bindgen.
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

    register_plugin = function (importObject) {
        importObject.env.platform_random_bytes = platform_random_bytes;
        importObject.env.platform_query_param = platform_query_param;
    };

    miniquad_add_plugin({ register_plugin, version: 1, name: 'platform' });
})();
