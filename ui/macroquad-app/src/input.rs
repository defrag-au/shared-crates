//! `TextField` — a single-line field the reader types into, with the phone's
//! own keyboard: its autocorrect, its paste, its IME and its Search key.
//!
//! On the web the canvas cannot take text, so while a field is being edited a
//! real `<input>` sits exactly over the field's text box (`js/app.js`): the
//! canvas draws the field's chrome around it, and the page draws the text and
//! the caret. A phone opens its keyboard only for a focus a tap made, so the
//! field names its box to the page every frame, and the page focuses the input
//! from inside the tap's own handler. Natively the field reads the keyboard
//! itself, so an app is driven, captured and tested with no browser.
//!
//! One [`Keyboard`] per app, around the frame's fields:
//!
//! ```ignore
//! self.keyboard.begin(frame);
//! let reply = self.search.show(&p, &mut self.keyboard, rect, FieldKind::Search, "Search by name");
//! if reply.changed { self.refilter(); }
//! self.keyboard.end();
//! ```
//!
//! While editing, the page's input owns the text; the field reads it back each
//! frame. [`TextField::set`] replaces it on purpose (a clear button, a value
//! restored from the route).

use macroquad::prelude::*;
use macroquad_gamekit::Frame;
use macroquad_gamekit::clip::with_clip;
use macroquad_widgets::{Align, Painter, fill_rounded, stroke_rounded};
use ui_theme::TextSize;

use crate::glyph::{Glyph, draw_glyph};

/// What the field holds, which sets the phone's keyboard up for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Prose: capitals and autocorrect as the phone sees fit.
    Text,
    /// A search: a lens in the field, and the keyboard's key says Search.
    Search,
    /// An address, a handle or an id: no capitals, no autocorrect.
    Address,
}

impl FieldKind {
    /// The code `js/app.js` reads.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn code(self) -> u32 {
        match self {
            FieldKind::Text => 0,
            FieldKind::Search => 1,
            FieldKind::Address => 2,
        }
    }
}

/// What happened to a field this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FieldReply {
    /// The text is different from last frame's.
    pub changed: bool,
    /// The reader pressed Enter (the keyboard's Search or Done).
    pub submitted: bool,
    /// The field is being edited.
    pub editing: bool,
}

#[cfg(target_arch = "wasm32")]
mod page {
    use sapp_jsutils::JsObject;

    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn app_input_zones_clear();
        #[allow(clippy::too_many_arguments)]
        fn app_input_zone(
            id: u32,
            x0: f32,
            y0: f32,
            x1: f32,
            y1: f32,
            value: JsObject,
            placeholder: JsObject,
            kind: u32,
            font: f32,
            color: JsObject,
        );
        fn app_input_zones_done();
        fn app_input_focused() -> i32;
        fn app_input_value() -> JsObject;
        fn app_input_set(value: JsObject);
        fn app_input_submits() -> u32;
        fn app_input_blur();
    }

    pub fn clear() {
        unsafe { app_input_zones_clear() }
    }

    pub struct Zone<'a> {
        pub id: u32,
        pub frac: [f32; 4],
        pub value: &'a str,
        pub placeholder: &'a str,
        pub kind: u32,
        pub font: f32,
        pub color: &'a str,
    }

    pub fn zone(z: Zone) {
        let [x0, y0, x1, y1] = z.frac;
        unsafe {
            app_input_zone(
                z.id,
                x0,
                y0,
                x1,
                y1,
                JsObject::string(z.value),
                JsObject::string(z.placeholder),
                z.kind,
                z.font,
                JsObject::string(z.color),
            )
        }
    }

    pub fn done() {
        unsafe { app_input_zones_done() }
    }

    pub fn focused() -> Option<u32> {
        u32::try_from(unsafe { app_input_focused() }).ok()
    }

    pub fn value() -> String {
        let mut s = String::new();
        unsafe { app_input_value() }.to_string(&mut s);
        s
    }

    pub fn set(value: &str) {
        unsafe { app_input_set(JsObject::string(value)) }
    }

    pub fn submits() -> u32 {
        unsafe { app_input_submits() }
    }

    pub fn blur() {
        unsafe { app_input_blur() }
    }
}

