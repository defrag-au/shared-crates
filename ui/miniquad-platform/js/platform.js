// Platform service plugin — browser-API surfaces that Rust crates need
// but which the miniquad WASM runtime doesn't provide out of the box.
//
// Currently exposes: the page's query string, host and path, a page reload,
// a fullscreen button the page itself answers, a count of the page's returns
// to focus, the text and image clipboards, cryptographically-secure random
// bytes (backed by `crypto.getRandomValues()`), and small-value persistence
// over `localStorage`. Rust's `getrandom` crate routes through this plugin via
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

    // The page's host (`buy.example.xyz`, no port) and path (`/aliens`): how a
    // page served under several names, or at several paths, learns which one
    // it was opened as. Same convention as the query: '' when there is none.
    function platform_location_host() {
        return js_object(window.location.hostname || '');
    }

    function platform_location_path() {
        return js_object(window.location.pathname || '');
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

    // ── Fullscreen ──────────────────────────────────────────────────────────
    //
    // A browser grants fullscreen only from inside a user gesture's handler,
    // and the wasm's frame loop is not one: miniquad's `set_fullscreen` is
    // refused on the web. So the game names where its fullscreen button is
    // (`platform_fullscreen_zone`, as fractions of the canvas) and this page
    // listens for the tap itself: one that ends inside the zone toggles
    // fullscreen from its own handler, where the browser allows it.
    //
    // `pointerup` is a gesture for a finger or pen; for a mouse, the
    // `mousedown` before it already was, and that lasts past the release.

    let fullscreenZone = null;

    function fullscreenElement() {
        return document.fullscreenElement || document.webkitFullscreenElement || null;
    }

    function fullscreenEnabled() {
        return !!(document.fullscreenEnabled || document.webkitFullscreenEnabled);
    }

    function toggleFullscreen() {
        try {
            const asked = fullscreenElement()
                ? (document.exitFullscreen || document.webkitExitFullscreen).call(document)
                : (function () {
                    const el = document.documentElement;
                    return (el.requestFullscreen || el.webkitRequestFullscreen)
                        .call(el, { navigationUI: 'hide' });
                })();
            if (asked && asked.catch) asked.catch(function () {});
        } catch (e) {
            console.warn('platform: fullscreen was refused', e);
        }
    }

    document.addEventListener('pointerup', function (e) {
        if (!fullscreenZone || !fullscreenEnabled()) return;
        const canvas = document.querySelector('#glcanvas') || document.querySelector('canvas');
        if (!canvas) return;
        const r = canvas.getBoundingClientRect();
        const fx = (e.clientX - r.left) / r.width;
        const fy = (e.clientY - r.top) / r.height;
        const z = fullscreenZone;
        if (fx >= z[0] && fx <= z[2] && fy >= z[1] && fy <= z[3]) toggleFullscreen();
    }, { capture: true, passive: true });

    // The button's box as fractions of the canvas, left, top, right, bottom;
    // a right edge not past the left clears it.
    function platform_fullscreen_zone(x0, y0, x1, y1) {
        fullscreenZone = x1 > x0 && y1 > y0 ? [x0, y0, x1, y1] : null;
    }

    // ── Links ───────────────────────────────────────────────────────────────
    //
    // Opening a new tab is a popup to the browser, allowed only from inside a
    // gesture's handler, like fullscreen. So the game names where a link is
    // drawn and where it goes (`platform_link_zone`), and a tap that ends in
    // the zone opens it from this handler. Only `https:` links: the zone is
    // the game's, the address may have come from anywhere.

    let linkZone = null;

    document.addEventListener('pointerup', function (e) {
        if (!linkZone) return;
        const canvas = document.querySelector('#glcanvas') || document.querySelector('canvas');
        if (!canvas) return;
        const r = canvas.getBoundingClientRect();
        const fx = (e.clientX - r.left) / r.width;
        const fy = (e.clientY - r.top) / r.height;
        const z = linkZone.box;
        if (fx >= z[0] && fx <= z[2] && fy >= z[1] && fy <= z[3]) {
            window.open(linkZone.url, '_blank', 'noopener,noreferrer');
        }
    }, { capture: true, passive: true });

    // The link's box as fractions of the canvas and its address; a right edge
    // not past the left, or an address that is not `https:`, clears it.
    function platform_link_zone(x0, y0, x1, y1, url_js) {
        const url = consume_js_object(url_js);
        linkZone = x1 > x0 && y1 > y0 && /^https:\/\//i.test(url)
            ? { box: [x0, y0, x1, y1], url: url }
            : null;
    }

    // 0: no fullscreen here (an iPhone's Safari, an embed that does not allow
    // it), 1: windowed, 2: fullscreen.
    function platform_fullscreen_state() {
        if (!fullscreenEnabled()) return 0;
        return fullscreenElement() ? 2 : 1;
    }

    // ── Clipboard ───────────────────────────────────────────────────────────
    //
    // miniquad's own clipboard on the web only answers the browser's copy
    // event (Ctrl+C); it cannot put text on the clipboard when a button is
    // tapped. `navigator.clipboard.writeText` can, inside the transient
    // activation a tap grants — which the next frame is still within. Where it
    // is missing or refused (an insecure origin, an older browser), the
    // hidden-textarea `execCommand('copy')` is the fallback.

    function copyByCommand(text) {
        try {
            const area = document.createElement('textarea');
            area.value = text;
            area.setAttribute('readonly', '');
            area.style.position = 'fixed';
            area.style.opacity = '0';
            document.body.appendChild(area);
            area.select();
            const ok = document.execCommand('copy');
            document.body.removeChild(area);
            if (!ok) console.warn('platform: copy was refused');
        } catch (e) {
            console.warn('platform: copy is unavailable', e);
        }
    }

    function platform_clipboard_write(text_js) {
        const text = consume_js_object(text_js);
        if (navigator.clipboard && navigator.clipboard.writeText) {
            navigator.clipboard.writeText(text).catch(function () { copyByCommand(text); });
        } else {
            copyByCommand(text);
        }
    }

    // ── Image clipboard ─────────────────────────────────────────────────────
    //
    // miniquad's clipboard carries text; an image needs
    // `navigator.clipboard.write` with a `ClipboardItem`. Two rules shape this:
    //
    //   * the write must START inside the tap's frame, because that is where
    //     the browser's activation is. A fetch before the write would spend it.
    //   * the image usually arrives after the tap. So the item is handed a
    //     Promise<Blob> — the ClipboardItem constructor takes one for exactly
    //     this — and `platform_clipboard_prepare_image` warms that promise when
    //     a caller shows the image, so a tap normally resolves at once.
    //
    // Every image is converted to PNG: ClipboardItem takes only a handful of
    // types, and a service may answer WebP or AVIF.

    // url -> Promise<Blob>, oldest first so the cap drops the least recently
    // asked for. A screen shows a dozen images; the cap is for a long scroll.
    const images = new Map();
    const IMAGE_CAP = 48;

    // Copy requests: id -> 'pending' | 'ok' | 'failed'.
    const imageCopies = new Map();
    let nextImageCopy = 1;

    function imageToPng(blob) {
        if (blob.type === 'image/png') return Promise.resolve(blob);
        return createImageBitmap(blob).then(function (bitmap) {
            const canvas = document.createElement('canvas');
            canvas.width = bitmap.width;
            canvas.height = bitmap.height;
            canvas.getContext('2d').drawImage(bitmap, 0, 0);
            // Free the decoded bitmap now rather than at GC time.
            bitmap.close();
            return new Promise(function (resolve, reject) {
                canvas.toBlob(function (png) {
                    if (png) resolve(png);
                    else reject(new Error('PNG encode failed'));
                }, 'image/png');
            });
        });
    }

    async function fetchImagePng(url) {
        const res = await fetch(url);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return imageToPng(await res.blob());
    }

    // The PNG for `url`, fetched once. A rejection is not cached: the next tap
    // tries again rather than failing for the life of the page.
    function imagePng(url) {
        let pending = images.get(url);
        if (pending) {
            images.delete(url);
            images.set(url, pending);
            return pending;
        }
        pending = fetchImagePng(url);
        images.set(url, pending);
        pending.catch(function () {
            if (images.get(url) === pending) images.delete(url);
        });
        while (images.size > IMAGE_CAP) {
            images.delete(images.keys().next().value);
        }
        return pending;
    }

    function platform_clipboard_prepare_image(url_js) {
        // A failure is the copy's to report; warming the cache is only a head
        // start, and the promise's own catch above keeps it from surfacing.
        imagePng(consume_js_object(url_js));
    }

    function platform_clipboard_copy_image(url_js) {
        const id = nextImageCopy++;
        const url = consume_js_object(url_js);
        imageCopies.set(id, 'pending');
        if (!navigator.clipboard || typeof ClipboardItem === 'undefined') {
            imageCopies.set(id, 'failed');
            return id;
        }
        try {
            // Called now, inside the tap's activation. The item carries the
            // promise, so the write waits for the image rather than the image
            // waiting for the write.
            navigator.clipboard
                .write([new ClipboardItem({ 'image/png': imagePng(url) })])
                .then(function () {
                    imageCopies.set(id, 'ok');
                })
                .catch(function (e) {
                    imageCopies.set(id, 'failed');
                    console.warn('platform: the browser refused the image copy', e);
                });
        } catch (e) {
            imageCopies.set(id, 'failed');
            console.warn('platform: the browser refused the image copy', e);
        }
        return id;
    }

    // 0 pending, 1 copied, 2 failed. An unknown id is a failed copy: it is
    // never a reason to spin.
    function platform_clipboard_image_poll(id) {
        const state = imageCopies.get(id);
        if (state === 'ok' || state === 'failed') imageCopies.delete(id);
        if (state === 'ok') return 1;
        return state === 'pending' ? 0 : 2;
    }

    // Each time the page comes back to the visitor: the tab is shown again or
    // the window takes focus again (a tab switch usually fires both, and a
    // wallet extension's popup closing fires the second). The game compares
    // the count with the one it last saw, so a double count is harmless and
    // nothing is missed between frames.
    let focusReturns = 0;
    document.addEventListener('visibilitychange', function () {
        if (document.visibilityState === 'visible') focusReturns += 1;
    });
    window.addEventListener('focus', function () {
        focusReturns += 1;
    });

    function platform_focus_returns() {
        return focusReturns;
    }

    register_plugin = function (importObject) {
        importObject.env.platform_focus_returns = platform_focus_returns;
        importObject.env.platform_link_zone = platform_link_zone;
        importObject.env.platform_clipboard_write = platform_clipboard_write;
        importObject.env.platform_clipboard_prepare_image = platform_clipboard_prepare_image;
        importObject.env.platform_clipboard_copy_image = platform_clipboard_copy_image;
        importObject.env.platform_clipboard_image_poll = platform_clipboard_image_poll;
        importObject.env.platform_fullscreen_zone = platform_fullscreen_zone;
        importObject.env.platform_fullscreen_state = platform_fullscreen_state;
        importObject.env.platform_random_bytes = platform_random_bytes;
        importObject.env.platform_query_param = platform_query_param;
        importObject.env.platform_location_host = platform_location_host;
        importObject.env.platform_location_path = platform_location_path;
        importObject.env.platform_reload = platform_reload;
        importObject.env.platform_storage_get = platform_storage_get;
        importObject.env.platform_storage_set = platform_storage_set;
        importObject.env.platform_storage_remove = platform_storage_remove;
    };

    // v2 added the storage calls, v3 the page's host and path, v4 fullscreen,
    // v5 the text clipboard, v6 the focus count, v7 link zones, v8 the image
    // clipboard. A consumer whose committed `platform.js` predates it fails
    // the handshake at instantiate rather than calling a stubbed function —
    // which is why the Rust export and this number move together.
    miniquad_add_plugin({ register_plugin, version: 8, name: 'platform' });
})();
