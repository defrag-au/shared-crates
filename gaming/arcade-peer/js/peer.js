// A WebRTC peer for the P2P overlay — one connection, two data channels.
//
// This is the boundary between the game and the browser's WebRTC stack, and it
// is deliberately dumb: it moves bytes and reports counters. It never parses a
// game message, never decides who to talk to, and never touches the sim. The
// schema lives in Rust (`island-wire`); who to talk to is the room's answer
// (`Group` in the overlay design).
//
// Two channels, because the overlay needs two behaviours from one connection:
//
//   ctrl   reliable, ordered   acks, group epoch, keyframe requests
//   state  unreliable, ordered:false, maxRetransmits:0   soft positions
//
// The split is the point. A reliable channel stalls everything behind a lost
// packet, which is exactly why the confirmed stream runs a 150 ms buffer; a
// position update that never arrives is worth nothing at all, and waiting for
// a retransmit is worse than dropping it.
//
// ## Driving it
//
// The spike drives this from `browser-shot`, which passes SDP between two
// browser processes through `eval … capture=` and `${name}` interpolation, so
// no signalling path is needed to test the channel itself:
//
//   eval a "window.__btOffer()" await capture=offer
//   eval b "window.__btAnswer(${offer})" await capture=answer
//   eval a "window.__btAcceptAnswer(${answer})" await
//   wait a,b predicate="window.__bt.state === 'open'"
//
// Both `__btOffer` and `__btAnswer` resolve to a **JSON string**, and the caller
// parses it:
//
//   eval b "window.__btAnswer(JSON.parse(${offer}))" await capture=answer
//
// The string is not a style choice. `capture=` substitutes the captured value
// JSON-encoded, so a string arrives quoted and an object arrives as the literal
// `"Object"`. Passing the description as a string and parsing on arrival is the
// only shape that survives the round trip.
//
// Gathering is non-trickle — they wait for `icegatheringstate === 'complete'` —
// because a trickle path needs a signalling channel to carry candidates, and
// this file must not depend on one yet.
//
// Nothing here runs until `__btOffer` or `__btAnswer` is called, so a page that
// never uses the overlay pays only the parse.

