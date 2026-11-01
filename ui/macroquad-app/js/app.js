// macroquad-app: what an app (rather than a game) needs from the page, over
// the miniquad plugin bridge. Four services, each a few functions:
//
// - history:  the page's hash as the app's route, pushed and replaced, and a
//             count of the browser's back and forward moves, so the phone's
//             back gesture goes back inside the app instead of leaving it.
// - insets:   the safe area (a notch, a home indicator) and the height the
//             soft keyboard takes, in CSS pixels, which are the app's points.
// - input:    one real <input> laid over the canvas while a field is being
//             edited, so a phone shows its own keyboard, autocorrect, paste
//             and IME. Focused from inside the tap's own handler, because a
//             phone opens its keyboard only for a focus a gesture made.
// - http:     a request with any method, headers and a binary body, whose
//             reply keeps its status, polled by id like every other plugin.
//
// String and buffer arguments arrive as integer handles to sapp_jsutils
// objects: read them with `consume_js_object(handle)`.

(function () {
    function canvasEl() {
        return document.querySelector('#glcanvas') || document.querySelector('canvas');
    }

    // ── History ─────────────────────────────────────────────────────────────
    //
    // The route lives in the hash, so a reload, a shared link and the back
    // button all land on the same screen. `pops` counts the browser's own
    // moves (back, forward, a typed hash); the app compares it with the count
    // it last saw and re-reads the hash when it moved. Pushes and replaces
    // from the app do not count: the app already knows where it went.

    let pops = 0;
    window.addEventListener('popstate', function () { pops += 1; });
    window.addEventListener('hashchange', function () { pops += 1; });

    function app_hash() {
        const h = window.location.hash || '';
        return js_object(h.charAt(0) === '#' ? h.slice(1) : h);
    }

    function hashUrl(hash) {
        return window.location.pathname + window.location.search + '#' + hash;
    }

    function app_history_push(hash_js) {
        const hash = consume_js_object(hash_js);
        try {
            history.pushState(null, '', hashUrl(hash));
        } catch (e) {
            console.warn('app: could not push a route', e);
        }
    }

    function app_history_replace(hash_js) {
        const hash = consume_js_object(hash_js);
        try {
            history.replaceState(null, '', hashUrl(hash));
        } catch (e) {
            console.warn('app: could not replace the route', e);
        }
    }

    function app_history_back() {
        history.back();
    }

    function app_history_pops() {
        return pops;
    }

    // ── Insets ──────────────────────────────────────────────────────────────
    //
    // `env(safe-area-inset-*)` can only be read by CSS, so a probe element
    // carries them as padding and its computed style is read back. The shell
    // sets `viewport-fit=cover`, without which every inset is zero.
    //
    // The keyboard is what the visual viewport has lost at the bottom: a
    // phone shrinks the visual viewport, not the layout one, when its
    // keyboard opens.

    let probe = null;

    function ensureProbe() {
        if (probe) return probe;
        probe = document.createElement('div');
        probe.style.cssText = 'position:fixed;left:0;top:0;width:0;height:0;visibility:hidden;'
            + 'pointer-events:none;padding:env(safe-area-inset-top) env(safe-area-inset-right) '
            + 'env(safe-area-inset-bottom) env(safe-area-inset-left);';
        document.body.appendChild(probe);
        return probe;
    }

    // 0 top, 1 right, 2 bottom, 3 left, 4 the keyboard.
    function app_inset(side) {
        if (side === 4) {
            const vv = window.visualViewport;
            if (!vv) return 0;
            return Math.max(0, window.innerHeight - vv.height - vv.offsetTop);
        }
        const s = getComputedStyle(ensureProbe());
        const v = [s.paddingTop, s.paddingRight, s.paddingBottom, s.paddingLeft][side];
        return parseFloat(v) || 0;
    }

    // ── Input ───────────────────────────────────────────────────────────────
    //
    // Each frame the app names the fields it drew (`app_input_zone`, after
    // `app_input_zones_clear`), with each field's text box as fractions of
    // the canvas. A tap that ends in one shows the input over that box and
    // focuses it from this handler. While a field is being edited the input
    // follows its box every frame, so a page that scrolls carries it along;
    // a field the app stops naming loses the focus.
    //
    // While editing, the input owns the text: the value the app passes is
    // only taken when editing starts, or when the app sets it on purpose
    // (`app_input_set`, a clear button).

    let zones = new Map();        // id -> { box, value, placeholder, kind, font, color }
    let nextZones = new Map();
    let focused = -1;             // the id being edited, or -1
    let submits = 0;              // Enter presses, ever
    let input = null;

    function ensureInput() {
        if (input) return input;
        input = document.createElement('input');
        input.type = 'text';
        input.setAttribute('autocomplete', 'off');
        input.setAttribute('spellcheck', 'false');
        input.style.cssText = 'position:fixed;display:none;margin:0;padding:0;border:0;'
            + 'outline:none;background:transparent;z-index:10;box-sizing:border-box;'
            + 'font-family:system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;';
        // miniquad's page cancels every paste at the window, for its own
        // clipboard. Stopping the event here, before it bubbles there, lets
        // the input take what was pasted.
        input.addEventListener('paste', function (e) { e.stopPropagation(); });
        input.addEventListener('keydown', function (e) {
            if (e.key === 'Enter') {
                submits += 1;
                e.preventDefault();
                input.blur();
            } else if (e.key === 'Escape') {
                e.preventDefault();
                input.blur();
            }
        });
        input.addEventListener('blur', function () {
            focused = -1;
            input.style.display = 'none';
            const c = canvasEl();
            // Back to the canvas, so the app's own keys work again. A phone
            // does not reopen a keyboard for a canvas.
            if (c) c.focus({ preventScroll: true });
        });
        document.body.appendChild(input);
        return input;
    }

    function place(el, box) {
        const c = canvasEl();
        if (!c) return;
        const r = c.getBoundingClientRect();
        el.style.left = (r.left + box[0] * r.width) + 'px';
        el.style.top = (r.top + box[1] * r.height) + 'px';
        el.style.width = ((box[2] - box[0]) * r.width) + 'px';
        el.style.height = ((box[3] - box[1]) * r.height) + 'px';
    }

    function styleFor(el, z) {
        // A phone zooms the page into any field under 16px when it takes the
        // focus, and does not zoom back out.
        el.style.fontSize = Math.max(16, z.font) + 'px';
        el.style.color = z.color;
        el.style.caretColor = z.color;
        el.placeholder = z.placeholder;
        // 1: search (the keyboard's key says Search), 2: an address or an id
        // (no capitals, no autocorrect).
        el.setAttribute('enterkeyhint', z.kind === 1 ? 'search' : 'done');
        el.setAttribute('autocapitalize', z.kind === 2 ? 'off' : 'sentences');
        el.setAttribute('autocorrect', z.kind === 2 ? 'off' : 'on');
        el.inputMode = z.kind === 1 ? 'search' : 'text';
    }

    function startEditing(id) {
        const z = zones.get(id);
        if (!z) return;
        const el = ensureInput();
        styleFor(el, z);
        el.value = z.value;
        el.style.display = 'block';
        place(el, z.box);
        focused = id;
        el.focus({ preventScroll: true });
        try {
            el.setSelectionRange(el.value.length, el.value.length);
        } catch (e) { /* some input modes refuse a selection */ }
    }

    document.addEventListener('pointerup', function (e) {
        const c = canvasEl();
        if (!c || zones.size === 0) return;
        const r = c.getBoundingClientRect();
        const fx = (e.clientX - r.left) / r.width;
        const fy = (e.clientY - r.top) / r.height;
        for (const [id, z] of zones) {
            const b = z.box;
            if (fx >= b[0] && fx <= b[2] && fy >= b[1] && fy <= b[3]) {
                if (focused !== id) startEditing(id);
                return;
            }
        }
    }, { capture: true, passive: true });

    function app_input_zones_clear() {
        nextZones = new Map();
    }

    function app_input_zone(id, x0, y0, x1, y1, value_js, placeholder_js, kind, font, color_js) {
        const value = consume_js_object(value_js);
        const placeholder = consume_js_object(placeholder_js);
        const color = consume_js_object(color_js);
        if (x1 <= x0 || y1 <= y0) return;
        nextZones.set(id, { box: [x0, y0, x1, y1], value, placeholder, kind, font, color });
    }

    // The frame's zones are complete: swap them in, and keep the input on
    // its field, or let it go if the field is gone.
    function app_input_zones_done() {
        zones = nextZones;
        if (focused < 0 || !input) return;
        const z = zones.get(focused);
        if (!z) {
            input.blur();
            return;
        }
        place(input, z.box);
    }

    function app_input_focused() {
        return focused;
    }

    function app_input_value() {
        return js_object(input ? input.value : '');
    }

    function app_input_set(value_js) {
        const value = consume_js_object(value_js);
        if (input) input.value = value;
    }

    function app_input_submits() {
        return submits;
    }

    function app_input_blur() {
        if (input && focused >= 0) input.blur();
    }

    // ── HTTP ────────────────────────────────────────────────────────────────
    //
    // `headers` is "name\nvalue\n" pairs; `body` an empty buffer for none.
    // A reply of any status settles as done, with its status and bytes: a
    // 401 is an answer the app acts on, not a failure. Only no reply at all
    // (offline, DNS, a CORS refusal) settles as an error.

    const requests = new Map();   // id -> { state: 'pending' } | { state: 'done', status, body } | { state: 'err', message }
    let nextRequest = 1;

    function app_http(method_js, url_js, headers_js, body_js) {
        const id = nextRequest++;
        const method = consume_js_object(method_js);
        const url = consume_js_object(url_js);
        const lines = consume_js_object(headers_js).split('\n');
        const body = consume_js_object(body_js);
        const headers = {};
        for (let i = 0; i + 1 < lines.length; i += 2) {
            if (lines[i]) headers[lines[i]] = lines[i + 1];
        }
        requests.set(id, { state: 'pending' });
        const init = { method, headers };
        if (body && body.length > 0 && method !== 'GET' && method !== 'HEAD') init.body = body;
        fetch(url, init)
            .then(async function (r) {
                const bytes = new Uint8Array(await r.arrayBuffer());
                if (requests.has(id)) requests.set(id, { state: 'done', status: r.status, body: bytes });
            })
            .catch(function (e) {
                const message = e && e.message ? e.message : String(e);
                if (requests.has(id)) requests.set(id, { state: 'err', message });
            });
        return id;
    }

    // 0 pending, 1 done (read the status and the body), 2 failed (read the
    // message), 3 no such request.
    function app_http_state(id) {
        const r = requests.get(id);
        if (!r) return 3;
        return r.state === 'pending' ? 0 : r.state === 'done' ? 1 : 2;
    }

    function app_http_status(id) {
        const r = requests.get(id);
        return r && r.state === 'done' ? r.status : 0;
    }

    // The body, handed over once: the request is forgotten after it.
    function app_http_take(id) {
        const r = requests.get(id);
        requests.delete(id);
        return js_object(r && r.state === 'done' ? r.body : new Uint8Array(0));
    }

    function app_http_error(id) {
        const r = requests.get(id);
        requests.delete(id);
        return js_object(r && r.state === 'err' ? r.message : 'unknown request');
    }

    // A request the app dropped before it settled.
    function app_http_forget(id) {
        requests.delete(id);
    }

    function register_plugin(importObject) {
        importObject.env.app_hash = app_hash;
        importObject.env.app_history_push = app_history_push;
        importObject.env.app_history_replace = app_history_replace;
        importObject.env.app_history_back = app_history_back;
        importObject.env.app_history_pops = app_history_pops;
        importObject.env.app_inset = app_inset;
        importObject.env.app_input_zones_clear = app_input_zones_clear;
        importObject.env.app_input_zone = app_input_zone;
        importObject.env.app_input_zones_done = app_input_zones_done;
        importObject.env.app_input_focused = app_input_focused;
        importObject.env.app_input_value = app_input_value;
        importObject.env.app_input_set = app_input_set;
        importObject.env.app_input_submits = app_input_submits;
        importObject.env.app_input_blur = app_input_blur;
        importObject.env.app_http = app_http;
        importObject.env.app_http_state = app_http_state;
        importObject.env.app_http_status = app_http_status;
        importObject.env.app_http_take = app_http_take;
        importObject.env.app_http_error = app_http_error;
        importObject.env.app_http_forget = app_http_forget;
    }

    // Bumped with every change to the functions above, with the Rust export
    // beside it, so a page carrying an older copy fails at instantiate rather
    // than calling a function gl.js stubbed out.
    miniquad_add_plugin({ register_plugin, version: 1, name: 'app_shell' });
})();
