// discord-auth's miniquad plugin: the few browser calls a session needs,
// for a macroquad game, whose wasm has no wasm-bindgen and so no web-sys.
// The Rust side is `discord_auth::miniquad`; the logic it serves is
// `discord_auth::session`, shared with the web-sys edge.
//
// Strings cross as sapp_jsutils objects: read with `consume_js_object`,
// returned with `js_object`. An absent value is an empty string, which the
// Rust side reads as `None`.
//
// Load it before the wasm, like every miniquad plugin.

(function () {
    function discord_auth_fragment() {
        return js_object(window.location.hash || '');
    }

    // Drop the fragment from the address bar without a reload, keeping the
    // path and the query (a game's `?world=` must survive a login).
    function discord_auth_clear_fragment() {
        const url = window.location.pathname + window.location.search;
        try {
            history.replaceState(null, '', url);
        } catch (e) {}
    }

    // Storage can throw (a private window, storage disabled): then nothing
    // is kept, and the player logs in again next time.
    function discord_auth_get(key_js) {
        const key = consume_js_object(key_js);
        let value = null;
        try {
            value = window.localStorage.getItem(key);
        } catch (e) {}
        return js_object(value === null ? '' : value);
    }

    function discord_auth_set(key_js, value_js) {
        const key = consume_js_object(key_js);
        const value = consume_js_object(value_js);
        try {
            window.localStorage.setItem(key, value);
        } catch (e) {}
    }

    function discord_auth_remove(key_js) {
        const key = consume_js_object(key_js);
        try {
            window.localStorage.removeItem(key);
        } catch (e) {}
    }

    function discord_auth_navigate(url_js) {
        window.location.assign(consume_js_object(url_js));
    }

    function discord_auth_now_ms() {
        return Date.now();
    }

    register_plugin = function (importObject) {
        importObject.env.discord_auth_fragment = discord_auth_fragment;
        importObject.env.discord_auth_clear_fragment = discord_auth_clear_fragment;
        importObject.env.discord_auth_get = discord_auth_get;
        importObject.env.discord_auth_set = discord_auth_set;
        importObject.env.discord_auth_remove = discord_auth_remove;
        importObject.env.discord_auth_navigate = discord_auth_navigate;
        importObject.env.discord_auth_now_ms = discord_auth_now_ms;
    };

    miniquad_add_plugin({ register_plugin, version: 1, name: 'discord_auth' });
})();