/// The app's one keyboard: which field is being edited, and this frame's
/// keys. See the [module docs](self).
#[derive(Debug, Default)]
pub struct Keyboard {
    /// The field being edited this frame.
    focused: Option<u32>,
    /// The field that was being edited last frame: an Enter is its.
    was_focused: Option<u32>,
    /// Enter was pressed since last frame.
    submitted: bool,
    /// Window points per stage unit, and the stage's origin in the window:
    /// how a field's rect becomes a box on the page.
    u: f32,
    origin: Vec2,
    #[cfg(target_arch = "wasm32")]
    seen_submits: u32,
    /// Native: this frame's typing.
    #[cfg(not(target_arch = "wasm32"))]
    typed: Typed,
}

/// Native: what the keyboard did this frame.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default)]
struct Typed {
    text: String,
    backspaces: u32,
    escape: bool,
}

impl Keyboard {
    pub fn new() -> Self {
        Self {
            u: 1.0,
            #[cfg(target_arch = "wasm32")]
            seen_submits: page::submits(),
            ..Self::default()
        }
    }

    /// Call once a frame, before drawing any field.
    pub fn begin(&mut self, frame: &Frame) {
        self.u = frame.viewport.u;
        self.origin = frame.origin;
        self.was_focused = self.focused;
        #[cfg(target_arch = "wasm32")]
        {
            page::clear();
            self.focused = page::focused();
            let submits = page::submits();
            self.submitted = submits != self.seen_submits;
            self.seen_submits = submits;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.typed = Typed::default();
            self.submitted = false;
            while let Some(ch) = get_char_pressed() {
                if !ch.is_control() {
                    self.typed.text.push(ch);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                self.typed.backspaces += 1;
            }
            if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                self.submitted = true;
            }
            self.typed.escape = is_key_pressed(KeyCode::Escape);
            let command = is_key_down(KeyCode::LeftControl)
                || is_key_down(KeyCode::RightControl)
                || is_key_down(KeyCode::LeftSuper)
                || is_key_down(KeyCode::RightSuper);
            if command && is_key_pressed(KeyCode::V) {
                // The `v` the chord typed is not text.
                self.typed.text.retain(|c| c != 'v' && c != 'V');
                if let Some(pasted) = macroquad::miniquad::window::clipboard_get() {
                    self.typed
                        .text
                        .extend(pasted.chars().filter(|c| !c.is_control()));
                }
            }
        }
    }

    /// Call once a frame, after the last field.
    pub fn end(&mut self) {
        #[cfg(target_arch = "wasm32")]
        page::done();
    }

    /// Whether any field is being edited: a screen keeps its own key
    /// bindings off while one is.
    pub fn editing(&self) -> bool {
        self.focused.is_some()
    }

    /// Stop editing, and put the phone's keyboard away.
    pub fn dismiss(&mut self) {
        #[cfg(target_arch = "wasm32")]
        page::blur();
        self.focused = None;
    }

    /// `rect` (stage units) as fractions of the canvas, as the page places
    /// its input.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn fractions(&self, rect: Rect) -> [f32; 4] {
        let screen = vec2(screen_width(), screen_height()).max(Vec2::ONE);
        let a = (rect.point() * self.u + self.origin) / screen;
        let b = ((rect.point() + rect.size()) * self.u + self.origin) / screen;
        [a.x, a.y, b.x, b.y]
    }
}

/// See the [module docs](self).
#[derive(Clone, Debug, Default)]
pub struct TextField {
    id: u32,
    value: String,
}

/// The field's height, in points: a finger's minimum.
pub const FIELD_H: f32 = 44.0;

