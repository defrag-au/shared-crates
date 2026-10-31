//! `Router` — the app's route kept in the page's hash, so a reload, a shared
//! link and the phone's back gesture all land where the reader expects.
//!
//! The app names its screens with a [`Route`] (its own enum, parsed from and
//! written to the hash) and moves with [`Router::push`] and
//! [`Router::replace`]. Once a frame, [`Router::poll`] reports a move the
//! browser made by itself (back, forward, a typed address), and the app shows
//! that route.
//!
//! The router also keeps the stack of routes visited this session, so an app
//! bar's back arrow can ask [`Router::up`] to go back when going back stays
//! inside the app, and to replace the route with the parent when the reader
//! arrived by a link and back would leave.
//!
//! Natively there is no page: the history is the same stack, in memory,
//! `--route <hash>` opens a deep screen for a capture, and [`Router::back`]
//! moves within the stack and is reported by the next [`Router::poll`].

/// A screen of the app, as the page's hash spells it (without the `#`).
pub trait Route: Clone + PartialEq {
    /// The route a hash names. Every hash names one: an unknown hash is the
    /// app's home, or its own "not found".
    fn parse(hash: &str) -> Self;
    /// The hash that names this route.
    fn to_hash(&self) -> String;
}

#[cfg(target_arch = "wasm32")]
mod page {
    use sapp_jsutils::JsObject;

    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn app_hash() -> JsObject;
        fn app_history_push(hash: JsObject);
        fn app_history_replace(hash: JsObject);
        fn app_history_back();
        fn app_history_pops() -> u32;
    }

    pub fn hash() -> String {
        let mut s = String::new();
        unsafe { app_hash() }.to_string(&mut s);
        s
    }

    pub fn push(hash: &str) {
        unsafe { app_history_push(JsObject::string(hash)) }
    }

    pub fn replace(hash: &str) {
        unsafe { app_history_replace(JsObject::string(hash)) }
    }

    pub fn back() {
        unsafe { app_history_back() }
    }

    pub fn pops() -> u32 {
        unsafe { app_history_pops() }
    }
}

/// See the [module docs](self).
#[derive(Debug)]
pub struct Router<R: Route> {
    current: R,
    /// Hashes visited this session, oldest first, and where in them the
    /// reader is: a back move steps down it, a forward move up.
    stack: Vec<String>,
    at: usize,
    /// The browser's move count when last polled.
    #[cfg(target_arch = "wasm32")]
    seen_pops: u32,
    /// A native back move, reported by the next poll as the browser's is.
    #[cfg(not(target_arch = "wasm32"))]
    moved: bool,
}

impl<R: Route> Router<R> {
    /// The route the page was opened at (natively, `--route`, else the empty
    /// hash).
    pub fn new() -> Self {
        #[cfg(target_arch = "wasm32")]
        let hash = page::hash();
        #[cfg(not(target_arch = "wasm32"))]
        let hash = crate::flag("--route").unwrap_or_default();
        Self::at(&hash)
    }

    /// A router opened at `hash`, with nothing behind it.
    pub fn at(hash: &str) -> Self {
        let current = R::parse(hash);
        Self {
            stack: vec![current.to_hash()],
            current,
            at: 0,
            #[cfg(target_arch = "wasm32")]
            seen_pops: page::pops(),
            #[cfg(not(target_arch = "wasm32"))]
            moved: false,
        }
    }

    /// The route on screen.
    pub fn current(&self) -> &R {
        &self.current
    }

    /// Whether a back move stays inside the app: the reader got here from
    /// another of its screens this session.
    pub fn can_go_back(&self) -> bool {
        self.at > 0
    }

    /// Go to `route`, behind which the current one waits for the back button.
    /// Going where the reader already is does nothing.
    pub fn push(&mut self, route: R) {
        if route == self.current {
            return;
        }
        let hash = route.to_hash();
        self.stack.truncate(self.at + 1);
        self.stack.push(hash.clone());
        self.at = self.stack.len() - 1;
        self.current = route;
        #[cfg(target_arch = "wasm32")]
        page::push(&hash);
    }

    /// Show `route` in place of the current one, with no new step in the
    /// history: a filter changed, a sheet's detail swapped.
    pub fn replace(&mut self, route: R) {
        if route == self.current {
            return;
        }
        let hash = route.to_hash();
        self.stack[self.at] = hash.clone();
        self.current = route;
        #[cfg(target_arch = "wasm32")]
        page::replace(&hash);
    }

