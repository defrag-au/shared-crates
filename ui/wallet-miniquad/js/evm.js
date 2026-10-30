// EVM wallet bridge — miniquad plugin exposing EIP-6963 discovery and the
// EIP-1193 provider surface to Rust. See ../src/evm.rs for the Rust side, and
// wallet.js for the Cardano plugin this mirrors: every call that waits returns
// an integer request id at once, and Rust polls `evm_poll(id)` each frame.
//
// Wallets are found through EIP-6963 announcements, never `window.ethereum`
// alone: several wallets can be installed at once, one of them owns the global,
// and the announcement is the only thing that carries the provider the reader
// actually picked — the same rule wallet-core's wasm-bindgen bridge follows.
//
// Surface: list, connect (prompting or silent), chain id, switch chain, icon,
// disconnect, events (accounts and chain changed), a recheck that reports
// like an event, cancel, poll.
//
// Plugin version 2: adds `evm_switch_chain`. Version 3: adds `evm_recheck`.

(function () {
    let nextReqId = 1;
    const pending = new Map();
    const icons = new Map();
    // uuid -> { info, provider }, from every announcement heard.
    const announced = new Map();
    // The provider of the wallet connected now.
    let current = null;
    // What the connected wallet reported since Rust last asked.
    let changes = { accounts: null, chain: null };

    function errMsg(e) {
        if (!e) return 'unknown error';
        if (typeof e === 'string') return e;
        if (e.message) return e.message;
        try { return JSON.stringify(e); } catch (_) { return String(e); }
    }

    function jsonObj(value) {
        return js_object(JSON.stringify(value));
    }

    // EIP-1193 4001 is the reader refusing; -32002 is a request already open
    // in the wallet, which is a fault to say plainly rather than a refusal.
    function failure_of(e) {
        const code = (e && typeof e === 'object') ? e.code : undefined;
        if (code === -32002) {
            return { kind: 'failed', message: 'The wallet already has a request open. Finish it there, then try again.' };
        }
        return { kind: code === 4001 ? 'declined' : 'failed', message: errMsg(e) };
    }

    function fail(id, message, kind) {
        pending.set(id, { status: 'err', data: message, kind: kind || 'failed' });
    }

    function remember(event) {
        const detail = event && event.detail;
        const info = detail && detail.info;
        if (!info || !info.uuid || !detail.provider) return;
        announced.set(info.uuid, { info, provider: detail.provider });
    }

    // Installed when the plugin loads, before the wasm: announcements made on
    // page load are heard, and the map outlives any one listing.
    if (typeof window !== 'undefined') {
        window.addEventListener('eip6963:announceProvider', remember);
        window.dispatchEvent(new Event('eip6963:requestProvider'));
    }

    function evm_list_wallets() {
        if (typeof window === 'undefined') return jsonObj([]);
        // Ask again: a wallet that loaded late announces on request.
        window.dispatchEvent(new Event('eip6963:requestProvider'));
        const wallets = [];
        for (const [uuid, entry] of announced.entries()) {
            wallets.push({ id: uuid, name: entry.info.name || uuid, rdns: entry.info.rdns || '' });
        }
        return jsonObj(wallets);
    }

    function watch(provider) {
        if (current === provider) return;
        current = provider;
        changes = { accounts: null, chain: null };
        if (typeof provider.on === 'function') {
            provider.on('accountsChanged', function (accounts) {
                if (current === provider) changes.accounts = Array.isArray(accounts) ? accounts : [];
            });
            provider.on('chainChanged', function (chain) {
                if (current === provider) changes.chain = String(chain);
            });
        }
    }

    // Connect wallet `id`. `silent` (1) asks `eth_accounts`, which never
    // prompts — the reconnect on load; otherwise `eth_requestAccounts`, which
    // asks the reader. On ok, `data` is the first account.
    function evm_connect(id_handle, silent) {
        const uuid = consume_js_object(id_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        const entry = announced.get(uuid);
        if (!entry) {
            fail(id, 'That wallet is no longer answering. Reload the page and try again.');
            return id;
        }
        const method = silent ? 'eth_accounts' : 'eth_requestAccounts';
        entry.provider.request({ method: method, params: [] })
            .then(function (accounts) {
                if (!pending.has(id)) return;
                const first = Array.isArray(accounts) && accounts.length > 0 ? String(accounts[0]) : '';
                if (!first) {
                    fail(id, silent ? 'not permitted' : 'The wallet shared no account.', silent ? 'declined' : 'failed');
                    return;
                }
                watch(entry.provider);
                pending.set(id, { status: 'ok', data: first });
            })
            .catch(function (err) {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    // `eth_chainId` of the connected wallet: `0x…` hex, as EIP-1193 gives it.
    function evm_chain_id() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!current) {
            fail(id, 'no wallet connected');
            return id;
        }
        current.request({ method: 'eth_chainId', params: [] })
            .then(function (chain) { pending.set(id, { status: 'ok', data: String(chain) }); })
            .catch(function (err) {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    // EIP-3326 4902, "unrecognised chain". MetaMask Mobile wraps it in a
    // -32603 whose `data.originalError` carries the real code.
    function isUnknownChain(e) {
        if (!e || typeof e !== 'object') return false;
        if (e.code === 4902) return true;
        const inner = e.data && e.data.originalError;
        return !!(inner && inner.code === 4902);
    }

    // Ask the connected wallet onto a chain (EIP-3326). A wallet that does not
    // know the chain is offered it to add (EIP-3085) when `add` is given —
    // the wallet switches to a chain it has just added. `request` is the JSON
    // `{ chainId: "0x…", add: { chainName, rpcUrls, nativeCurrency,
    // blockExplorerUrls } | null }`. On ok, `data` is the chain the wallet is
    // on afterwards, read back rather than assumed.
    function evm_switch_chain(request_handle) {
        const request = JSON.parse(consume_js_object(request_handle));
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!current) {
            fail(id, 'no wallet connected');
            return id;
        }
        const provider = current;
        const readBack = function () {
            return provider.request({ method: 'eth_chainId', params: [] }).then(function (chain) {
                pending.set(id, { status: 'ok', data: String(chain) });
            });
        };
        const failed = function (err) {
            const f = failure_of(err);
            fail(id, f.message, f.kind);
        };
        provider.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: request.chainId }] })
            .then(readBack)
            .catch(function (err) {
                if (!pending.has(id)) return;
                if (isUnknownChain(err) && request.add) {
                    const add = Object.assign({ chainId: request.chainId }, request.add);
                    provider.request({ method: 'wallet_addEthereumChain', params: [add] })
                        .then(readBack)
                        .catch(failed);
                    return;
                }
                failed(err);
            });
        return id;
    }

    // The icon an announcement carries, rasterised to RGBA — as wallet.js
    // does for CIP-30, so an SVG needs no decoder on the Rust side.
    function evm_icon(id_handle, size) {
        const uuid = consume_js_object(id_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        const entry = announced.get(uuid);
        const src = entry && entry.info.icon;
        if (typeof src !== 'string' || src.length === 0) {
            fail(id, 'no icon');
            return id;
        }
        const img = new Image();
        if (/^https?:/i.test(src)) img.crossOrigin = 'anonymous';
        img.onload = function () {
            if (!pending.has(id)) return;
            try {
                const canvas = document.createElement('canvas');
                canvas.width = size;
                canvas.height = size;
                const ctx = canvas.getContext('2d');
                const w = img.naturalWidth || size;
                const h = img.naturalHeight || size;
                const scale = Math.min(size / w, size / h);
                ctx.drawImage(img, (size - w * scale) / 2, (size - h * scale) / 2, w * scale, h * scale);
                const rgba = ctx.getImageData(0, 0, size, size).data;
                icons.set(id, new Uint8Array(rgba.buffer.slice(0)));
                pending.set(id, { status: 'ok', data: String(size) });
            } catch (e) {
                fail(id, 'icon could not be read: ' + errMsg(e));
            }
        };
        img.onerror = function () { if (pending.has(id)) fail(id, 'icon did not load'); };
        img.src = src;
        return id;
    }

    function evm_icon_pixels(req_id) {
        const px = icons.get(req_id);
        icons.delete(req_id);
        return js_object(px || new Uint8Array(0));
    }

    // Let go of the connected wallet. Where the wallet supports EIP-2255's
    // revoke, the site's permission goes too, so the next connect prompts;
    // where it does not, the grant stays in the wallet's own settings.
    function evm_disconnect() {
        const provider = current;
        current = null;
        changes = { accounts: null, chain: null };
        if (provider) {
            provider.request({ method: 'wallet_revokePermissions', params: [{ eth_accounts: {} }] })
                .catch(function () {});
        }
    }

    // What the connected wallet reported since the last call, then forgotten:
    // `{ accounts: [..] | null, chain: "0x.." | null }`.
    function evm_changes() {
        const out = changes;
        changes = { accounts: null, chain: null };
        return jsonObj(out);
    }

    // Ask the connected wallet what it holds now, and report the answer as if
    // the wallet had said so itself: the accounts and chain land in `changes`,
    // which Rust already compares with what it knew. For the page coming back
    // into view — a wallet's own events are not always delivered while a tab
    // is hidden, and some wallets never send them.
    function evm_recheck() {
        const provider = current;
        if (!provider) return;
        provider.request({ method: 'eth_accounts', params: [] })
            .then(function (accounts) {
                if (current === provider) changes.accounts = Array.isArray(accounts) ? accounts.map(String) : [];
            })
            .catch(function () {});
        provider.request({ method: 'eth_chainId', params: [] })
            .then(function (chain) {
                if (current === provider) changes.chain = String(chain);
            })
            .catch(function () {});
    }

    function evm_cancel(req_id) {
        pending.delete(req_id);
        icons.delete(req_id);
    }

    function evm_poll(req_id) {
        const r = pending.get(req_id);
        if (!r) return jsonObj({ status: 'err', data: 'unknown req ' + req_id, kind: 'failed' });
        if (r.status === 'pending') return jsonObj({ status: 'pending' });
        pending.delete(req_id);
        const out = { status: r.status, data: String(r.data) };
        if (r.status === 'err') out.kind = r.kind || 'failed';
        return jsonObj(out);
    }

    register_plugin = function (importObject) {
        importObject.env.evm_list_wallets = evm_list_wallets;
        importObject.env.evm_connect = evm_connect;
        importObject.env.evm_chain_id = evm_chain_id;
        importObject.env.evm_switch_chain = evm_switch_chain;
        importObject.env.evm_icon = evm_icon;
        importObject.env.evm_icon_pixels = evm_icon_pixels;
        importObject.env.evm_disconnect = evm_disconnect;
        importObject.env.evm_changes = evm_changes;
        importObject.env.evm_recheck = evm_recheck;
        importObject.env.evm_cancel = evm_cancel;
        importObject.env.evm_poll = evm_poll;
    };

    miniquad_add_plugin({ register_plugin, version: 3, name: 'evm' });
})();
