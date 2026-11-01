// Midnight wallet bridge — miniquad plugin exposing the DApp connector's
// discovery, unshielded address and `signData` to Rust. See ../src/midnight.rs
// for the Rust side, and evm.js for the plugin this mirrors: every call that
// waits returns an integer request id at once, and Rust polls
// `midnight_poll(id)` each frame.
//
// Wallets are found on `window.midnight`, an object the first wallet to install
// it locks down, keyed by a UUIDv4 the wallet generates and holding an
// `InitialAPI` — `{ rdns, name, icon, apiVersion, connect }`. That is the whole
// of the discovery story: unlike EIP-6963 there is no announcement event, and
// unlike CIP-30 there is no well-known `window.cardano.<name>`.
//
// Source: https://github.com/midnightntwrk/midnight-dapp-connector-api — the
// `InitialAPI`/`ConnectedAPI` types in `src/api.ts`, and the "Initial API"
// section of `SPECIFICATION.md`.
//
// Surface: list, connect (mainnet, then the unshielded address), sign a text
// message, disconnect, cancel, poll.
//
// Plugin version 1.

(function () {
    let nextReqId = 1;
    const pending = new Map();
    // The connected API, once a wallet has been connected.
    let connected = null;

    // The network this bridge asks for. `mainnet` is the id the connector
    // specification defines ("The network id of mainnet is `mainnet`"; api.ts's
    // `connect` says the same), and the link is stored as `midnight:mainnet`,
    // so a wallet that can only offer another network cannot be linked here.
    const NETWORK = 'mainnet';

    // The oldest API major this bridge speaks. Every method used below
    // (`connect`, `getConnectionStatus`, `getUnshieldedAddress`, `signData`)
    // has been in every published version, so the range is deliberately open at
    // the top: a wallet that over-reports simply fails at the call. A `0.x` is a
    // pre-release of an API that was never published that way, and the
    // specification requires the DApp to refuse an unsupported version rather
    // than try it.
    const MIN_API_MAJOR = 1;

    function errMsg(e) {
        if (!e) return 'unknown error';
        if (typeof e === 'string') return e;
        const body = e.reason || e.message;
        // `APIError` carries `code` beside the sentence; it is the part that
        // survives a wallet rewording its message, so it travels with it.
        const code = e.code ? String(e.code) : '';
        if (body) return code && !String(body).includes(code) ? code + ': ' + body : String(body);
        try { return JSON.stringify(e); } catch (_) { return String(e); }
    }

    // What a wallet threw, in full. A connector error is `Error & { type, code,
    // reason }`; the extension's own plumbing can also throw a bare object or a
    // string, and the shape is what says which.
    function describe(e) {
        if (!e || typeof e !== 'object') return { thrown: String(e) };
        return {
            name: e.name,
            type: e.type,
            code: e.code,
            reason: e.reason,
            message: e.message,
            keys: Object.keys(e),
            stack: typeof e.stack === 'string'
                ? e.stack.split('\n').slice(0, 4).join('\n')
                : undefined,
        };
    }

    // Every step on one prefix, at debug level. A wallet that fails inside its
    // own messaging layer logs once, in its own console, with no detail — so
    // the bridge says what it asked for and what came back, and the point of
    // failure is one line away instead of a guess.
    function log(...args) {
        console.debug('[midnight]', ...args);
    }

    function jsonObj(value) {
        return js_object(JSON.stringify(value));
    }

    // A connector error carries `code`, one of the APIError codes. `Rejected`
    // is this one request refused; `PermissionRejected` is the reader refusing
    // this part of the API for the session. Both are the reader's answer, not a
    // fault, so both read as a decline.
    function failure_of(e) {
        const code = (e && typeof e === 'object') ? e.code : undefined;
        const declined = code === 'Rejected' || code === 'PermissionRejected';
        return { kind: declined ? 'declined' : 'failed', message: errMsg(e) };
    }

    function fail(id, message, kind) {
        pending.set(id, { status: 'err', data: message, kind: kind || 'failed' });
    }

    // Whether asking again could plausibly work. A reader's own answer could
    // not — a retry would put a second dialog in front of someone who said no —
    // and neither could a request the wallet says is already open.
    function worth_retrying(err) {
        const code = (err && typeof err === 'object') ? err.code : undefined;
        if (code === 'Rejected' || code === 'PermissionRejected') return false;
        if (/already (has a request|pending)|already open/i.test(errMsg(err))) return false;
        return true;
    }

    // Run `step`, and if it fails in a way that could be transient, run it once
    // more after a beat.
    //
    // WHY: an MV3 wallet's service worker sleeps, and the first message that
    // reaches its content script after the page settles has nothing to forward
    // to — the extension logs "Error forwarding message" in its own console and
    // the DApp sees a rejection, with no approval dialog ever shown. The next
    // message wakes it. Without this, the reader has to tap "Try again" to get
    // the link moving, for a fault that is entirely the extension's.
    //
    // The retry is safe because a request that failed to forward never reached
    // the wallet's UI: there is no dialog to duplicate. The guard is the clock —
    // nobody reads and approves a wallet dialog in under 300 ms, so a failure
    // that arrives faster than that is plumbing, not a decision, whatever the
    // wallet calls it.
    const RETRY_DELAY_MS = 400;
    const FAST_FAILURE_MS = 300;

    function with_retry(id, what, step, ok, failed) {
        let attempt = 0;
        function go() {
            // Abandoned by the host while we waited: nothing to answer.
            if (!pending.has(id)) return;
            attempt += 1;
            const at = Date.now();
            let started;
            try {
                started = step();
            } catch (err) {
                handle(err, true, at);
                return;
            }
            Promise.resolve(started).then(ok).catch(function (err) { handle(err, false, at); });
        }
        function handle(err, sync, at) {
            const quick = Date.now() - at < FAST_FAILURE_MS;
            if (attempt < 2 && quick && worth_retrying(err)) {
                log(what + ': failed in ' + (Date.now() - at) + ' ms, retrying once', {
                    attempt: attempt,
                    sync: sync,
                    detail: describe(err),
                });
                setTimeout(go, RETRY_DELAY_MS);
                return;
            }
            log(what + ': failed', describe(err));
            failed(err);
        }
        go();
    }

    // Whether `apiVersion` is a version this bridge can use. `undefined` is
    // refused: the field is required by the `InitialAPI` type.
    function compatible(apiVersion) {
        const match = /^(\d+)\./.exec(String(apiVersion == null ? '' : apiVersion).trim());
        return !!match && Number(match[1]) >= MIN_API_MAJOR;
    }

    function installed() {
        if (typeof window === 'undefined') return [];
        const api = window.midnight;
        if (!api || typeof api !== 'object') return [];
        const out = [];
        for (const id of Object.keys(api)) {
            const wallet = api[id];
            if (!wallet || typeof wallet.connect !== 'function') continue;
            out.push({ id: id, wallet: wallet });
        }
        return out;
    }

    // Every installed wallet this bridge can use. A wallet whose API version is
    // outside the supported range is left out, as the specification requires,
    // and logged so the omission is not a silent mystery.
    function midnight_list_wallets() {
        const wallets = [];
        for (const entry of installed()) {
            if (!compatible(entry.wallet.apiVersion)) {
                console.warn(
                    'midnight: ignoring ' + (entry.wallet.name || entry.id) +
                    ', unsupported apiVersion ' + entry.wallet.apiVersion
                );
                continue;
            }
            wallets.push({
                id: entry.id,
                name: String(entry.wallet.name || entry.id),
                rdns: String(entry.wallet.rdns || ''),
                apiVersion: String(entry.wallet.apiVersion || ''),
            });
        }
        return jsonObj(wallets);
    }

    // Connect wallet `id`. Asks for mainnet, then reads back the connection's
    // own status — a wallet that connected to another network is refused here
    // rather than trusted — and then the unshielded address, which is the
    // account a link is stored against. On ok, `data` is that address.
    function midnight_connect(id_handle) {
        const uuid = consume_js_object(id_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });

        const entry = installed().find(function (e) { return e.id === uuid; });
        if (!entry) {
            fail(id, 'That wallet is no longer answering. Reload the page and try again.');
            return id;
        }

        log('connect', { wallet: entry.wallet.name || uuid, rdns: entry.wallet.rdns, apiVersion: entry.wallet.apiVersion, network: NETWORK });

        // One `step` so a retry re-runs the whole exchange — connect, then the
        // status read-back, then the address. A throw, sync or async, lands in
        // `with_retry` either way: a wallet's injection layer throws
        // synchronously when its own messaging fails, and unwrapped that used
        // to leave the request pending forever.
        with_retry(id, 'connect', function () {
            return Promise.resolve(entry.wallet.connect(NETWORK)).then(function (api) {
                if (!api || typeof api.getUnshieldedAddress !== 'function') {
                    throw new Error('the wallet did not return a connected API');
                }
                return api.getConnectionStatus().then(function (status) {
                    const on = status && status.networkId;
                    log('connected', { status: status && status.status, networkId: on });
                    if (!status || status.status !== 'connected' || on !== NETWORK) {
                        throw new Error(
                            'The wallet connected to ' + String(on || 'an unknown network') +
                            ', and a Midnight link needs ' + NETWORK + '.'
                        );
                    }
                    return api.getUnshieldedAddress().then(function (answer) {
                        const address = answer && answer.unshieldedAddress;
                        if (!address) throw new Error('The wallet shared no unshielded address.');
                        return { api: api, address: String(address) };
                    });
                });
            });
        }, function (result) {
            if (!pending.has(id)) return;
            connected = result.api;
            log('account', result.address);
            pending.set(id, { status: 'ok', data: result.address });
        }, function (err) {
            if (!pending.has(id)) return;
            const f = failure_of(err);
            fail(id, f.message, f.kind);
        });
        return id;
    }

    // Sign the text `request.message` for `request.address`. The wallet prepends
    // `midnight_signed_message:<byte length>:` itself and hashes the result, so
    // the data goes over as text (`encoding: "text"`, UTF-8) and the Rust side
    // must not pre-hash or hex it. On ok, `data` is the wallet's `Signature` as
    // JSON: `{ signature, verifyingKey, scheme? }`, where `scheme` is absent when
    // the wallet did not name one — which means `schnorr_bip340`.
    function midnight_sign_message(request_handle) {
        const request = JSON.parse(consume_js_object(request_handle));
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!connected) {
            log('signData: no connected wallet');
            fail(id, 'no wallet connected');
            return id;
        }
        const message = String(request.message);
        const bytes = new TextEncoder().encode(message).length;
        log('signData', { address: request.address, bytes: bytes, encoding: 'text', keyType: 'unshielded' });

        // Same retry as `connect`, and for the same reason: a message the
        // extension failed to forward never reached the wallet's approval UI, so
        // asking again cannot duplicate a dialog.
        with_retry(id, 'signData', function () {
            return Promise.resolve(connected.signData(message, {
                encoding: 'text',
                keyType: 'unshielded',
            }));
        }, function (signature) {
            if (!pending.has(id)) return;
            log('signed', {
                scheme: signature && signature.scheme,
                signatureBytes: signature && signature.signature
                    ? String(signature.signature).length / 2
                    : 0,
                verifyingKeyChars: signature && signature.verifyingKey
                    ? String(signature.verifyingKey).length
                    : 0,
                dataMatches: !signature || signature.data == null || signature.data === message,
            });
            if (!signature || !signature.signature || !signature.verifyingKey) {
                fail(id, 'The wallet returned an incomplete signature.');
                return;
            }
            pending.set(id, {
                status: 'ok',
                data: JSON.stringify({
                    signature: String(signature.signature),
                    verifyingKey: String(signature.verifyingKey),
                    scheme: signature.scheme == null ? null : String(signature.scheme),
                }),
            });
        }, function (err) {
            if (!pending.has(id)) return;
            const f = failure_of(err);
            fail(id, f.message, f.kind);
        });
        return id;
    }

    // Let go of the connected API. The connector has no revoke — the wallet's
    // own settings are where a grant is withdrawn — so this only forgets it
    // here, and the next connect asks the wallet again.
    function midnight_disconnect() {
        log('disconnect');
        connected = null;
    }

    function midnight_cancel(req_id) {
        pending.delete(req_id);
    }

    function midnight_poll(req_id) {
        const r = pending.get(req_id);
        if (!r) return jsonObj({ status: 'err', data: 'unknown req ' + req_id, kind: 'failed' });
        if (r.status === 'pending') return jsonObj({ status: 'pending' });
        pending.delete(req_id);
        const out = { status: r.status, data: String(r.data) };
        if (r.status === 'err') out.kind = r.kind || 'failed';
        return jsonObj(out);
    }

    register_plugin = function (importObject) {
        importObject.env.midnight_list_wallets = midnight_list_wallets;
        importObject.env.midnight_connect = midnight_connect;
        importObject.env.midnight_sign_message = midnight_sign_message;
        importObject.env.midnight_disconnect = midnight_disconnect;
        importObject.env.midnight_cancel = midnight_cancel;
        importObject.env.midnight_poll = midnight_poll;
    };

    miniquad_add_plugin({ register_plugin, version: 1, name: 'midnight' });
})();
