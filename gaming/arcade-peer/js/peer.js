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
// ## An offer that is not answered
//
// `startOffer` refuses while an attempt is outstanding, and a link the browser
// never connects stays `connecting` until ICE consent gives up — about half a
// minute — so a signal lost in flight parks the pair for that long with nothing
// to restart it. `OFFER_MS` abandons the attempt, tells the far side, and lets
// the ordinary paths start a fresh one on a newer epoch.
//
// ## The offer queue
//
// A page just named twelve peers would otherwise write twelve offers in one
// frame, each with its own candidates. `MAX_NEGOTIATING` keeps four in flight
// and queues the rest; `__btDrain` starts the next as a slot frees — connected,
// expired, or reset. The cap is about the burst a group change makes, not the
// steady state: the room regroups on every join, so an ordinary world never
// queues.
//
// ## A link that dies
//
// ICE does not always say so: the transport can sit `connected` while both
// channels are closed, and a `failed` connection without `?persist=1` used to be
// terminal — the record stayed, so nothing renegotiated it. `dropDead` is the one
// exit: reset, tell the far side, and let the leader offer again if the room
// still names the seat. `disconnected` waits `LINK_STALE_MS` because it often
// comes back on its own; `failed` acts at once; a channel that closes under us
// waits `LINK_SETTLE_MS` for the far side's `Bye`, which a deliberate close
// always sends just behind it — and a set change is exactly that, so the window
// is what stops the shim re-offering to a seat it is about to drop.
//
// The cheap repair, tried first on `disconnected`, is an ICE **restart**: the end
// that offered asks for new candidates on the connection both ends already have
// (`TAG_RESTART`), the far side answers without a reset, and the channels — and
// anything buffered on them — stay where they are. The offer reuses the attempt's
// epoch deliberately: a higher one means "start over", which would tear down the
// very channels the restart is there to keep.
//
// ## What the path is worth
//
// A peer one hop away and a peer on another continent are the same 20 Hz
// stream; what differs is the confirmed stream's round trip. `__btDrain` samples
// each link's nominated candidate pair once a second and leaves the round trip
// and both candidates' types on the link, so a report can say "direct, 4.2 ms"
// rather than assuming it. Design §12.2 is the measurement that decides whether
// colo belongs in the interest set, and this is its instrument.
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
  // An offer for the connection both ends already have: new ICE credentials,
  // same `RTCPeerConnection`, same channels. It carries the epoch of the attempt
  // it belongs to — a higher epoch means "start over", which is the teardown this
  // exists to avoid — and is answered with a plain Answer.
  var TAG_RESTART = 6;

  var MAX_SDP = 8192;
  var MAX_CANDIDATE = 512;

  // The `to` a queued signal carries when it is for everyone. The room routes a
  // targeted signal to one seat and copies a broadcast one to the sender's
  // reach set; a world holds 64 seats, so 0xffff cannot collide with one.
  var BROADCAST = 0xffff;

  var BYE_REPLACED = 1;

  // Bounded interest sets are the design's rule, not this file's to invent.
  // The room publishes a group (§5), and this is the shim's half of it: links
  // outside the group are closed and no new one is opened. `MAX_LINKS` stays as
  // a hard cap — a group that is somehow larger than the design's bound still
  // cannot produce a mesh — but it should never bind.
  var MAX_LINKS = 12;

  // Persistent links hold dormant channels too, so the cap has to cover the
  // set and the seats it recently dropped. When it binds, the longest-dormant
  // link is evicted: the ranking problem moved from the room to the client,
  // which §5 says is what this costs.
  var MAX_LINKS_PERSIST = 18;

  // How many offers this page runs at once. A leader whose group has just grown
  // would otherwise create one connection, one SDP and a stream of candidates
  // per new seat in the same frame, and every one of those messages leaves
  // together — the burst §9 measures. The rest wait their turn and start as the
  // ones in flight settle: connected, expired, or reset.
  var MAX_NEGOTIATING = 4;

  // How often the same link may be restarted. A path that flaps reports
  // `disconnected` on each one, and a restart per flap is a handshake's worth of
  // messages for a blip. The stale timer is the backstop if a restart does not
  // take: past it the link is torn down and offered again.
  var RESTART_EVERY_MS = 5000;

  // ── ICE ──────────────────────────────────────────────────────────────────
  //
  // Two STUN servers by default. STUN is how a peer learns its own address on
  // the far side of its NAT; without it a peer offers host candidates only, and
  // a browser disguises those as mDNS `.local` names, so two peers on different
  // networks never find a pair.
  //
  // TURN is the relay of last resort, for the symmetric-NAT pair STUN cannot
  // connect. Its credentials are short-lived and cannot live in this file, so
  // they come from the room: `GET /ice` mints them server-side and answers the
  // list a `RTCPeerConnection` takes. The fetch starts at load and a join waits
  // for it, because a connection negotiated before the list arrived would be
  // STUN-only for the rest of its life. An empty answer — the route not
  // deployed, no secret configured, the mint failing — leaves the default in
  // place: TURN is an improvement, never a requirement.
  //
  // `?ice=off` for a run that must not depend on the network; `?ice=stun:a:1,…`
  // to point a test at named servers. Either wins over the room's list, and
  // neither fetches it.
  //
  // `?relay=1` restricts gathering to relay candidates: how a run proves the
  // credentials work, rather than that two peers happen to be on one machine.
  //
  // The default pair is measured rather than remembered. `freeice`'s `ice-check`
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

  var ICE = iceFromQuery(window.location.search);
  var RELAY = /[?&]relay=1/.test(window.location.search || '');
  var OFFLINE = /[?&]offline=1/.test(window.location.search || '');
  // Persistent links (design §5): a seat the room stops naming keeps its
  // channel, dormant, so walking back into view costs no handshake. Off by
  // default while it is measured — `p2p-persist.dsl` and `p2p-persist-off.dsl`
  // are the A/B, and the eviction rule below is what the run is judging.
  var PERSIST = /[?&]persist=1/.test(window.location.search || '');

  // How long an offer may go unanswered before the attempt is abandoned and
  // retried. Longer than a handshake takes on any path that connects at all
  // (loopback measures ~100 ms; a TURN relay adds round trips, not tens of
  // seconds), and short enough that a lost signal costs a retry rather than a
  // stare. `?offerMs=<ms>` overrides it; `?offerMs=0` turns expiry off.
  var OFFER_MS = offerMs(window.location.search);
  // How long an attempt the far side has **answered** may take to connect. Longer
  // than ICE's own give-up (~30 s) on purpose: `ICE failed` is the report worth
  // having, and it reaches `dropDead` by itself. This is the backstop for a
  // browser that never says anything. `?answerMs=<ms>` overrides it; `0` waits.
  var ANSWER_MS = answerMs(window.location.search);

  // How long a link may sit `disconnected` before it is treated as dead. ICE
  // reports `disconnected` for a path that may come back — a wifi hiccup, a
  // route change — and `failed` for one that will not, so the first waits and the
  // second does not. `?staleMs=<ms>` overrides it; `?staleMs=0` waits forever.
  var LINK_STALE_MS = staleMs(window.location.search);

  // How long a channel that closed under a live link is given for the far side's
  // `Bye` to arrive. A deliberate close always sends one, and when the room's set
  // changed the two cross in flight: without this window the close looks like a
  // crash and the shim re-offers to a seat it is about to drop. A channel that
  // closes with no `Bye` behind it is a peer that died mid-connection.
  var LINK_SETTLE_MS = 1000;

  // How long an offer refused for being from a seat outside the room's set is
  // held before it is dropped. The room's set and the signals it implies arrive
  // on different frames, so a seat returning to a group can offer before the
  // other end has been told the group names it. That offer used to be discarded,
  // and the pair waited for `OFFER_MS` (12 s) to try again; holding it for a
  // moment and replaying it when the set arrives makes the return cost a round
  // trip. Long enough to cover a room message to every seat, short enough that
  // a genuinely stale offer is not applied.
  var DEFER_MS = 2000;

  // The buffer's bound. A gather is tens of candidates, and only signals that
  // arrived inside `DEFER_MS` survive, so this is a guard against a malformed
  // sender rather than a working limit.
  var MAX_DEFERRED = 256;

  // How long a join waits for the room's list. Under `wrangler dev` the mint is
  // a round trip to Cloudflare Realtime and back, and a longer budget would be
  // a visible delay at the door for a relay that may not be needed. Past it the
  // default is used.
  var ICE_BUDGET_MS = 1500;

  var iceSettled = false;
  var iceReady = null;
  var pendingJoin = null;

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

  function offerMs(search) {
    var match = /[?&]offerMs=(\d+)/.exec(search || '');
    if (!match) {
      return 12000;
    }
    var ms = parseInt(match[1], 10);
    return ms > 0 ? ms : Infinity;
  }

  function staleMs(search) {
    var match = /[?&]staleMs=(\d+)/.exec(search || '');
    if (!match) {
      return 10000;
    }
    var ms = parseInt(match[1], 10);
    return ms > 0 ? ms : Infinity;
  }

  function answerMs(search) {
    var match = /[?&]answerMs=(\d+)/.exec(search || '');
    if (!match) {
      return 30000;
    }
    var ms = parseInt(match[1], 10);
    return ms > 0 ? ms : Infinity;
  }

  function reportIce() {
    if (!window.__bt) {
      return;
    }
    window.__bt.ice = {
      source: ICE.source,
      urls: iceUrls(ICE.servers),
      room: ICE.room,
      relay: RELAY,
    };
  }

  // The room's list, ahead of the default. A default earns its place only if it
  // names an address the room's list does not already have: Cloudflare's mint
  // carries its own STUN server, and listing it twice buys nothing. Google's is
  // a second operator, so it is the entry that survives.
  function mergeIce(room, defaults) {
    var seen = {};
    var merged = [];
    function urlsOf(server) {
      return Array.isArray(server.urls) ? server.urls : [server.urls];
    }
    function take(server, onlyIfNew) {
      var urls = urlsOf(server);
      if (onlyIfNew && urls.every(function (url) { return seen[url]; })) {
        return;
      }
      urls.forEach(function (url) { seen[url] = true; });
      merged.push(server);
    }
    room.forEach(function (server) { take(server, false); });
    defaults.forEach(function (server) { take(server, true); });
    return merged;
  }

  // Ask the room for credentials, once. `ICE.room` says what came back, for a
  // report that would otherwise show only the list it ended up with:
  // 'off' (an override, or no room on this origin), 'pending', 'ok', 'empty'
  // (the route answered and had nothing to add), 'unavailable' (no route).
  function loadRoomIce() {
    if (ICE.source !== 'default' || OFFLINE) {
      ICE.room = 'off';
      iceSettled = true;
      reportIce();
      return Promise.resolve(ICE);
    }
    ICE.room = 'pending';
    reportIce();
    var settled = false;
    return new Promise(function (resolve) {
      var controller = typeof AbortController === 'function' ? new AbortController() : null;
      var timer = setTimeout(function () {
        if (controller) {
          controller.abort();
        }
        finish('unavailable');
      }, ICE_BUDGET_MS);
      function finish(state) {
        if (settled) {
          return;
        }
        settled = true;
        iceSettled = true;
        clearTimeout(timer);
        if (state !== null) {
          ICE.room = state;
        }
        reportIce();
        resolve(ICE);
      }
      fetch('/ice', {
        cache: 'no-store',
        signal: controller ? controller.signal : undefined,
      }).then(function (response) {
        return response.ok ? response.json() : null;
      }).then(function (body) {
        var servers = body && Array.isArray(body.iceServers) ? body.iceServers : [];
        servers = servers.filter(function (server) {
          return server && (typeof server.urls === 'string' ||
            (Array.isArray(server.urls) && server.urls.length > 0));
        });
        if (servers.length === 0) {
          finish('empty');
          return;
        }
        ICE.servers = mergeIce(servers, DEFAULT_ICE);
        finish('ok');
      }).catch(function () {
        finish('unavailable');
      });
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

  // seat -> the last epoch this page used with it, kept across link records. A
  // record is deleted when a link dies or a seat leaves the set, and a recreated
  // one must not restart the count below what the far side has already seen: it
  // would read every later offer as one from an older attempt and ignore it.
  var epochs = new Map();

  // Seats waiting for an offer slot (`MAX_NEGOTIATING`), in arrival order.
  var queue = [];

  // Offers, answers and candidates refused because the sender was not in this
  // seat's set yet: `{from, bytes, at}`. Replayed in arrival order when the set
  // names the sender, expired after `DEFER_MS`. Every pairwise kind is held,
  // because a description without the candidates around it is a link that never
  // starts.
  var deferred = [];

  // The room's interest set for this seat: who it is worth keeping a channel
  // to. Null until the room has said; before then every peer, which is what
  // the room does until its sim has an opinion about who is near whom.
  var group = null;

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
    live: 0,
    idle: 0,
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
    // Pairwise signals that reached us addressed to another peer. With the
    // room routing a targeted signal to one seat this is zero, and a number
    // that climbs means the broadcast path came back.
    ignored: 0,
    // Signals for a pair we hold no link to: a race after a bye, or the room
    // carrying a dropped pair's message inside its grace (§9).
    unknown: 0,
    // Discovery, counted: announces queued for the room, announces received,
    // and signals dropped because the sender is outside this seat's group.
    // An announce is the first step of every negotiation, so when a seat the
    // room names has no link, these say whether it never asked, asked and was
    // answered by silence, or was dropped here.
    signals: { announced: 0, announces: 0, dropped: 0 },
    // Negotiations started, cumulative: `offered` by this page, `answered` for
    // a peer that offered, and `expired` offers abandoned unanswered; `failed` is
    // links the browser gave up on and the shim renegotiated; `restarts` is ICE
    // restarts this page asked for, `restartAnswered` the ones it answered. `peak` is
    // the most offers this page ever ran at once, which is what the queue behind
    // `MAX_NEGOTIATING` is there to bound. The number
    // persistence exists to stop moving when the room's set changes and nothing
    // else did.
    negotiations: {
      offered: 0,
      answered: 0,
      expired: 0,
      failed: 0,
      restarts: 0,
      restartAnswered: 0,
      peak: 0,
    },
    // Game bytes that arrived on a dormant link, and bytes the game offered one.
    // Both are dropped: a dormant link is open and silent.
    idleDropped: 0,
    // What each link is doing, refreshed by `rollup`, so a report has more than
    // a count when a connection does not hold.
    links: [],
    // Who the room says this player should be linked to: `{ epoch, members }`,
    // or null before it has said. Its length is the peer count the room
    // considers worth a channel, which is not the same as the world's seats.
    group: null,
    // A bounded ring of link events — who offered, who answered, what closed —
    // because "state: closed" is not a diagnosis. Read it with
    // `eval "<peer>" "window.__bt.events" out=events.json`.
    events: [],
    // Bytes the game offered a link that could not take them, by seat, channel
    // and reason. Bounded by the link count and the two channels.
    refused: {},
    // What ICE was configured with: `source` is 'default' | 'query' | 'off',
    // `urls` is the list, `room` says what the room's credential route answered
    // ('off' | 'pending' | 'ok' | 'empty' | 'unavailable'), and `relay` is
    // whether gathering was restricted to relay candidates. A report says which
    // list a run used rather than assuming it.
    ice: { source: ICE.source, urls: iceUrls(ICE.servers), room: ICE.room, relay: RELAY },
    // Every candidate gathered, by type. `host` is this machine and would be
    // gathered with no server configured at all; `srflx` is the one that proves
    // a STUN server answered.
    candidates: { host: 0, srflx: 0, prflx: 0, relay: 0 },
    // The address a STUN server reported for us, once one has. Null is the
    // difference between "ICE is configured" and "ICE answered".
    mapped: null,
  };

  // Ask the room for TURN credentials now, so the join is not the first thing
  // that has to wait for them.
  iceReady = loadRoomIce();

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
    window.__btQueueSignal(link.seat, frame(tag, link.myEpoch, link.seat, framed));
  }

  function sendBye(seat, epoch, reason) {
    window.__btQueueSignal(seat, frame(TAG_BYE, epoch, seat, new Uint8Array([reason])));
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
      if (tag === TAG_OFFER || tag === TAG_RESTART || tag === TAG_ANSWER) {
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
    if (!makeRoom()) {
      return null;
    }
    link = {
      seat: seat,
      pc: null,
      ctrl: null,
      state: null,
      // What this pair last agreed on, my direction and theirs. Each only moves
      // forward, and only within its own direction: two peers' counters mean
      // nothing to each other, which is why they are tracked separately.
      myEpoch: epochs.get(seat) || sessionEpoch,
      theirEpoch: 0,
      // I am the offerer in this negotiation, so a competing offer is mine to
      // judge. Cleared when I answer instead.
      offerer: false,
      // When the outstanding offer was written, or 0 when there is none. The
      // sweep in `__btDrain` abandons an attempt older than `OFFER_MS`.
      offerSince: 0,
      // When ICE first said `disconnected` — or a channel closed under a live
      // link — and how long that suspicion is given. The sweep gives up after it.
      staleSince: 0,
      staleFor: 0,
      // When this link last asked for an ICE restart, so a flapping path cannot
      // ask once per flap.
      restartedAt: 0,
      // How many times a channel has opened on this record. A restart leaves it
      // at 1; a renegotiation makes a new record and starts again.
      opens: 0,
      // The harness closed this link's transport without telling anyone
      // (`__btDrop`). Its own events are ignored until it is renegotiated.
      silent: false,
      remoteSet: false,
      pending: [],
      // In the room's group, or dropped from it and kept open (persistence).
      // An idle link sends nothing and hands nothing to the game.
      idle: false,
      idleSince: 0,
      // What this link has carried, each way, by channel. The roll-up counts
      // are for the page; these are for the answer when one link differs from
      // its neighbours — an ack that reaches every peer but one, or a link whose
      // acks never arrive.
      sentCtrl: 0,
      recvCtrl: 0,
      sentState: 0,
      recvState: 0,
      // The nominated candidate pair, sampled once a second (see
      // `samplePaths`). `null` until a report lands: a connection has no round
      // trip to report until its checks have succeeded.
      rttMs: null,
      local: null,
      remote: null,
      sampling: false,
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

  // Reusable: the group names the seat, the channels are open, and the
  // transport says connected. The channel state alone is not enough — after a
  // peer closes its end they can read open for a while, and reusing one of
  // those would black-hole the seat rather than renegotiate it.
  function isHealthy(link) {
    return link.pc !== null && link.pc.connectionState === 'connected' && isOpen(link);
  }

  function maxLinks() {
    return PERSIST ? MAX_LINKS_PERSIST : MAX_LINKS;
  }

  // Make space for one more link. At the cap, persistence evicts the link that
  // has been dormant longest — the seats dropped most recently are the ones a
  // player is most likely to walk back into.
  function makeRoom() {
    if (links.size < maxLinks()) {
      return true;
    }
    if (!PERSIST) {
      return false;
    }
    var oldest = null;
    links.forEach(function (link) {
      if (!link.idle) {
        return;
      }
      if (oldest === null || link.idleSince < oldest.idleSince) {
        oldest = link;
      }
    });
    if (oldest === null) {
      return false;
    }
    note(oldest.seat, 'evicted: dormant longest');
    sendBye(oldest.seat, oldest.myEpoch, BYE_REPLACED);
    reset(oldest, 'evicted');
    links.delete(oldest.seat);
    seen.delete(oldest.seat);
    return true;
  }

  // The room stopped naming this seat. The channel stays open and silent.
  function setIdle(link) {
    if (link.idle) {
      return;
    }
    link.idle = true;
    link.idleSince = performance.now();
    note(link.seat, 'dormant: left the group');
    rollup();
  }

  function setLive(link) {
    if (!link.idle) {
      return;
    }
    link.idle = false;
    link.idleSince = 0;
    note(link.seat, 'live: back in the group');
    rollup();
  }

  function rollup() {
    var open = 0;
    var openLive = 0;
    var dormant = 0;
    var live = 0;
    var detail = [];
    links.forEach(function (link) {
      if (isOpen(link)) {
        open += 1;
        if (link.idle) {
          dormant += 1;
        } else {
          openLive += 1;
        }
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
        sentCtrl: link.sentCtrl,
        recvCtrl: link.recvCtrl,
        sentState: link.sentState,
        recvState: link.recvState,
        opens: link.opens,
        idle: link.idle,
        healthy: isHealthy(link),
        // The path this link is actually using: tenths of a millisecond of
        // round trip, and whether each end is a local address, a mapped one or
        // a relay. `null` until the first sample.
        rttMs: link.rttMs,
        local: link.local,
        remote: link.remote,
      });
    });
    window.__bt.links = detail;
    window.__bt.open = open;
    window.__bt.live = openLive;
    window.__bt.idle = dormant;
    window.__bt.peers = links.size;
    window.__bt.queue = queue.length;
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
    link.offerSince = 0;
    link.staleSince = 0;
    link.staleFor = 0;
    link.restartedAt = 0;
    link.silent = false;
    link.remoteSet = false;
    link.pending = [];
    rollup();
  }

  // Mark a link suspect: it may be dying, or the far side may be about to tell us
  // it left. The first suspicion keeps its (earlier) deadline.
  function suspect(link, forMs) {
    if (link.staleSince === 0) {
      link.staleSince = performance.now();
      link.staleFor = forMs;
    }
  }

  // A link the browser has given up on.
  //
  // Reset the browser half, forget the record, and tell the far side — it may be
  // holding the other half of a connection that will never carry anything. Then
  // the ordinary paths start again if the room still names the seat: the leader
  // offers, the follower announces. Persistence does not change that: a dormant
  // link that died is not dormant, it is gone.
  //
  // Idempotent, because one dead connection reports itself twice — `ICE failed`
  // and `connection failed` are the same event in two spellings, and the
  // channels closing behind them are a third.
  function dropDead(link, why) {
    if (links.get(link.seat) !== link) {
      return;
    }
    var seat = link.seat;
    var epoch = link.myEpoch;
    var live = !link.idle && (group === null || group.members.has(seat));
    window.__bt.negotiations.failed += 1;
    note(seat, why);
    reset(link, why);
    links.delete(seat);
    seen.delete(seat);
    sendBye(seat, epoch, BYE_REPLACED);
    if (me !== null && live) {
      if (leadsOffer(seat)) {
        if (linkFor(seat) !== null) {
          queueOffer(seat);
        }
      } else {
        announce();
      }
    }
    rollup();
  }

  function wire(channel, link, name) {
    channel.binaryType = 'arraybuffer';
    channel.onopen = function () {
      note(link.seat, name + ' open');
      link.opens += 1;
      rollup();
    };
    channel.onclose = function () {
      note(link.seat, name + ' closed');
      // A deliberate close (`reset`, a group change, a `Bye`) has already nulled
      // the field before this queued event runs, and a renegotiation replaces it
      // with a different object; either way the channel that closed is no longer
      // one of the link's. A channel that closes under its own steam is the
      // transport dying without ICE saying so.
      var current = name === 'state' ? link.state : link.ctrl;
      if (current === channel && !link.silent) {
        suspect(link, LINK_SETTLE_MS);
      }
      rollup();
    };
    channel.onerror = function () {
      // A channel error is the transport dying, not a fault in this page: the
      // far side's ICE going raises one here, and it fires on the half that is
      // still healthy. `fail` never clears, so reporting it left `error` set for
      // the rest of a run after a link had already been repaired — a state the
      // design reads as a failure. Let an error settle exactly as a close does;
      // the repair is `dropDead`'s job.
      var current = name === 'state' ? link.state : link.ctrl;
      if (current === channel && !link.silent) {
        suspect(link, LINK_SETTLE_MS);
      }
      rollup();
    };
    channel.onmessage = function (event) {
      if (link.idle) {
        // Dormant: the room no longer names this seat, so its samples are not
        // ours to draw. Counted apart from `received` so that stays a rate of
        // samples the game can use.
        window.__bt.idleDropped += 1;
        if (name === 'state') {
          link.recvState += 1;
        } else {
          link.recvCtrl += 1;
        }
        return;
      }
      if (name === 'state') {
        window.__bt.received += 1;
        link.recvState += 1;
        window.__bt.buffered = channel.bufferedAmount;
        if (window.__bt.received % 20 === 0) {
          sample();
        }
      } else {
        window.__bt.receivedCtrl += 1;
        link.recvCtrl += 1;
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
    var options = { iceServers: ICE.servers };
    if (RELAY) {
      // Only a relay may carry this connection. `?relay=1` is a proof, not a
      // deployment: with no TURN credentials it cannot connect at all.
      options.iceTransportPolicy = 'relay';
    }
    var pc = new RTCPeerConnection(options);
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
        dropDead(link, 'ICE failed');
      }
    };
    pc.onconnectionstatechange = function () {
      note(link.seat, 'connection ' + pc.connectionState);
      if (pc.connectionState === 'failed') {
        dropDead(link, 'connection failed');
      } else if (pc.connectionState === 'disconnected') {
        // May come back on its own. The stale timer is the backstop; the end
        // that offered asks ICE to try a new path first, because that is the
        // cheap repair and it leaves the channels alone.
        suspect(link, LINK_STALE_MS);
        startRestart(link);
      } else {
        link.staleSince = 0;
        link.staleFor = 0;
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
    window.__bt.signals.announced += 1;
    window.__btQueueSignal(BROADCAST, frame(TAG_ANNOUNCE, sessionEpoch, null, new Uint8Array(0)));
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
    link.offerSince = performance.now();
    var inFlight = offersInFlight();
    if (inFlight > window.__bt.negotiations.peak) {
      window.__bt.negotiations.peak = inFlight;
    }
    // A fresh attempt needs a newer epoch. Without one, a far side still holding
    // a stale link reads the offer as a duplicate of the one it already answered
    // and ignores it; every other value on the wire already moves forward per
    // attempt, and this one did not.
    link.myEpoch = (link.myEpoch % 65535) + 1;
    epochs.set(link.seat, link.myEpoch);
    window.__bt.negotiations.offered += 1;
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

  // Ask ICE to try again on the connection this pair already has.
  //
  // Only the end that offered does this: it is the same rule that decides who
  // offers, so two ends cannot restart at each other. The offer carries the
  // attempt's epoch unchanged, which is what stops the far side's `deliver`
  // treating it as a restart of the *negotiation* and resetting the channels this
  // exists to keep. Nothing here is timed by `OFFER_MS`: a restart that does not
  // take is caught by the stale timer, which tears the link down properly.
  function startRestart(link) {
    if (!link.offerer || link.pc === null || link.offerSince !== 0) {
      return false;
    }
    if (!isOpen(link)) {
      // Channels already gone: there is no association left to keep, and the
      // settle timer is about to tear the link down properly.
      return false;
    }
    var now = performance.now();
    if (link.restartedAt !== 0 && now - link.restartedAt < RESTART_EVERY_MS) {
      return false;
    }
    link.restartedAt = now;
    window.__bt.negotiations.restarts += 1;
    note(link.seat, 'restarting ice');
    var pc = link.pc;
    pc.restartIce();
    pc.createOffer()
      .then(function (offer) {
        return pc.setLocalDescription(offer);
      })
      .then(function () {
        sendBlob(TAG_RESTART, link, pc.localDescription.sdp, MAX_SDP);
      })
      .catch(function (e) {
        fail(link, 'restart: ' + describe(e));
      });
    return true;
  }

  function onAnnounce(link) {
    if (!seen.has(link.seat)) {
      seen.add(link.seat);
      announce();
    }
    if (leadsOffer(link.seat)) {
      queueOffer(link.seat);
    }
  }

  function offersInFlight() {
    var n = 0;
    links.forEach(function (link) {
      if (link.offerSince !== 0) {
        n += 1;
      }
    });
    return n;
  }

  function queueOffer(seat) {
    if (queue.indexOf(seat) === -1) {
      queue.push(seat);
    }
    pumpQueue();
  }

  // Start what fits, called every frame from `__btDrain` — so a slot freed by a
  // connection, an expiry or a `Bye` is taken by the next seat without every
  // one of those paths having to remember to. A queued seat that has left the
  // group, lost its link, or no longer leads is dropped rather than offered to.
  function pumpQueue() {
    while (queue.length > 0 && offersInFlight() < MAX_NEGOTIATING) {
      var seat = queue.shift();
      if (group !== null && !group.members.has(seat)) {
        continue;
      }
      var link = links.get(seat);
      if (link === undefined || !leadsOffer(seat)) {
        continue;
      }
      startOffer(link);
    }
  }

  // Abandon an attempt that is going nowhere.
  //
  // Two deadlines, because there are two ways an attempt fails and only one of
  // them is ours to time. An offer **nobody answered** is a lost signal: the
  // fault is here, and `OFFER_MS` retries it. An offer the far side **answered**
  // is no longer ours to judge — ICE decides, and it says `failed` in about
  // thirty seconds — so `ANSWER_MS` is only the backstop for a browser that
  // never says anything.
  //
  // One deadline used to do both, at twelve seconds. That is shorter than ICE's
  // own give-up, so a pair whose answer arrived and whose checks could not
  // succeed was reset before it could fail: it re-offered every twelve seconds
  // forever, and `failed` — the counter that exists to notice a dead link —
  // never counted one (design §15).
  function sweepOffer(link, now) {
    if (link.offerSince === 0) {
      return;
    }
    if (isOpen(link) || (link.pc !== null && link.pc.connectionState === 'connected')) {
      // The handshake did connect; from here a failure is ICE's to report.
      link.offerSince = 0;
      return;
    }
    var answered = link.remoteSet;
    if (now - link.offerSince < (answered ? ANSWER_MS : OFFER_MS)) {
      return;
    }
    var seat = link.seat;
    var epoch = link.myEpoch;
    var why = answered ? 'no path after the answer' : 'offer expired';
    if (answered) {
      window.__bt.negotiations.failed += 1;
    } else {
      window.__bt.negotiations.expired += 1;
    }
    note(seat, why);
    reset(link, why);
    sendBye(seat, epoch, BYE_REPLACED);
    if (me === null || link.idle) {
      return;
    }
    if (leadsOffer(seat)) {
      startOffer(link);
    } else {
      announce();
    }
  }

  function sweepOffers(now) {
    links.forEach(function (link) {
      sweepOffer(link, now);
    });
  }

  // A link that stayed suspect past its deadline: ICE said `disconnected` and
  // never took it back, or a channel closed and no `Bye` followed. Collected
  // first, because `dropDead` may recreate the record for a retry and a live
  // iteration would then visit the new one too.
  function sweepStale(now) {
    var dead = [];
    links.forEach(function (link) {
      if (link.staleSince !== 0 && now - link.staleSince >= link.staleFor) {
        dead.push(link);
      }
    });
    for (var i = 0; i < dead.length; i += 1) {
      dropDead(dead[i], 'silent for ' + dead[i].staleFor + ' ms');
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
    window.__bt.negotiations.answered += 1;
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

  // Answer a restart on the connection both ends already have.
  //
  // No `reset`, which is the whole point: the offer carries new ICE credentials
  // for an existing association, and applying it keeps the channels. A restart
  // for a link whose browser half is already gone is ignored — there is nothing
  // to restart, and the end that asked will reach its stale timer and start over.
  function onRestart(link, msg) {
    if (link.pc === null || link.pc.connectionState === 'closed') {
      // Nothing to restart. The end that asked reaches its stale timer and
      // starts over, which is the only repair available from here.
      return;
    }
    window.__bt.negotiations.restartAnswered += 1;
    link.offerer = false;
    link.offerSince = 0;
    note(link.seat, 'restart taken ' + msg.sdp.length + 'B sdp');
    var pc = link.pc;
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
        fail(link, 'restart answer: ' + describe(e));
      });
  }

  function onAnswer(link, msg) {
    if (!link.pc || !link.offerer) {
      note(link.seat, 'answer ignored: not the offerer');
      return;
    }
    // An answer is only good for an offer that is still outstanding. One that
    // arrives after the attempt it belongs to was reset — expiry, an ICE
    // failure, a restart — lands on either a connection in `stable` (the new
    // offer has not been written yet) or one that has already been answered,
    // and `setRemoteDescription` throws "called in wrong state". The throw used
    // to reach `fail`, which tore down the *new* attempt: a reset storm that
    // only became reachable once an answered offer was given time to fail
    // (design §15). The far side re-answers every offer it receives, so
    // dropping this one loses nothing.
    if (link.pc.signalingState !== 'have-local-offer') {
      note(link.seat, 'answer ignored: no offer outstanding');
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
    if (group !== null && !group.members.has(from) && !links.has(from)) {
      // The room delivers to the sender's group, so this is a set that changed
      // between the room deciding and the frame arriving. Dropping it is what
      // the new set would have done. A link we hold is the exception: a dormant
      // one still has to hear `Bye` or a restarted offer, or persistence would
      // keep a link nobody can use and never replace it.
      window.__bt.signals.dropped += 1;
      if (msg.tag === TAG_OFFER || msg.tag === TAG_ANSWER || msg.tag === TAG_CANDIDATE) {
        // The set that names the sender has not arrived yet. Hold the whole
        // pairwise stream — not just the offer — and replay it from
        // `__btGroup`. A candidate arrives either side of the description it
        // belongs to, and one dropped here is one the far side never gathers
        // again: the offerer's gather is already finished by the time a replayed
        // offer lands, so the pair sat in `new` until ICE's own timeout.
        deferred.push({ from: from, bytes: bytes, at: performance.now() });
        if (deferred.length > MAX_DEFERRED) {
          deferred.shift();
        }
        note(from, 'signal deferred: set has not arrived');
      } else if (msg.tag === TAG_ANNOUNCE) {
        note(from, 'announce dropped: outside the group');
      }
      return;
    }
    if (msg.tag === TAG_ANNOUNCE) {
      window.__bt.signals.announces += 1;
    } else if (msg.to !== me) {
      // Addressed to another peer. The room routes a targeted signal to one
      // seat, so this is now only reachable with an old room or a relay in a
      // scenario — and the count staying at zero is what says targeting holds.
      window.__bt.ignored += 1;
      return;
    }
    var link = links.get(from);
    if (!link) {
      if (msg.tag === TAG_ANNOUNCE) {
        // A seat we have not met. At the cap it is ignored rather than told:
        // a `Bye` answered by a `Bye` from a peer that is also at its cap
        // ping-pongs, and an announce is broadcast, so it is not a request.
        if (!makeRoom()) {
          return;
        }
      } else if (msg.tag === TAG_OFFER) {
        // They are negotiating with a seat we have no link for, so offer them
        // the courtesy of a refusal rather than a half-open connection. A
        // `Bye` is not answered with a `Bye`, which is what stops this looping.
        if (!makeRoom()) {
          sendBye(from, msg.epoch, BYE_REPLACED);
          return;
        }
      } else {
        // An answer, candidate or bye for a pair we are not in: a race after a
        // bye, or a message the room carried under a dropped pair's grace (§9).
        // Not a misdelivery, so it is counted apart from `ignored`.
        window.__bt.unknown += 1;
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
    } else if (msg.tag === TAG_RESTART) {
      onRestart(link, msg);
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
    if (me === seat || pendingJoin === seat) {
      return false;
    }
    if (!iceSettled) {
      // A connection negotiated now would never learn about TURN, and a link is
      // only negotiated once. Hold the seat until the list settles — at once,
      // unless the room is still minting.
      pendingJoin = seat;
      iceReady.then(function () {
        if (pendingJoin === seat) {
          pendingJoin = null;
          join(seat);
        }
      });
      return true;
    }
    pendingJoin = null;
    return join(seat);
  };

  // The room's interest set for this seat, and the epoch that names it. Called
  // when the set changes — on the room's regroup interval, and at once after a
  // join. Links outside the set are told `Bye` and forgotten; a seat the set
  // has gained is offered a channel from here, by whichever end leads.
  //
  // Offering on the group rather than on an announce is what makes the set
  // self-starting. An announce can be dropped in flight by the far side's group
  // filter — it arrives between the room deciding the set and that page
  // applying it — and the `seen` guard then suppresses the echo that would
  // recover it, which left two peers the room names permanently unlinked until
  // the set changed again. The group is the room's statement that a channel is
  // worth having, so the leader opens it without waiting to be announced at.
  // The announce stays as the nudge for a page whose group has not arrived.
  window.__btGroup = function (epoch, members) {
    if (group !== null && epoch <= group.epoch) {
      return false;
    }
    group = { epoch: epoch, members: new Set(members) };
    window.__bt.group = { epoch: epoch, members: members.slice() };
    // A seat that left has no business holding an offer slot either.
    queue = queue.filter(function (seat) {
      return group.members.has(seat);
    });
    var leaving = [];
    links.forEach(function (link, seat) {
      if (group.members.has(seat)) {
        return;
      }
      if (PERSIST) {
        // Persistent links (design §5): the channel stays, dormant, so walking
        // back into view costs no handshake. Nothing is sent on it and nothing
        // arriving on it reaches the game while the set does not name the seat.
        setIdle(link);
      } else {
        leaving.push(link);
      }
    });
    for (var i = 0; i < leaving.length; i += 1) {
      var link = leaving[i];
      sendBye(link.seat, link.myEpoch, BYE_REPLACED);
      note(link.seat, 'left the group');
      reset(link, 'left the group');
      links.delete(link.seat);
      seen.delete(link.seat);
    }
    if (me !== null && group.members.size > 0) {
      announce();
      group.members.forEach(function (seat) {
        var held = links.get(seat);
        if (held !== undefined) {
          var wasIdle = held.idle;
          setLive(held);
          if (wasIdle && PERSIST && !isHealthy(held)) {
            // It died while the set did not name it, so nothing noticed. Forget
            // it and let this group start a fresh negotiation. A link that is
            // still connecting is not stale — it has not finished its first
            // handshake, and tearing it down here would restart every slow
            // negotiation once per regroup.
            reset(held, 'stale on return');
            links.delete(seat);
            seen.delete(seat);
            held = undefined;
          }
        }
        if (held === undefined && leadsOffer(seat) && linkFor(seat) !== null) {
          queueOffer(seat);
        }
      });
    }
    // An offer that arrived before this set did, from a seat the set now names.
    // `deliver` re-checks the filter, so the replay is refused again for any
    // seat the set still does not name.
    if (deferred.length > 0) {
      var replay = deferred.filter(function (item) {
        return group.members.has(item.from) || links.has(item.from);
      });
      deferred = deferred.filter(function (item) {
        return !(group.members.has(item.from) || links.has(item.from));
      });
      var told = {};
      for (var d = 0; d < replay.length; d += 1) {
        if (!told[replay[d].from]) {
          told[replay[d].from] = true;
          note(replay[d].from, 'signals replayed: the set names it now');
        }
        deliver(replay[d].from, replay[d].bytes);
      }
    }
    rollup();
    return true;
  };

  function join(seat) {
    // A seat change is a new session: whatever the room said about the old one
    // describes a set that may not be this seat's, and a room that restarted
    // begins its epochs again.
    group = null;
    window.__bt.group = null;
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
      queue = [];
      epochs.clear();
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
  }

  // ── Path quality ─────────────────────────────────────────────────────────
  //
  // Candidate-pair round trip per link, sampled once a second from the
  // browser's own stats. It is the one number that says whether a peer is next
  // door or on another continent — the stream is 20 Hz either way — and §12.2 of
  // the design turns on it.
  //
  // Sampled rather than asked for on demand because `getStats` is a promise: a
  // report taken at the moment of an assertion could only be awaited by the
  // caller, and the game's frame loop cannot wait a frame for telemetry.
  var PATH_EVERY_MS = 1000;
  var lastPath = 0;

  function samplePaths(now) {
    if (now - lastPath < PATH_EVERY_MS) {
      return;
    }
    lastPath = now;
    links.forEach(function (link) {
      // One report in flight per link: a slow report must not queue another.
      if (link.pc === null || link.sampling) {
        return;
      }
      link.sampling = true;
      link.pc.getStats().then(function (report) {
        link.sampling = false;
        var best = null;
        report.forEach(function (stat) {
          if (stat.type !== 'candidate-pair' || stat.state !== 'succeeded') {
            return;
          }
          // Chrome marks the pair in use `selected`; older builds and other
          // browsers use `nominated`. Take the most marked, then the one that
          // has actually carried bytes.
          var score = (stat.selected ? 2 : 0) + (stat.nominated ? 1 : 0);
          if (best === null || score > best.score ||
              (score === best.score &&
               (stat.bytesSent || 0) > (best.stat.bytesSent || 0))) {
            best = { score: score, stat: stat };
          }
        });
        if (best === null) {
          return;
        }
        var pair = best.stat;
        if (typeof pair.currentRoundTripTime === 'number') {
          // Tenths of a millisecond: loopback reads 0.1–0.5, and rounding to
          // whole milliseconds would call every local pair zero.
          link.rttMs = Math.round(pair.currentRoundTripTime * 10000) / 10;
        }
        link.local = pairCandidateType(report, pair.localCandidateId);
        link.remote = pairCandidateType(report, pair.remoteCandidateId);
      }).catch(function () {
        // The connection went away between the call and the report, which its
        // own state already says. A rejected sample is not an error.
        link.sampling = false;
      });
    });
  }

  // `host`, `srflx`, `prflx` or `relay`: whether this end of a pair is a local
  // address, a mapped one, or a TURN relay. A `relay` on either end explains a
  // round trip that is nowhere near the two players' distance, and is what
  // `p2p-turn.dsl` proves.
  //
  // Named apart from `candidateType` above on purpose: this file is one
  // function scope, so a second declaration of that name silently replaced the
  // counter's and every gathered candidate was counted as `null`.
  function pairCandidateType(report, id) {
    if (!id) {
      return null;
    }
    var candidate = report.get(id);
    return candidate && candidate.candidateType ? candidate.candidateType : null;
  }

  // Whatever has arrived since the last drain, handed to the links. Called by
  // `probe_peer_out` once a frame. The sweep runs after the inbox: an answer
  // that arrives on the frame an offer would have expired is worth applying, so
  // the deadline is judged against the link's state once this frame's signals
  // have had their say.
  window.__btDrain = function () {
    var inbox = window.__btInbox;
    if (inbox && inbox.length > 0) {
      var take = inbox.splice(0, inbox.length);
      for (var i = 0; i < take.length; i += 1) {
        deliver(take[i].from, take[i].bytes);
      }
    }
    var now = performance.now();
    if (deferred.length > 0) {
      deferred = deferred.filter(function (item) {
        return now - item.at < DEFER_MS;
      });
    }
    sweepStale(now);
    sweepOffers(now);
    samplePaths(now);
    pumpQueue();
  };

  // ── The state and control channels ───────────────────────────────────────
  //
  // What the game puts on a data channel, and what arrived on one. The shim
  // never parses either — `island-wire` owns the format.
  window.__btSendLink = function (seat, kind, bytes) {
    var link = links.get(seat);
    if (!link) {
      refuse(seat, kind, 'no link');
      return false;
    }
    if (link.idle) {
      refuse(seat, kind, 'dormant');
      return false;
    }
    var channel = kind === LINK_STATE ? link.state : link.ctrl;
    if (!channel || channel.readyState !== 'open') {
      refuse(seat, kind, 'closed');
      return false;
    }
    channel.send(bytes);
    if (kind === LINK_STATE) {
      window.__bt.sentState += 1;
      link.sentState += 1;
    } else {
      window.__bt.sentCtrl += 1;
      link.sentCtrl += 1;
    }
    return true;
  };

  // Why the game's byte did not go. A link whose state flow is one-way is not a
  // connection as far as the overlay is concerned, and "sent 0" cannot tell a
  // missing link from a channel that has not opened.
  function refuse(seat, kind, why) {
    var key = seat + ':' + (kind === LINK_STATE ? 'state' : 'ctrl') + ':' + why;
    window.__bt.refused[key] = (window.__bt.refused[key] || 0) + 1;
  }

  // Whatever arrived on the data channels since the last drain, oldest first.
  // The per-link counters in `links` are refreshed here, because the drain is
  // the one thing that runs every frame: a link whose traffic changed but whose
  // connection state did not would otherwise report the counts it had at open.
  window.__btDrainLinks = function () {
    var out = recv;
    recv = [];
    rollup();
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
  // two-network test should look at when a link does not open. A relay-only run
  // (`?relay=1`) never gathers `srflx` and is still fine: `state: 'gathered'`
  // with `types.relay > 0` is what that run asserts.
  //
  // The result is left on `window.__bt.iceProbe` as well as resolved, so a
  // harness that cannot await a promise can poll it.
  window.__btIceProbe = function () {
    var result = {
      state: 'gathering',
      source: ICE.source,
      room: ICE.room,
      relay: RELAY,
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
      pc = new RTCPeerConnection(RELAY
        ? { iceServers: ICE.servers, iceTransportPolicy: 'relay' }
        : { iceServers: ICE.servers });
    } catch (e) {
      result.state = 'error';
      result.error = describe(e);
      return Promise.resolve(result);
    }
    return new Promise(function (resolve) {
      var settled = false;
      // A candidate that proves something outside this machine: a relay
      // allocation, or a STUN server answering. Host candidates are gathered
      // with no server at all, so they do not count.
      var gathered = false;
      var timer = setTimeout(function () {
        done(result.srflx || gathered ? 'gathered' : 'timeout');
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
        if (type === 'srflx' || type === 'relay') {
          gathered = true;
        }
        if (type === 'srflx' && !result.srflx) {
          result.srflx = candidateAddress(e.candidate);
        }
      };
      pc.onicegatheringstatechange = function () {
        if (pc.iceGatheringState === 'complete') {
          done(result.srflx ? 'complete' : (gathered ? 'gathered' : 'no-srflx'));
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

  // Ask for an ICE restart on a live link, as the `disconnected` handler does on
  // its own. A harness seam like `__btDrop`: a path that goes quiet without
  // closing its channels cannot be staged from a page, so this is how
  // `p2p-ice-restart.dsl` exercises the signalling and the answer that keeps the
  // channels.
  window.__btRestart = function (seat) {
    var link = links.get(seat);
    if (!link) {
      return false;
    }
    return startRestart(link);
  };

  // Simulate the far side's browser going away without a `Bye`: close this
  // link's transport and ignore its own events until something renegotiates it.
  // `__btClose` is the clean shutdown; this is the crash a liveness test needs,
  // and it is a harness seam like the rest.
  window.__btDrop = function (seat) {
    var link = links.get(seat);
    if (!link || !link.pc) {
      return false;
    }
    link.silent = true;
    try {
      link.pc.close();
    } catch (e) {
      /* already gone */
    }
    return true;
  };

  // Close every connection, so a run leaves nothing behind.
  window.__btClose = function () {
    links.forEach(function (link) {
      reset(link, 'closed by the page');
    });
    links.clear();
    seen.clear();
    queue = [];
    window.__bt.state = 'closed';
    return Promise.resolve(true);
  };
})();
