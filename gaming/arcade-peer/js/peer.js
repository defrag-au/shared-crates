// A WebRTC peer plane for an arcade room — one connection per peer, two data
// channels, and the signalling that gets them there.
//
// This is the boundary between the game and the browser's WebRTC stack. It
// moves bytes and reports counters. It never parses a game message, never
// decides who to talk to on its own account, and never touches the sim. The
// schema lives in Rust (`island-wire`); who is in the room arrives as signals.
//
// ## Driving it
//
// The room's socket is Rust's and the `RTCPeerConnection` is this file's, so
// every signal crosses the seam. `probe.js` on the game side owns that crossing
// (`probe_peer_in` / `probe_peer_out`); this file never talks to wasm.
//
// Once the room seats a player, Rust calls `__btJoin(seat)`. From then on the
// shim announces, offers or answers, and queues signals for Rust to send. It is
// pulled, not pushed: nothing here runs on a timer, and `__btDrain` (called by
// `probe_peer_out` once a frame) is where inbound signals are processed. A
// frame boundary is the right moment — Rust pumps the room once a frame anyway.
//
// ## Two channels, one connection
//
//   ctrl   reliable, ordered          acks, epoch, keyframe requests
//   state  unreliable, unordered, maxRetransmits: 0   soft positions
//
// The split is the point. A reliable channel stalls everything behind a lost
// packet, which is exactly why the confirmed stream runs a 150 ms buffer; a
// position update that never arrives is worth nothing at all, and waiting for
// a retransmit is worse than dropping it.
//
// ## Who offers
//
// The lower seat offers: `leadsOffer(me, them) = me < them`, which is
// `arcade-peer`'s `leads_offer`. Deterministic from two ids, no round trip, and
// exactly one end of a pair is true. It is treated as an optimisation rather
// than a guarantee — `acceptTheirOffer` lets the higher seat roll its own offer
// back and take the lower seat's — because during a seat change the two ends
// can briefly disagree. `rtc-quickconnect` does the same, and says why: the
// master's offer is "only really for the sake of politeness".
//
// ## Announcing
//
// `Announce` is broadcast, and the room stamps the sender. A peer that joined
// first cannot have been heard by one that joined later, so the first announce
// from a new seat is answered with one of ours. `seen` makes that terminate:
// without it the two ends answer each other for ever.
//
// ## Testing without a room
//
// `__btPump` and `__btFeed` (in `probe.js`) expose what the room would carry,
// so a browser-shot scenario can relay between two pages and exercise this
// file's negotiation end to end. They are harness seams, not game API.

