//! The `stories!` macro — group membership, order and dispatch, declared once.
//!
//! # What was actually broken
//!
//! The registry was seven lists of the same 129 things across ~1,500 of
//! `lib.rs`'s 1,706 lines. But they are **not equally dangerous**, and that is
//! what decides the macro's scope:
//!
//! | list | failure mode |
//! |---|---|
//! | `label()`, `description()`, dispatch | `match self { … }` with no `_` arm, so **the compiler catches an omission** |
//! | `all()` | a variant present in the `enum` but missing here **silently vanishes from the sidebar** |
//! | `all()` order vs `category()` | disagree and the sidebar emits a **duplicate or misplaced heading** |
//!
//! Both live bugs were in the second pair:
//!
//! - Two groups were both named `"Wallet"`, separated in `all()` order by the
//!   "Trade Desk" run. The sidebar only compares against the *previous*
//!   category, so the `"Wallet"` heading rendered **twice**.
//! - `ListingGrid` reported `"TX Cart"` while sitting in the Data-Visualization
//!   run, so it appeared under the wrong heading.
//! - The `// Primitives` comment in `all()` spanned ~100 entries while
//!   `category()` assigned only 56 — the comments had gone stale and nothing
//!   could notice.
//!
//! So this macro owns exactly the unsafe part: **group membership and order are
//! one fact, written once.** A group is a contiguous block by construction, so
//! neither bug is expressible. `label()` and `description()` stay as they are —
//! verbose, but the compiler already guards them, and rewriting 129 prose
//! strings would risk pairing one with the wrong story for no safety gain.
//!
//! # Why the dispatch body is a closure
//!
//! The tidier DSL — `disclosure [disclosure_state]`, macro assembles the call —
//! does not work here:
//!
//! 1. **`macro_rules!` hygiene.** Identifiers the macro body introduces (`app`,
//!    `ui`) are invisible to tokens passed in from the call site, so a bare
//!    `$call:expr` mentioning `app` would not resolve. A closure declares its own
//!    parameter names *at the call site*, sidestepping hygiene entirely.
//! 2. **The arities vary.** Most stories take `ui` alone; 21 take `ui` plus one
//!    or more `&mut app.field`; at least one wants a cloned `Context` too. A
//!    rigid DSL needs a special case per shape. A closure needs none.
//!
//! # Shape
//!
//! ```ignore
//! stories! {
//!     enum Story for StorybookApp;
//!
//!     group "Primitives" {
//!         Formatting => |_app, ui| stories::formatting::show(ui);
//!         Disclosure => |app, ui| stories::disclosure::show(ui, &mut app.disclosure_state);
//!     }
//! }
//! ```

/// Declare the story registry. See the module docs for the why.
///
/// `allow(unused_macros)`: the only non-test consumer is `mod app`, which is
/// `#[cfg(target_arch = "wasm32")]`. A plain native `cargo check` compiles
/// neither that module nor the tests below, so the macro looks dead on exactly
/// one of the three build configurations it serves.
#[allow(unused_macros)]
macro_rules! stories {
    (
        enum $enum:ident for $app:ty;

        $(
            group $group:literal {
                $( $variant:ident => $body:expr ; )*
            }
        )*
    ) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum $enum {
            $( $( $variant, )* )*
        }

        impl $enum {
            /// Every story in sidebar order: groups in declaration order, stories
            /// in declaration order within each group.
            ///
            /// This IS the ordering — there is no second list to keep in step,
            /// and a variant cannot be omitted because the `enum` is generated
            /// from the same tokens.
            pub fn all() -> &'static [Self] {
                &[ $( $( Self::$variant, )* )* ]
            }

            /// The sidebar heading this story sits under.
            ///
            /// Derived from the group it was declared in, so it cannot disagree
            /// with [`Self::all`]'s ordering — which is what produced the
            /// duplicate `"Wallet"` heading before.
            pub fn category(&self) -> &'static str {
                match self { $( $( Self::$variant => $group, )* )* }
            }

            /// Every group heading, in declaration order, without duplicates.
            pub fn categories() -> Vec<&'static str> {
                let mut out: Vec<&'static str> = Vec::new();
                $(
                    if !out.contains(&$group) {
                        out.push($group);
                    }
                )*
                out
            }

            /// URL-safe identifier for deep-linking a story: `#/party-badge`.
            ///
            /// Derived from the label rather than hand-maintained, so a new story
            /// is addressable the moment it has a name.
            pub fn slug(&self) -> String {
                self.label()
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect::<String>()
                    .split('-')
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
                    .join("-")
            }

            /// Resolve a slug back to a story, ignoring any leading `#` / `#/`.
            pub fn from_slug(raw: &str) -> Option<Self> {
                let want = raw.trim_start_matches('#').trim_start_matches('/');
                if want.is_empty() {
                    return None;
                }
                Self::all().iter().find(|s| s.slug() == want).copied()
            }

            /// Render this story.
            ///
            /// The per-story closure is applied rather than inlined, which is what
            /// lets each story name its own state without the macro knowing
            /// anything about the app's fields.
            #[allow(unused_variables)]
            pub fn draw(&self, app: &mut $app, ui: &mut egui::Ui) {
                match self {
                    $( $(
                        Self::$variant => {
                            let body = $body;
                            body(app, ui);
                        }
                    )* )*
                }
            }

            /// Walk `all()` the way the sidebar does, emitting a heading whenever
            /// the category changes.
            ///
            /// Shared with the sidebar so the invariant test below checks the real
            /// walk rather than a copy of it.
            pub fn headings_in_walk_order() -> Vec<&'static str> {
                let mut out = Vec::new();
                let mut current = "";
                for story in Self::all() {
                    if story.category() != current {
                        current = story.category();
                        out.push(current);
                    }
                }
                out
            }
        }
    };
}

