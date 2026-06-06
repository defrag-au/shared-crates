//! macroquad-widgets — VM-driven immediate-mode widgets for macroquad
//! buyer-facing surfaces (the txmints mint app). See [`Painter`] for the draw
//! surface and individual widget modules for the VM/action contracts.
//!
//! Pattern (mirrors `egui-widgets`): host projects a VM → widget renders →
//! widget returns actions → host dispatches. No async, no I/O, no backend deps.

pub mod painter;
pub mod theme;

mod button;
mod order_fulfilment;

pub use button::{Button, ButtonVariant};
pub use order_fulfilment::{
    order_fulfilment, FulfilmentAction, FulfilmentResponse, FulfilmentStatus, FulfilmentTx,
    OrderFulfilmentVm, OrderStatus,
};
pub use painter::{draw_rounded_rect, frame_tap, Painter};
