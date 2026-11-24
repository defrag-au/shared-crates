// The page half of the offthread ABI, for miniquad.
//
// The wasm-bindgen client spawns its worker through `web_sys::Worker`; under
// miniquad there is no wasm-bindgen glue, so the same job is done here and
// reached over the plugin protocol. What crosses is BYTES both ways: this file
// knows nothing about the protocol inside them — the Rust half decodes,
// handshakes and correlates, exactly as `offthread::Client` does.
//
// Written next to the artifact by the consumer's build script, from
// `offthread::CLIENT_JS`, so it cannot drift from the Rust ABI it implements.
// Load it BEFORE `load(…wasm)`, like every other plugin.
//
// ⚠️ NO `__PLACEHOLDER__` substitution here, unlike the worker runtime. The
// worker's URL is the page's to name (`spawn(url)`), which is what lets one
// service artifact be spawned from any route a host serves.

(function () {
  // The Worker object exists, but something fatal happened to it: a module
  // that would not load, or a message this page could not read. The Rust half
  // turns this into a refusal and falls back to whatever route existed before
  // there was a worker, so it must be recorded rather than thrown.
  let failed = false;
  let why = "";

  let worker = null;
  // Messages that arrived between two frames. Drained by `try_recv`, one per
  // call, so a burst is read in order and none is lost to a frame boundary.
  const inbox = [];

  function fail(reason) {
    if (failed) return;
    failed = true;
    why = String(reason);
    console.error("offthread: " + why);
  }

  // Spawn the worker at `url`. Returns "" on success, or the reason it could
  // not be spawned — the Rust half reads that as `Link::Rejected`.
  //
  // `type: "module"` because the host runtime is written as one; a classic
  // worker would also run it, but a module is what the wasm-bindgen page
  // spawns and the two halves should not disagree about the artifact.
  function open(url_js) {
    if (worker !== null) return js_object("");
    const url = consume_js_object(url_js);

    try {
      worker = new Worker(url, { type: "module" });
    } catch (e) {
      fail(e && e.message ? e.message : e);
      return js_object(why);
    }

    // One line, so a page with no builder in its console is distinguishable
    // from one whose spawn threw before it could say anything.
    console.info("offthread: worker spawned at " + url);

    worker.onmessage = function (event) {
      const data = event.data;
      if (data instanceof ArrayBuffer) {
        inbox.push(new Uint8Array(data));
      } else if (ArrayBuffer.isView(data)) {
        // A view over a buffer this page may not own entirely: keep the
        // window it names rather than treating the whole buffer as the message.
        inbox.push(new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
      } else {
        fail("the worker posted something that is not bytes");
      }
    };

    // A module that 404s or is refused by the host reports HERE, not from the
    // constructor — which is why the Rust half has to ask again after spawn.
    worker.onerror = function (event) {
      fail(event && event.message ? event.message : "the worker failed to start");
    };
    worker.onmessageerror = function () {
      fail("a message from the worker could not be read");
    };

    return js_object("");
  }

  // Send one request. The JsObject comes from `JsObject::buffer`, a fresh
  // Uint8Array nobody else holds, so its buffer is transferred rather than
  // copied — the request can carry a 3.5 KB script.
  function post(bytes_js) {
    if (worker === null) return;
    const bytes = consume_js_object(bytes_js);
    worker.postMessage(bytes.buffer, [bytes.buffer]);
  }

  // The oldest message, or nil. -1 is sapp_jsutils' NULL sentinel, which is
  // what `JsObject::is_nil` tests — the same convention quad-net's socket and
  // HTTP polling use.
  function try_recv() {
    return inbox.length ? js_object(inbox.shift()) : -1;
  }

  function has_failed() {
    return failed ? 1 : 0;
  }

  function failure() {
    return js_object(why);
  }

  function close() {
    if (worker !== null) {
      worker.terminate();
      worker = null;
    }
  }

  register_plugin = function (importObject) {
    importObject.env.offthread_client_open = open;
    importObject.env.offthread_client_post = post;
    importObject.env.offthread_client_try_recv = try_recv;
    importObject.env.offthread_client_failed = has_failed;
    importObject.env.offthread_client_why = failure;
    importObject.env.offthread_client_close = close;
  };

  miniquad_add_plugin({ register_plugin, version: 1, name: "offthread" });
})();