#[cfg(test)]
mod tests {
    /// A stand-in app with one piece of per-story state, so the test exercises
    /// both the stateless and the stateful closure shapes.
    ///
    /// `pub` for the same reason `StorybookApp` is: the macro generates a
    /// `pub fn draw(&self, app: &mut $app, …)`, so the app type is part of a
    /// public signature whether or not anything outside can name it.
    #[derive(Default)]
    pub struct DemoApp {
        counter: u32,
    }

    stories! {
        enum DemoStory for DemoApp;

        group "First" {
            Alpha => |_app: &mut DemoApp, _ui: &mut egui::Ui| {};
            Beta => |app: &mut DemoApp, _ui: &mut egui::Ui| { app.counter += 1; };
        }

        group "Second" {
            Gamma => |_app: &mut DemoApp, _ui: &mut egui::Ui| {};
        }
    }

    /// Hand-written, exactly as the real registry keeps them: an exhaustive match
    /// the compiler guards.
    impl DemoStory {
        fn label(&self) -> &'static str {
            match self {
                Self::Alpha => "Alpha",
                Self::Beta => "Beta Two",
                Self::Gamma => "Gamma",
            }
        }
    }

    #[test]
    fn all_is_declaration_order_across_groups() {
        assert_eq!(
            DemoStory::all(),
            &[DemoStory::Alpha, DemoStory::Beta, DemoStory::Gamma]
        );
    }

    /// A story's heading is its declared group, so ordering and category cannot
    /// drift apart.
    #[test]
    fn category_follows_the_declared_group() {
        assert_eq!(DemoStory::Alpha.category(), "First");
        assert_eq!(DemoStory::Beta.category(), "First");
        assert_eq!(DemoStory::Gamma.category(), "Second");
    }

    /// The regression test for the duplicate `"Wallet"` heading: walking `all()`
    /// and emitting on change must yield each group exactly once, which only
    /// holds if groups are contiguous.
    #[test]
    fn each_heading_appears_exactly_once_in_the_walk() {
        assert_eq!(DemoStory::headings_in_walk_order(), vec!["First", "Second"]);
        assert_eq!(
            DemoStory::headings_in_walk_order(),
            DemoStory::categories(),
            "a group is split across the list, so its heading repeats"
        );
    }

    #[test]
    fn slugs_are_url_safe_and_round_trip() {
        assert_eq!(DemoStory::Beta.slug(), "beta-two");
        assert_eq!(DemoStory::from_slug("beta-two"), Some(DemoStory::Beta));
        assert_eq!(DemoStory::from_slug("#/beta-two"), Some(DemoStory::Beta));
        assert_eq!(DemoStory::from_slug("#beta-two"), Some(DemoStory::Beta));
        assert_eq!(DemoStory::from_slug(""), None);
        assert_eq!(DemoStory::from_slug("nope"), None);
    }

    /// Every story must be addressable and uniquely so, or a deep link silently
    /// lands on the wrong widget.
    #[test]
    fn every_slug_is_unique() {
        let mut slugs: Vec<String> = DemoStory::all().iter().map(|s| s.slug()).collect();
        let before = slugs.len();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), before, "two stories share a slug");
    }

    /// The closure really is applied, and it really does reach the app's state —
    /// the half a tidier DSL could not express.
    #[test]
    fn drawing_runs_the_storys_own_closure_against_app_state() {
        let mut app = DemoApp::default();
        egui::__run_test_ui(|ui| {
            DemoStory::Alpha.draw(&mut app, ui);
            assert_eq!(app.counter, 0, "a stateless story must not touch state");
            DemoStory::Beta.draw(&mut app, ui);
            DemoStory::Beta.draw(&mut app, ui);
            assert_eq!(app.counter, 2, "the stateful closure did not run");
        });
    }
}