(function () {
  'use strict';

  // ── The protocol, mirrored from `arcade-peer` ────────────────────────────
  //
  // `Signal`'s byte layout and the caps its decoder enforces:
  //
  //   tag    u8
  //   epoch  u16 LE
  //   to     u16 LE   — every message but Announce: which of the sender's
  //                     links this is for
  //   body   Announce  — nothing
  //          Offer     — u16 LE length, then UTF-8 SDP
  //          Answer    — the same
  //          Candidate — u16 LE length, then UTF-8 JSON of RTCIceCandidateInit
  //          Bye       — u8 reason
  //
  // `to` is here because the room broadcasts: with three peers, C's answer to
  // A also reaches B, and B has a link to C too. Without a target both would
  // apply it to their own link. `arcade-peer`'s `the_layout_is_fixed` is the
  // authority on these bytes.
  //
  // The browser cannot read a Rust constant, so these are written twice. The
  // crate's `the_browser_half_agrees_with_the_protocol` test fails when they
  // drift from it.
  var TAG_ANNOUNCE = 1;
  var TAG_OFFER = 2;
  var TAG_ANSWER = 3;
  var TAG_CANDIDATE = 4;
  var TAG_BYE = 5;

  var MAX_SDP = 8192;
  var MAX_CANDIDATE = 512;

  var BYE_REPLACED = 1;

  // Bounded interest sets are the design's rule, not this file's to invent. But
  // until the room sends `Group`, every seated peer announces and every announce
  // would be answered: at 64 players that is a full mesh, which is explicitly
  // out of scope. The cap is the shim's half of "bounded" — it bounds
  // connections, not traffic, and it is replaced when `Group` lands.
  var MAX_LINKS = 12;

  // ── ICE ──────────────────────────────────────────────────────────────────
  //
  // Two STUN servers, no TURN. STUN is how a peer learns its own address on the
  // far side of its NAT; without it a peer offers host candidates only, and a
  // browser disguises those as mDNS `.local` names, so two peers on different
  // networks never find a pair. TURN is the relay of last resort and it needs
  // credentials this file cannot hold: see `docs/P2P_OVERLAY_DESIGN.md` §9.
  //
  // The pair is measured rather than remembered. `freeice`'s `ice-check`
  // (2026-10-25, Sydney) answered in 6 ms from Cloudflare and 18 ms from
  // Google, while the European hosts that list also ships took 200–350 ms.
  // Both of these are anycast, so the figure is a distance to the nearest colo
  // rather than to a continent. Google's five `stun1..4` names all resolve to
  // the one address, which is why one of them is here: more entries buy more
  // candidates to gather, not more redundancy.
  var DEFAULT_ICE = [
    { urls: 'stun:stun.cloudflare.com:3478' },
    { urls: 'stun:stun.l.google.com:19302' },
  ];

  // `?ice=off` for a run that must not depend on the network; `?ice=stun:a:1,…`
  // to point a test at named servers. Absent, the default above applies: this
  // game is played across the internet.
  var ICE = iceFromQuery(window.location.search);

  function iceFromQuery(search) {
    var match = /[?&]ice=([^&#]*)/.exec(search || '');
    if (!match) {
      return { servers: DEFAULT_ICE, source: 'default' };
    }
    var value = decodeURIComponent(match[1]).trim();
    if (value === 'off' || value === 'none') {
      return { servers: [], source: 'off' };
    }
    var urls = value.split(',').map(function (part) {
      return part.trim();
    }).filter(Boolean);
    return {
      servers: urls.map(function (url) {
        return { urls: url };
      }),
      source: 'query',
    };
  }

  function iceUrls(servers) {
    return servers.map(function (server) {
      return Array.isArray(server.urls) ? server.urls.join(' ') : server.urls;
    });
  }

  // How long a probe waits for gathering before deciding no STUN server
  // answered. Chrome gives up on an unreachable server in about two seconds.
  var ICE_PROBE_MS = 3000;

  // Which channel a link byte belongs to. Mirrors the game's `overlay::Kind`.
  var LINK_STATE = 0;
  var LINK_CTRL = 1;

  // How many arrived bytes to hold for the game. A queue that grows means the
  // game stopped draining, which is a stall; dropping the oldest is right
  // because a position's value decays, and a keyframe comes every second.
  var MAX_RECV = 256;

  var recv = [];

  var encoder = new TextEncoder();
  var decoder = new TextDecoder();

  // My seat, null until the room seats me; bumped on every join so a re-seat
  // is a new session to everyone who knew the old one. Null, not 0: seat 0 is
  // a real seat — the first one a room hands out — so 0 cannot mean "unseated".
  var me = null;
  var sessionEpoch = 0;

  // seat -> link, and the seats we have heard an announce from.
  var links = new Map();
  var seen = new Set();

  window.__bt = {
    // 'new' | 'connecting' | 'open' | 'closed' | 'failed'. A roll-up over the
    // links rather than a value any one browser event can walk backwards: a
    // channel that is open was, by definition, open.
    state: 'new',
    // -1 until the room seats us. Not 0: seat 0 is the first seat a room
    // hands out, so 0 has to mean a seat like any other.
    me: -1,
    peers: 0,
    open: 0,
    sent: 0,
    received: 0,
    // The game's bytes, by channel: what went out on `state` and `ctrl`, and
    // what arrived on `ctrl`. `received` above is the state channel, which is
    // the one the overlay's samples come from.
    sentState: 0,
    sentCtrl: 0,
    receivedCtrl: 0,
    // What the state channel had buffered when the last sample was taken.
    // The overlay's whole claim is that a thin buffer does not stall, so this
    // is the number that shows it.
    buffered: 0,
    // Why the last connection failed, for a report that would otherwise say
    // only "failed".
    error: null,
    // Pairwise signals that reached us addressed to another peer. They are the
    // cost of broadcasting, and a count of them says the filter is working.
    ignored: 0,
    // What each link is doing, refreshed by `rollup`, so a report has more than
    // a count when a connection does not hold.
    links: [],
    // A bounded ring of link events — who offered, who answered, what closed —
    // because "state: closed" is not a diagnosis. Read it with
    // `eval "<peer>" "window.__bt.events" out=events.json`.
    events: [],
    // What ICE was configured with: `source` is 'default' | 'query' | 'off', and
    // `urls` is the list, so a report says which server list a run used rather
    // than assuming it.
    ice: { source: ICE.source, urls: iceUrls(ICE.servers) },
    // Every candidate gathered, by type. `host` is this machine and would be
    // gathered with no server configured at all; `srflx` is the one that proves
    // a STUN server answered.
    candidates: { host: 0, srflx: 0, prflx: 0, relay: 0 },
    // The address a STUN server reported for us, once one has. Null is the
    // difference between "ICE is configured" and "ICE answered".
    mapped: null,
  };

  // The ring's bound: enough for a few negotiations, small enough that a long
  // session cannot grow without limit.
  var MAX_EVENTS = 64;

  function note(seat, what) {
    window.__bt.events.push({ t: Math.round(performance.now()), seat: seat, what: what });
    if (window.__bt.events.length > MAX_EVENTS) {
      window.__bt.events.shift();
    }
  }

  function describe(e) {
    return e && e.message ? e.message : String(e);
  }

  function candidateType(candidate) {
    if (candidate.type) {
      return candidate.type;
    }
    var match = / typ ([a-z]+)/.exec(candidate.candidate || '');
    return match ? match[1] : 'other';
  }

  function candidateAddress(candidate) {
    var host = candidate.address || candidate.ip;
    return host ? host + ':' + candidate.port : null;
  }

  function countCandidate(candidate) {
    var type = candidateType(candidate);
    var counts = window.__bt.candidates;
    counts[type] = (counts[type] || 0) + 1;
    if (type === 'srflx' && !window.__bt.mapped) {
      window.__bt.mapped = candidateAddress(candidate);
    }
  }

  function sample() {
    if (typeof window.__btSample !== 'function') {
      return;
    }
    window.__btSample({
      t: performance.timeOrigin + performance.now(),
      received: window.__bt.received,
      sent: window.__bt.sent,
      peers: window.__bt.peers,
      buffered: window.__bt.buffered,
    });
  }

  function fail(link, why) {
    window.__bt.error = why;
    console.error('[peer] seat ' + link.seat + ': ' + why);
    rollup();
  }

  // ── Encoding and decoding ────────────────────────────────────────────────

  function frame(tag, epoch, to, rest) {
    var out = new Uint8Array(3 + (to === null ? 0 : 2) + rest.length);
    out[0] = tag;
    out[1] = epoch & 0xff;
    out[2] = (epoch >> 8) & 0xff;
    var at = 3;
    if (to !== null) {
      out[at] = to & 0xff;
      out[at + 1] = (to >> 8) & 0xff;
      at += 2;
    }
    out.set(rest, at);
    return out;
  }

  // A length-prefixed body. No escaping, which is the point: SDP is full of
  // `\r\n` and JSON would double every one of them.
  function sendBlob(tag, link, text, cap) {
    var body = encoder.encode(text);
    if (body.length > cap) {
      // Refused rather than truncated: a cut body decodes as something else
      // entirely on the far side, or not at all.
      window.__bt.error = 'a ' + body.length + '-byte body exceeds ' + cap;
      console.warn('[peer] ' + window.__bt.error + '; not sent');
      return;
    }
    var framed = new Uint8Array(2 + body.length);
    framed[0] = body.length & 0xff;
    framed[1] = (body.length >> 8) & 0xff;
    framed.set(body, 2);
    window.__btQueueSignal(frame(tag, link.myEpoch, link.seat, framed));
  }

  function sendBye(seat, epoch, reason) {
    window.__btQueueSignal(frame(TAG_BYE, epoch, seat, new Uint8Array([reason])));
  }

  // Strict, like `Signal::decode`: a body that does not finish exactly where
  // its lengths say it does is refused rather than partly believed.
  function decode(bytes) {
    if (bytes.length < 3) {
      return null;
    }
    var tag = bytes[0];
    var epoch = bytes[1] | (bytes[2] << 8);
    var at = 3;

    function u16() {
      if (at + 2 > bytes.length) {
        return null;
      }
      var v = bytes[at] | (bytes[at + 1] << 8);
      at += 2;
      return v;
    }

    function blob(cap) {
      if (at + 2 > bytes.length) {
        return null;
      }
      var len = bytes[at] | (bytes[at + 1] << 8);
      at += 2;
      if (len > cap || at + len > bytes.length) {
        return null;
      }
      var body = decoder.decode(bytes.subarray(at, at + len));
      at += len;
      return body;
    }

    var msg = { tag: tag, epoch: epoch };
    if (tag === TAG_ANNOUNCE) {
      // Broadcast: no target, no body.
    } else {
      msg.to = u16();
      if (msg.to === null) {
        return null;
      }
      if (tag === TAG_OFFER || tag === TAG_ANSWER) {
        msg.sdp = blob(MAX_SDP);
        if (msg.sdp === null) {
          return null;
        }
      } else if (tag === TAG_CANDIDATE) {
        msg.candidate = blob(MAX_CANDIDATE);
        if (msg.candidate === null) {
          return null;
        }
      } else if (tag === TAG_BYE) {
        if (at + 1 > bytes.length) {
          return null;
        }
        msg.reason = bytes[at];
        at += 1;
      } else {
        return null;
      }
    }
    return at === bytes.length ? msg : null;
  }

  // ── The link ─────────────────────────────────────────────────────────────

  function linkFor(seat) {
    var link = links.get(seat);
    if (link) {
      return link;
    }
    if (links.size >= MAX_LINKS) {
      return null;
    }
    link = {
      seat: seat,
      pc: null,
      ctrl: null,
      state: null,
      // My attempt at this pair, and theirs. Each only moves forward, and only
      // within its own direction: two peers' counters mean nothing to each
      // other, which is why they are tracked separately.
      myEpoch: sessionEpoch,
      theirEpoch: 0,
      // I am the offerer in this negotiation, so a competing offer is mine to
      // judge. Cleared when I answer instead.
      offerer: false,
      remoteSet: false,
      pending: [],
    };
    links.set(seat, link);
    return link;
  }

  function isOpen(link) {
    return (
      link.ctrl !== null &&
      link.ctrl.readyState === 'open' &&
      link.state !== null &&
      link.state.readyState === 'open'
    );
  }

  function rollup() {
    var open = 0;
    var live = 0;
    var detail = [];
    links.forEach(function (link) {
      if (isOpen(link)) {
        open += 1;
      }
      if (link.pc !== null) {
        live += 1;
      }
      detail.push({
        seat: link.seat,
        pc: link.pc === null ? 'none' : link.pc.connectionState,
        ctrl: link.ctrl === null ? 'none' : link.ctrl.readyState,
        state: link.state === null ? 'none' : link.state.readyState,
        offerer: link.offerer,
        epoch: link.myEpoch,
        theirEpoch: link.theirEpoch,
      });
    });
    window.__bt.links = detail;
    window.__bt.open = open;
    window.__bt.peers = links.size;
    if (open > 0) {
      window.__bt.state = 'open';
    } else if (live > 0) {
      window.__bt.state = 'connecting';
    } else if (window.__bt.state === 'open' || window.__bt.state === 'connecting') {
      window.__bt.state = 'closed';
    }
  }

  // Tear the browser half of a link down but keep the record: the epoch, and
  // who this is, survive a renegotiation.
  function reset(link, why) {
    note(link.seat, 'reset: ' + why);
    if (link.ctrl) {
      try {
        link.ctrl.close();
      } catch (e) {
        // Closing an already-closed channel is not a failure.
      }
      link.ctrl = null;
    }
    if (link.state) {
      try {
        link.state.close();
      } catch (e) {
        // As above.
      }
      link.state = null;
    }
    if (link.pc) {
      try {
        link.pc.close();
      } catch (e) {
        // As above.
      }
      link.pc = null;
    }
    link.offerer = false;
    link.remoteSet = false;
    link.pending = [];
    rollup();
  }

  function wire(channel, link, name) {
    channel.binaryType = 'arraybuffer';
    channel.onopen = function () {
      note(link.seat, name + ' open');
      rollup();
    };
    channel.onclose = function () {
      note(link.seat, name + ' closed');
      rollup();
    };
    channel.onerror = function (e) {
      fail(link, name + ' channel: ' + describe(e));
    };
    channel.onmessage = function (event) {
      if (name === 'state') {
        window.__bt.received += 1;
        window.__bt.buffered = channel.bufferedAmount;
        if (window.__bt.received % 20 === 0) {
          sample();
        }
      } else {
        window.__bt.receivedCtrl += 1;
      }
      // The game's bytes. This file does not look at them: `island-wire` owns
      // the format, and all the shim knows is which seat and which channel.
      recv.push({
        seat: link.seat,
        kind: name === 'state' ? LINK_STATE : LINK_CTRL,
        bytes: new Uint8Array(event.data),
      });
      if (recv.length > MAX_RECV) {
        recv.shift();
      }
    };
    return channel;
  }

  function ensurePc(link) {
    if (link.pc) {
      return link.pc;
    }
    var pc = new RTCPeerConnection({ iceServers: ICE.servers });
    link.pc = pc;
    link.remoteSet = false;
    link.pending = [];

    // Trickle. Waiting for gathering to complete would add a round of
    // hundreds of milliseconds to every join, and the protocol carries
    // candidates precisely so it does not have to.
    pc.onicecandidate = function (e) {
      if (!e.candidate) {
        return; // Gathering finished; there is no "end" message to send.
      }
      countCandidate(e.candidate);
      // `toJSON`, not `.candidate`: the init needs its mid and m-line index to
      // be added again, and only the init has them.
      sendBlob(TAG_CANDIDATE, link, JSON.stringify(e.candidate.toJSON()), MAX_CANDIDATE);
    };
    pc.oniceconnectionstatechange = function () {
      if (pc.iceConnectionState === 'failed') {
        fail(link, 'ICE failed');
      }
    };
    pc.onconnectionstatechange = function () {
      note(link.seat, 'connection ' + pc.connectionState);
      if (pc.connectionState === 'failed') {
        fail(link, 'connection failed');
      }
      rollup();
    };
    // The answering side receives the channels the offering side made.
    pc.ondatachannel = function (event) {
      if (event.channel.label === 'ctrl') {
        link.ctrl = wire(event.channel, link, 'ctrl');
      } else if (event.channel.label === 'state') {
        link.state = wire(event.channel, link, 'state');
      }
    };
    return pc;
  }

  function flush(link) {
    var pending = link.pending;
    link.pending = [];
    for (var i = 0; i < pending.length; i += 1) {
      addCandidate(link, pending[i]);
    }
  }

  function addCandidate(link, candidate) {
    link.pc.addIceCandidate(candidate).catch(function (e) {
      // A candidate that cannot be added is not fatal: the SDP it was
      // gathered beside may still connect.
      console.warn('[peer] seat ' + link.seat + ' candidate: ' + describe(e));
    });
  }

  // ── Negotiation ──────────────────────────────────────────────────────────

  function announce() {
    window.__btQueueSignal(frame(TAG_ANNOUNCE, sessionEpoch, null, new Uint8Array(0)));
  }

  function leadsOffer(them) {
    return me < them;
  }

  function acceptTheirOffer(them) {
    return them < me;
  }

  function startOffer(link) {
    if (link.offerer || isOpen(link)) {
      return;
    }
    var pc = ensurePc(link);
    link.offerer = true;
    note(link.seat, 'offering');
    link.ctrl = wire(pc.createDataChannel('ctrl', { ordered: true }), link, 'ctrl');
    link.state = wire(
      pc.createDataChannel('state', { ordered: false, maxRetransmits: 0 }),
      link,
      'state'
    );
    pc.createOffer()
      .then(function (offer) {
        return pc.setLocalDescription(offer);
      })
      .then(function () {
        sendBlob(TAG_OFFER, link, pc.localDescription.sdp, MAX_SDP);
      })
      .catch(function (e) {
        fail(link, 'offer: ' + describe(e));
      });
  }

  function onAnnounce(link) {
    if (!seen.has(link.seat)) {
      seen.add(link.seat);
      announce();
    }
    if (leadsOffer(link.seat)) {
      startOffer(link);
    }
  }

  function onOffer(link, msg) {
    if (isOpen(link)) {
      // A duplicate offer under a working connection. Renegotiation is a
      // higher epoch, which tore the link down before we got here.
      return;
    }
    if (link.offerer && !acceptTheirOffer(link.seat)) {
      // Glare, and ours wins: they will drop theirs when ours arrives.
      note(link.seat, 'offer ignored: ours leads');
      return;
    }
    reset(link, 'taking their offer');
    var pc = ensurePc(link);
    note(link.seat, 'answering ' + msg.sdp.length + 'B sdp');
    pc.setRemoteDescription({ type: 'offer', sdp: msg.sdp })
      .then(function () {
        link.remoteSet = true;
        flush(link);
        return pc.createAnswer();
      })
      .then(function (answer) {
        return pc.setLocalDescription(answer);
      })
      .then(function () {
        sendBlob(TAG_ANSWER, link, pc.localDescription.sdp, MAX_SDP);
      })
      .catch(function (e) {
        fail(link, 'answer: ' + describe(e));
      });
  }

  function onAnswer(link, msg) {
    if (!link.pc || !link.offerer) {
      note(link.seat, 'answer ignored: not the offerer');
      return;
    }
    note(link.seat, 'answer taken ' + msg.sdp.length + 'B sdp');
    link.pc
      .setRemoteDescription({ type: 'answer', sdp: msg.sdp })
      .then(function () {
        link.remoteSet = true;
        flush(link);
      })
      .catch(function (e) {
        fail(link, 'answer: ' + describe(e));
      });
  }

  function onCandidate(link, msg) {
    var candidate;
    try {
      candidate = JSON.parse(msg.candidate);
    } catch (e) {
      console.warn('[peer] seat ' + link.seat + ' sent an unreadable candidate');
      return;
    }
    if (link.pc && link.remoteSet) {
      addCandidate(link, candidate);
    } else {
      // Candidates can arrive before the description they belong to — legal
      // for the sender to send, and an error to add. Hold them.
      link.pending.push(candidate);
    }
  }

  function onBye(link) {
    reset(link, 'they said bye');
    links.delete(link.seat);
    seen.delete(link.seat);
    rollup();
  }

  function deliver(from, raw) {
    if (me === null || from === me) {
      return;
    }
    var bytes = raw instanceof Uint8Array ? raw : Uint8Array.from(raw);
    var msg = decode(bytes);
    if (msg === null) {
      console.warn('[peer] a signal from seat ' + from + ' did not decode');
      return;
    }
    if (msg.tag !== TAG_ANNOUNCE && msg.to !== me) {
      // Broadcast delivers every peer's pairwise signal to everyone. Only the
      // one it names acts on it — otherwise, with three peers, B would apply
      // C's answer to A's offer on its own link to C.
      window.__bt.ignored += 1;
      return;
    }
    var link = links.get(from);
    if (!link) {
      if (msg.tag === TAG_ANNOUNCE) {
        // A seat we have not met. At the cap it is ignored rather than told:
        // a `Bye` answered by a `Bye` from a peer that is also at its cap
        // ping-pongs, and an announce is broadcast, so it is not a request.
        if (links.size >= MAX_LINKS) {
          return;
        }
      } else if (msg.tag === TAG_OFFER) {
        // They are negotiating with a seat we have no link for, so offer them
        // the courtesy of a refusal rather than a half-open connection. A
        // `Bye` is not answered with a `Bye`, which is what stops this looping.
        if (links.size >= MAX_LINKS) {
          sendBye(from, msg.epoch, BYE_REPLACED);
          return;
        }
      } else {
        // An answer, candidate or bye for a pair we are not in: another
        // receiver's copy of a broadcast, or a race after a bye.
        window.__bt.ignored += 1;
        return;
      }
      link = linkFor(from);
      if (link === null) {
        return;
      }
    }
    if (msg.epoch < link.theirEpoch) {
      // A message from a negotiation that has already been replaced.
      return;
    }
    if (msg.epoch > link.theirEpoch) {
      // They restarted. Everything we held for them describes the old attempt.
      // The first message from a peer is not a restart — there is nothing to
      // tear down, and resetting anyway would only fill the event ring.
      var first = link.theirEpoch === 0;
      link.theirEpoch = msg.epoch;
      if (!first) {
        reset(link, 'their epoch moved to ' + msg.epoch);
      }
    }
    if (msg.tag === TAG_ANNOUNCE) {
      onAnnounce(link);
    } else if (msg.tag === TAG_OFFER) {
      onOffer(link, msg);
    } else if (msg.tag === TAG_ANSWER) {
      onAnswer(link, msg);
    } else if (msg.tag === TAG_CANDIDATE) {
      onCandidate(link, msg);
    } else if (msg.tag === TAG_BYE) {
      onBye(link);
    }
  }

  // ── Entry points ─────────────────────────────────────────────────────────

  // The room has seated us. Called from Rust whenever the seat changes, so it
  // must be cheap and idempotent for the seat it is already on.
  window.__btJoin = function (seat) {
    if (me === seat) {
      return false;
    }
    if (me !== null) {
      // Re-seated: tell everyone who knew the old seat, and forget them. Their
      // `leadsOffer` comparisons were against the old number.
      links.forEach(function (link) {
        sendBye(link.seat, link.myEpoch, BYE_REPLACED);
      });
      links.clear();
      seen.clear();
    }
    note(seat, me === null ? 'joined' : 're-seated from ' + me);
    me = seat;
    sessionEpoch += 1;
    window.__bt.me = seat;
    window.__bt.error = null;
    window.__bt.state = 'new';
    announce();
    rollup();
    return true;
  };

  // Whatever has arrived since the last drain, handed to the links. Called by
  // `probe_peer_out` once a frame.
  window.__btDrain = function () {
    var inbox = window.__btInbox;
    if (!inbox || inbox.length === 0) {
      return;
    }
    var take = inbox.splice(0, inbox.length);
    for (var i = 0; i < take.length; i += 1) {
      deliver(take[i].from, take[i].bytes);
    }
  };

  // ── The state and control channels ───────────────────────────────────────
  //
  // What the game puts on a data channel, and what arrived on one. The shim
  // never parses either — `island-wire` owns the format.
  window.__btSendLink = function (seat, kind, bytes) {
    var link = links.get(seat);
    if (!link) {
      return false;
    }
    var channel = kind === LINK_STATE ? link.state : link.ctrl;
    if (!channel || channel.readyState !== 'open') {
      return false;
    }
    channel.send(bytes);
    if (kind === LINK_STATE) {
      window.__bt.sentState += 1;
    } else {
      window.__bt.sentCtrl += 1;
    }
    return true;
  };

  // Whatever arrived on the data channels since the last drain, oldest first.
  window.__btDrainLinks = function () {
    var out = recv;
    recv = [];
    return out;
  };

  // Send `n` state messages, so a receiver has something to count and a
  // report has something to compare against what arrived.
  window.__btSend = function (n) {
    var sent = 0;
    links.forEach(function (link) {
      if (!link.state || link.state.readyState !== 'open') {
        return;
      }
      var payload = new Uint8Array(16);
      for (var i = 0; i < n; i += 1) {
        payload[0] = i & 0xff;
        payload[1] = (i >> 8) & 0xff;
        link.state.send(payload);
        window.__bt.sent += 1;
        sent += 1;
      }
    });
    if (sent === 0) {
      return Promise.reject(new Error('no state channel is open'));
    }
    return Promise.resolve(sent);
  };

  // ── ICE diagnostic ───────────────────────────────────────────────────────

  // What this end can learn about its own address, with no peer involved. A
  // capture asserts `srflx` is non-zero: host candidates prove nothing, because
  // a machine gathers those with no server configured at all, and a browser
  // disguises them as mDNS names. This is the one check that separates "ICE is
  // configured" from "a STUN server answered" — and the first thing a
  // two-network test should look at when a link does not open.
  //
  // The result is left on `window.__bt.iceProbe` as well as resolved, so a
  // harness that cannot await a promise can poll it.
  window.__btIceProbe = function () {
    var result = {
      state: 'gathering',
      source: ICE.source,
      urls: iceUrls(ICE.servers),
      types: {},
      srflx: null,
      error: null,
    };
    window.__bt.iceProbe = result;
    if (ICE.servers.length === 0) {
      result.state = 'none';
      return Promise.resolve(result);
    }
    var pc;
    try {
      pc = new RTCPeerConnection({ iceServers: ICE.servers });
    } catch (e) {
      result.state = 'error';
      result.error = describe(e);
      return Promise.resolve(result);
    }
    return new Promise(function (resolve) {
      var settled = false;
      var timer = setTimeout(function () {
        done(result.srflx ? 'gathered' : 'timeout');
      }, ICE_PROBE_MS);
      function done(state) {
        if (settled) {
          return;
        }
        settled = true;
        clearTimeout(timer);
        result.state = state;
        try {
          pc.close();
        } catch (e) {
          /* already gone */
        }
        resolve(result);
      }
      pc.onicecandidate = function (e) {
        if (!e.candidate) {
          return;
        }
        var type = candidateType(e.candidate);
        result.types[type] = (result.types[type] || 0) + 1;
        if (type === 'srflx' && !result.srflx) {
          result.srflx = candidateAddress(e.candidate);
        }
      };
      pc.onicegatheringstatechange = function () {
        if (pc.iceGatheringState === 'complete') {
          done(result.srflx ? 'complete' : 'no-srflx');
        }
      };
      pc.createDataChannel('probe');
      pc.createOffer().then(function (offer) {
        return pc.setLocalDescription(offer);
      }).catch(function (e) {
        result.error = describe(e);
        done('error');
      });
    });
  };

  // Close every connection, so a run leaves nothing behind.
  window.__btClose = function () {
    links.forEach(function (link) {
      reset(link, 'closed by the page');
    });
    links.clear();
    seen.clear();
    window.__bt.state = 'closed';
    return Promise.resolve(true);
  };
})();
