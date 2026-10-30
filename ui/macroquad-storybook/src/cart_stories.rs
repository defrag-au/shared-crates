//! The gamekit's `Cart`, in every stage a purchase passes through, at a desk's
//! width and a phone's.
//!
//! The rows are Perps for sale with what each would add to a staker's rate:
//! the cart's first host, the staking site. The cart itself knows nothing of
//! Perps or staking; these stories only feed it words.
//!
//! - fixed stages, opened and settled, for review captures;
//! - `live`: the button opens it; Go runs a fake purchase (building,
//!   approving, done); `A` adds a row, `F` makes the next purchase fail.

use macroquad::prelude::*;
use macroquad_gamekit::cart::{
    Cart, CartAction, CartRow, CartStage, CartTone, CartTotal, CartVm, cart_button,
};
use macroquad_widgets::{Gesture, Painter, ScrollInput, TextSize};

/// A phone's width, where the cart is a bottom sheet.
const PHONE_W: f32 = 390.0;

/// `n` with thousands separated by commas.
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Which cart a story shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CartView {
    Editing,
    /// More rows than the sheet holds: the list scrolls.
    Many,
    Empty,
    /// The action waits on something the host names.
    Blocked,
    Working,
    Failed,
    Done,
    /// Driven by hand.
    Live,
}

/// Perps for sale, as the staking site would word them: name, gain, price.
const FOR_SALE: [(&str, &str, u32); 14] = [
    ("Perp #5838", "+22 $PERP a day · ×2.2", 30),
    ("Perp #3292", "+22 $PERP a day · ×2.2", 35),
    ("Perp #1083", "+20 $PERP a day · ×2.0", 35),
    ("Perp #5283", "+24.2 $PERP a day · ×2.42", 60),
    ("Perp #4244", "+22 $PERP a day · ×2.2", 60),
    ("Perp #0608", "+10 $PERP a day · ×1.0", 30),
    ("Perp #2810", "+15 $PERP a day · ×1.5", 48),
    ("Perp #4410", "+11 $PERP a day · ×1.1", 40),
    ("Perp #0911", "+20 $PERP a day · ×2.0", 75),
    ("Perp #3360", "+35 $PERP a day · ×3.5", 140),
    ("Perp #1744", "+11 $PERP a day · ×1.1", 42),
    ("Perp #5120", "+10 $PERP a day · ×1.0", 31),
    ("Perp #0077", "+150 $PERP a day · ×15.0", 2200),
    ("Perp #2222", "+12.1 $PERP a day · ×1.21", 45),
];

pub struct CartStory {
    view: CartView,
    phone: bool,
    icon: Option<Texture2D>,
    cart: Cart,
    /// Indexes into [`FOR_SALE`].
    rows: Vec<usize>,
    stage: CartStage,
    /// The live story's fake purchase: seconds into it, and whether it fails.
    running: Option<f32>,
    fail_next: bool,
    last: String,
}

impl CartStory {
    pub fn new(view: CartView, phone: bool, icon: Option<Texture2D>) -> Self {
        let rows = match view {
            CartView::Empty => Vec::new(),
            CartView::Many => (0..FOR_SALE.len()).collect(),
            _ => vec![0, 3, 4],
        };
        let stage = match view {
            CartView::Working => CartStage::Working {
                step: "Approve the purchase in Eternl".into(),
            },
            CartView::Failed => CartStage::Failed {
                message: "Perp #5283 sold before the purchase went through.".into(),
            },
            CartView::Done => CartStage::Done {
                message: "Bought 3 Perps. They earn from the next block.".into(),
            },
            _ => CartStage::Editing,
        };
        let cart = if view == CartView::Live {
            Cart::new()
        } else {
            Cart::new().opened()
        };
        Self {
            view,
            phone,
            icon,
            cart,
            rows,
            stage,
            running: None,
            fail_next: false,
            last: String::new(),
        }
    }

