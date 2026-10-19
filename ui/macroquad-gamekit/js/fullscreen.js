// gamekit-fullscreen.js — take a phone or tablet game fullscreen on its first
// tap. Stamped by a game's build.rs (`macroquad_gamekit::web::PageScript`).
//
// Browsers grant fullscreen only from inside a user gesture's handler, so
// macroquad's `set_fullscreen`, which runs from the frame loop, is refused on
// the web. This listens for the first tap instead and asks from there.
//
// - Touch devices only (`pointer: coarse`): on a desktop, a click going
//   fullscreen would be a surprise.
// - Not inside an iframe (a Discord Activity): the embedder owns the frame.
// - Where the browser has no element fullscreen (an iPhone's Safari allows it
//   only for video), this does nothing: there, Add to Home Screen launches
//   the game without the browser's bars, from the page's web app manifest.
// - After the player leaves fullscreen, the next tap asks again: a game wants
//   the whole screen back.
(function () {
    if (window.self !== window.top) return;
    if (!window.matchMedia || !window.matchMedia("(pointer: coarse)").matches) return;
    var el = document.documentElement;
    var request = el.requestFullscreen || el.webkitRequestFullscreen;
    if (!request) return;

    function full() {
        return document.fullscreenElement || document.webkitFullscreenElement;
    }

    function go() {
        if (full()) return;
        try {
            var asked = request.call(el, { navigationUI: "hide" });
            if (asked && asked.catch) asked.catch(function () {});
        } catch (e) {
            // A browser that throws rather than rejecting: stay windowed.
        }
    }

    document.addEventListener("touchend", go, { passive: true });
    document.addEventListener("click", go, { passive: true });
})();