    /// The browser's back button, from inside the app. The move is reported
    /// by the next [`Router::poll`], as one the browser made itself is.
    pub fn back(&mut self) {
        #[cfg(target_arch = "wasm32")]
        page::back();
        #[cfg(not(target_arch = "wasm32"))]
        if self.at > 0 {
            self.at -= 1;
            self.moved = true;
        }
    }

    /// An app bar's back arrow: back, when that stays inside the app;
    /// otherwise `parent` in place of where the reader is, so the arrow never
    /// leaves the app for whatever page linked here.
    pub fn up(&mut self, parent: R) {
        if self.can_go_back() {
            self.back();
        } else {
            self.replace(parent);
        }
    }

    /// Call once a frame, before laying out. `Some` with the new route when
    /// the browser moved (back, forward, a typed address) since the last call.
    pub fn poll(&mut self) -> Option<&R> {
        #[cfg(target_arch = "wasm32")]
        {
            let pops = page::pops();
            if pops == self.seen_pops {
                return None;
            }
            self.seen_pops = pops;
            let hash = page::hash();
            self.arrived(&hash);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !std::mem::take(&mut self.moved) {
                return None;
            }
            self.current = R::parse(&self.stack[self.at]);
        }
        Some(&self.current)
    }

    /// The browser now shows `hash`: a step back or forward in the stack when
    /// it matches one, else a new place (a typed address) after this one.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn arrived(&mut self, hash: &str) {
        if self.at > 0 && self.stack[self.at - 1] == hash {
            self.at -= 1;
        } else if self.stack.get(self.at + 1).is_some_and(|h| h == hash) {
            self.at += 1;
        } else if self.stack[self.at] != hash {
            self.stack.truncate(self.at + 1);
            self.stack.push(hash.to_string());
            self.at = self.stack.len() - 1;
        }
        self.current = R::parse(hash);
    }
}

impl<R: Route> Default for Router<R> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    enum Screen {
        Home,
        Item(u32),
    }

    impl Route for Screen {
        fn parse(hash: &str) -> Self {
            match hash.strip_prefix("/item/").and_then(|n| n.parse().ok()) {
                Some(n) => Screen::Item(n),
                None => Screen::Home,
            }
        }

        fn to_hash(&self) -> String {
            match self {
                Screen::Home => "/".to_string(),
                Screen::Item(n) => format!("/item/{n}"),
            }
        }
    }

    #[test]
    fn an_unknown_hash_is_home() {
        let r: Router<Screen> = Router::at("nonsense");
        assert_eq!(r.current(), &Screen::Home);
        assert!(!r.can_go_back());
    }

    #[test]
    fn back_returns_to_where_the_reader_came_from() {
        let mut r: Router<Screen> = Router::at("/");
        r.push(Screen::Item(3));
        r.push(Screen::Item(4));
        assert!(r.can_go_back());
        r.back();
        assert_eq!(r.poll(), Some(&Screen::Item(3)));
        assert_eq!(r.poll(), None, "reported once");
        r.back();
        assert_eq!(r.poll(), Some(&Screen::Home));
        assert!(!r.can_go_back());
    }

    #[test]
    fn up_never_leaves_the_app() {
        // Opened from a shared link straight onto an item.
        let mut r: Router<Screen> = Router::at("/item/9");
        r.up(Screen::Home);
        assert_eq!(r.current(), &Screen::Home, "replaced, not gone back");
        assert!(!r.can_go_back());
        // Arrived from home this time: up goes back.
        r.push(Screen::Item(9));
        r.up(Screen::Home);
        assert_eq!(r.poll(), Some(&Screen::Home));
    }

    #[test]
    fn replace_takes_no_step() {
        let mut r: Router<Screen> = Router::at("/");
        r.push(Screen::Item(1));
        r.replace(Screen::Item(2));
        r.back();
        assert_eq!(r.poll(), Some(&Screen::Home));
    }

    #[test]
    fn pushing_where_the_reader_is_does_nothing() {
        let mut r: Router<Screen> = Router::at("/item/1");
        r.push(Screen::Item(1));
        assert!(!r.can_go_back());
    }

    #[test]
    fn the_browsers_own_moves_walk_the_stack() {
        let mut r: Router<Screen> = Router::at("/");
        r.push(Screen::Item(1));
        r.push(Screen::Item(2));
        r.arrived("/item/1");
        assert_eq!(r.current(), &Screen::Item(1));
        assert!(r.can_go_back());
        r.arrived("/item/2");
        assert_eq!(r.current(), &Screen::Item(2), "forward");
        // A typed address is a new place after this one.
        r.arrived("/item/7");
        assert_eq!(r.current(), &Screen::Item(7));
        r.arrived("/item/2");
        assert_eq!(r.current(), &Screen::Item(2), "and back from it");
    }
}