    fn vm(&self) -> CartVm {
        let rows: Vec<CartRow> = self
            .rows
            .iter()
            .map(|&i| {
                let (title, detail, ada) = FOR_SALE[i];
                CartRow {
                    id: title.to_string(),
                    title: title.to_string(),
                    detail: detail.to_string(),
                    // "ADA", not "₳": the storybook's faces have no ₳.
                    figure: format!("{ada} ADA"),
                    // Every other row has its picture; the rest are still
                    // loading.
                    image: if i % 2 == 0 { self.icon.clone() } else { None },
                    detail_tone: CartTone::Good,
                }
            })
            .collect();
        let total: u32 = self.rows.iter().map(|&i| FOR_SALE[i].2).sum();
        let n = rows.len();
        let totals = if n == 0 {
            Vec::new()
        } else {
            vec![
                CartTotal {
                    label: "Adds to your rate".into(),
                    value: format!("+{} $PERP a day", n * 22),
                    tone: CartTone::Good,
                    strong: false,
                },
                CartTotal {
                    label: "Your rate".into(),
                    value: format!("3,658 → {} a day", grouped(3658 + n * 22)),
                    tone: CartTone::Normal,
                    strong: false,
                },
                CartTotal {
                    label: "Total".into(),
                    value: format!("{} ADA", grouped(total as usize)),
                    tone: CartTone::Accent,
                    strong: true,
                },
            ]
        };
        CartVm {
            title: "Cart".into(),
            rows,
            totals,
            stage: self.stage.clone(),
            action: match (&self.stage, n) {
                (CartStage::Failed { .. }, _) => "Try again".into(),
                (_, 1) => "Buy 1 Perp".into(),
                (_, n) => format!("Buy {n} Perps"),
            },
            blocker: (self.view == CartView::Blocked)
                .then(|| "Connect a wallet to buy.".to_string()),
            empty: "Nothing in the cart yet. Tap a Perp for sale, then Add to cart.".into(),
        }
    }

    pub fn draw(&mut self, p: &Painter, area: Rect, g: &Gesture, dt: f32) {
        let stage = if self.phone {
            Rect::new(area.x, area.y, PHONE_W.min(area.w), area.h.min(760.0))
        } else {
            area
        };
        let t = p.theme.color;
        draw_rectangle(stage.x, stage.y, stage.w, stage.h, t.bg_primary);
        p.text(
            "STAKING · BOOST",
            stage.x + 16.0,
            stage.y + 32.0,
            p.size(TextSize::Sm),
            t.text_muted,
        );
        if self.view == CartView::Live {
            self.play(dt);
        }
        let vm = self.vm();
        let total = vm
            .totals
            .last()
            .map_or(String::new(), |line| line.value.clone());
        let w = if self.phone { 150.0 } else { 190.0 };
        let button = Rect::new(stage.right() - w - 16.0, stage.y + 14.0, w, 34.0);
        let behind = p.untapped();
        let button_p = if self.cart.visible() { &behind } else { p };
        if cart_button(button_p, button, vm.rows.len(), &total) {
            self.cart.open();
            self.last = "button: open".into();
        }
        let input = ScrollInput::read(g.drag, Vec2::from(mouse_position()));
        if let Some(action) = self.cart.show(p, stage, &vm, &input, dt) {
            self.last = format!("action: {action:?}");
            self.act(action);
        }
        if self.view == CartView::Live && is_key_pressed(KeyCode::A) {
            let next = (0..FOR_SALE.len()).find(|i| !self.rows.contains(i));
            if let Some(next) = next {
                self.rows.push(next);
            }
        }
        if self.view == CartView::Live && is_key_pressed(KeyCode::F) {
            self.fail_next = true;
            self.last = "the next purchase fails".into();
        }
        if !self.last.is_empty() {
            p.mono(
                &self.last,
                stage.x,
                stage.bottom() + 16.0,
                p.size(TextSize::Md),
                t.accent,
            );
        }
    }

    fn act(&mut self, action: CartAction) {
        match action {
            CartAction::Remove(id) => self.rows.retain(|&i| FOR_SALE[i].0 != id),
            CartAction::Close => self.cart.close(),
            CartAction::Go if self.view == CartView::Live => {
                self.running = Some(0.0);
                self.stage = CartStage::Working {
                    step: "Building the transaction".into(),
                };
            }
            CartAction::Go => {}
        }
    }

    /// The fake purchase: build, approve, then done or failed.
    fn play(&mut self, dt: f32) {
        let Some(t) = self.running.as_mut() else {
            return;
        };
        *t += dt;
        let t = *t;
        self.stage = if t < 1.2 {
            CartStage::Working {
                step: "Building the transaction".into(),
            }
        } else if t < 3.0 {
            CartStage::Working {
                step: "Approve the purchase in Eternl".into(),
            }
        } else if t < 4.0 {
            CartStage::Working {
                step: "Waiting for the chain".into(),
            }
        } else {
            self.running = None;
            if std::mem::take(&mut self.fail_next) {
                CartStage::Failed {
                    message: "Perp #5283 sold before the purchase went through.".into(),
                }
            } else {
                let n = self.rows.len();
                self.rows.clear();
                CartStage::Done {
                    message: format!("Bought {n} Perps. They earn from the next block."),
                }
            }
        };
    }
}
