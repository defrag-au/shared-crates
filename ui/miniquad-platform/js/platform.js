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

    register_plugin = function (importObject) {
        importObject.env.platform_random_bytes = platform_random_bytes;
    };

    miniquad_add_plugin({ register_plugin, version: 1, name: 'platform' });
})();