(function () {
  'use strict';

  var ICE_SERVERS = [];

  var peer = null;
  var ctrl = null;
  var state = null;

  window.__bt = {
    // 'new' | 'connecting' | 'open' | 'closed' | 'failed'
    state: 'new',
    sent: 0,
    received: 0,
    // What the state channel had buffered when the last sample was taken.
    // The overlay's whole claim is that a thin buffer does not stall, so this
    // is the number that shows it.
    buffered: 0,
    // Why the connection failed, for a report that would otherwise say only
    // "failed".
    error: null,
    // The `state` channel's generation, so a receiver can tell a fresh channel
    // from a stale one.
    epoch: 0,
  };

  function sample() {
    if (typeof window.__btSample !== 'function') {
      return;
    }
    window.__btSample({
      t: performance.timeOrigin + performance.now(),
      received: window.__bt.received,
      sent: window.__bt.sent,
      buffered: state ? state.bufferedAmount : 0,
    });
  }

  function note(kind, detail) {
    if (kind === 'error') {
      window.__bt.error = detail;
      window.__bt.state = 'failed';
      console.error('[peer] ' + detail);
    }
  }

  function wire(channel, name) {
    channel.binaryType = 'arraybuffer';
    channel.onopen = function () {
      if (name === 'state') {
        window.__bt.epoch += 1;
      }
      if (ctrl && ctrl.readyState === 'open' && state && state.readyState === 'open') {
        window.__bt.state = 'open';
      }
    };
    channel.onclose = function () {
      if (name === 'ctrl') {
        window.__bt.state = 'closed';
      }
    };
    channel.onerror = function (e) {
      note('error', name + ' channel: ' + (e && e.message ? e.message : 'error'));
    };
    if (name === 'state') {
      channel.onmessage = function () {
        window.__bt.received += 1;
        window.__bt.buffered = state ? state.bufferedAmount : 0;
        if (window.__bt.received % 20 === 0) {
          sample();
        }
      };
    }
    return channel;
  }

  function makePeer() {
    if (peer) {
      return peer;
    }
    var pc = new RTCPeerConnection({ iceServers: ICE_SERVERS });
    pc.oniceconnectionstatechange = function () {
      if (pc.iceConnectionState === 'failed') {
        note('error', 'ICE failed');
      }
    };
    pc.onconnectionstatechange = function () {
      // Data channels can open before `connectionState` reports `connected`,
      // so this must never walk the reported state backwards: a run that has
      // already seen messages was, by definition, open.
      if (window.__bt.state === 'open') {
        return;
      }
      if (pc.connectionState === 'connected') {
        window.__bt.state = 'connecting';
      } else if (pc.connectionState === 'failed') {
        note('error', 'connection failed');
      } else if (pc.connectionState === 'closed' || pc.connectionState === 'disconnected') {
        window.__bt.state = pc.connectionState;
      }
    };
    // The answering side receives the channels the offering side made.
    pc.ondatachannel = function (event) {
      if (event.channel.label === 'ctrl') {
        ctrl = wire(event.channel, 'ctrl');
      } else if (event.channel.label === 'state') {
        state = wire(event.channel, 'state');
      }
    };
    peer = pc;
    return pc;
  }

  function gathered(pc) {
    if (pc.iceGatheringState === 'complete') {
      return Promise.resolve();
    }
    return new Promise(function (resolve, reject) {
      var timer = setTimeout(function () {
        reject(new Error('ICE gathering did not finish within 15 s'));
      }, 15000);
      pc.addEventListener('icegatheringstatechange', function () {
        if (pc.iceGatheringState === 'complete') {
          clearTimeout(timer);
          resolve();
        }
      });
    });
  }

  // Offer, with the two channels the overlay needs already open on the wire.
  window.__btOffer = function () {
    var pc = makePeer();
    ctrl = wire(pc.createDataChannel('ctrl', { ordered: true }), 'ctrl');
    state = wire(
      pc.createDataChannel('state', { ordered: false, maxRetransmits: 0 }),
      'state'
    );
    window.__bt.state = 'connecting';
    return pc
      .createOffer()
      .then(function (offer) {
        return pc.setLocalDescription(offer);
      })
      .then(function () {
        return gathered(pc);
      })
      .then(function () {
        return JSON.stringify({
          type: pc.localDescription.type,
          sdp: pc.localDescription.sdp,
        });
      });
  };

  // Answer an offer and resolve with this side's description, for the offerer.
  window.__btAnswer = function (offer) {
    var pc = makePeer();
    return pc
      .setRemoteDescription(offer)
      .then(function () {
        return pc.createAnswer();
      })
      .then(function (answer) {
        return pc.setLocalDescription(answer);
      })
      .then(function () {
        return gathered(pc);
      })
      .then(function () {
        window.__bt.state = 'connecting';
        return JSON.stringify({
          type: pc.localDescription.type,
          sdp: pc.localDescription.sdp,
        });
      });
  };

  // The offerer's last step: take the answerer's description.
  window.__btAcceptAnswer = function (answer) {
    return makePeer()
      .setRemoteDescription(answer)
      .then(function () {
        return null;
      });
  };

  // Send `n` state messages, so a receiver has something to count and a
  // report has something to compare against what arrived.
  window.__btSend = function (n) {
    if (!state || state.readyState !== 'open') {
      return Promise.reject(new Error('state channel is not open'));
    }
    var payload = new Uint8Array(16);
    for (var i = 0; i < n; i += 1) {
      payload[0] = i & 0xff;
      payload[1] = (i >> 8) & 0xff;
      state.send(payload);
      window.__bt.sent += 1;
    }
    return Promise.resolve(window.__bt.sent);
  };

  // Close both channels and the connection, so a run leaves nothing behind.
  window.__btClose = function () {
    if (ctrl) {
      ctrl.close();
    }
    if (state) {
      state.close();
    }
    if (peer) {
      peer.close();
    }
    window.__bt.state = 'closed';
    return Promise.resolve(true);
  };
})();