impl TextField {
    /// A field known to the page as `id`: unique among the fields on screen
    /// at once.
    pub fn new(id: u32) -> Self {
        Self {
            id,
            value: String::new(),
        }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// Replace the text, including under the reader's fingers if the field is
    /// being edited.
    pub fn set(&mut self, keyboard: &Keyboard, value: &str) {
        self.value = value.to_string();
        if keyboard.focused == Some(self.id) {
            #[cfg(target_arch = "wasm32")]
            page::set(value);
        }
    }

    /// Draw the field in `rect` and take this frame's typing.
    pub fn show(
        &mut self,
        p: &Painter,
        keyboard: &mut Keyboard,
        rect: Rect,
        kind: FieldKind,
        placeholder: &str,
    ) -> FieldReply {
        let c = &p.theme.color;
        let before = self.value.clone();
        let size = p.size(TextSize::Md);
        let leading = matches!(kind, FieldKind::Search);
        let clear = !self.value.is_empty();
        let text_x = rect.x + if leading { 40.0 } else { 14.0 };
        let text_right = rect.right() - if clear { 40.0 } else { 14.0 };
        let text_box = Rect::new(text_x, rect.y, (text_right - text_x).max(0.0), rect.h);
        let clear_box = Rect::new(rect.right() - 40.0, rect.y, 40.0, rect.h);

        // The tap: the clear button, a tap in the field, or one outside it.
        let mut reply = FieldReply::default();
        if clear && p.tapped(clear_box) {
            self.set(keyboard, "");
        } else if let Some(tap) = p.tap {
            let inside = rect.contains(tap);
            if inside && keyboard.focused != Some(self.id) {
                // On the web the page has already focused its input, from the
                // same tap; natively this is the focus.
                #[cfg(not(target_arch = "wasm32"))]
                {
                    keyboard.focused = Some(self.id);
                }
            } else if !inside && keyboard.focused == Some(self.id) {
                keyboard.dismiss();
            }
        }

        let editing = keyboard.focused == Some(self.id);
        if editing {
            #[cfg(target_arch = "wasm32")]
            {
                self.value = page::value();
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                for _ in 0..keyboard.typed.backspaces {
                    self.value.pop();
                }
                self.value.push_str(&keyboard.typed.text);
                if keyboard.typed.escape {
                    keyboard.focused = None;
                }
            }
        }
        if keyboard.submitted && keyboard.was_focused == Some(self.id) {
            reply.submitted = true;
            #[cfg(not(target_arch = "wasm32"))]
            {
                keyboard.focused = None;
            }
        }
        let editing = keyboard.focused == Some(self.id);
        reply.editing = editing;
        reply.changed = self.value != before;

        #[cfg(target_arch = "wasm32")]
        page::zone(page::Zone {
            id: self.id,
            frac: keyboard.fractions(text_box),
            value: &self.value,
            placeholder,
            kind: kind.code(),
            font: size * keyboard.u,
            color: &crate::css_rgba(c.text_primary),
        });

        // The chrome.
        fill_rounded(rect, 12.0, c.bg_secondary);
        if editing {
            stroke_rounded(rect, 12.0, 1.5, c.accent);
        } else {
            stroke_rounded(rect, 12.0, 1.0, c.border);
        }
        if leading {
            let g = Rect::new(rect.x + 12.0, rect.center().y - 9.0, 18.0, 18.0);
            draw_glyph(Glyph::Search, g, c.text_muted);
        }
        if !self.value.is_empty() {
            let g = Rect::new(clear_box.center().x - 8.0, clear_box.center().y - 8.0, 16.0, 16.0);
            draw_circle(g.center().x, g.center().y, 11.0, c.bg_highlight);
            draw_glyph(Glyph::Close, g, c.text_secondary);
        }

        // The text. On the web, while editing, the page's input draws it.
        let page_draws = cfg!(target_arch = "wasm32") && editing;
        if !page_draws {
            if self.value.is_empty() {
                if !editing {
                    let hint = p.fit(placeholder, size, text_box.w);
                    p.text_in(&hint, text_box, size, c.text_muted, Align::Start);
                }
            } else {
                let w = p.measure(&self.value, size).width;
                // Editing natively, the end of a long value stays in view, as
                // a field's does; at rest it is cut with an ellipsis.
                if editing {
                    let shift = (w - text_box.w + 2.0).max(0.0);
                    let shifted = Rect::new(text_box.x - shift, text_box.y, w + 4.0, text_box.h);
                    with_clip(text_box, || {
                        p.text_in(&self.value, shifted, size, c.text_primary, Align::Start);
                    });
                } else {
                    let shown = p.fit(&self.value, size, text_box.w);
                    p.text_in(&shown, text_box, size, c.text_primary, Align::Start);
                }
            }
            if editing {
                let w = p.measure(&self.value, size).width.min(text_box.w - 2.0);
                // The caret blinks once a second, on the host's clock.
                if p.time().fract() < 0.5 {
                    let x = text_box.x + w + 1.0;
                    let h = size * 1.2;
                    draw_rectangle(x, rect.center().y - h * 0.5, 1.5, h, c.accent);
                }
            }
        }
        reply
    }
}
