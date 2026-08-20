//! macroquad-widgets — VM-driven immediate-mode widgets for macroquad
//! buyer-facing surfaces (the txmints mint app). See [`Painter`] for the draw
//! surface and individual widget modules for the VM/action contracts.
//!
//! Pattern (mirrors `egui-widgets`): host projects a VM → widget renders →
//! widget returns actions → host dispatches. No async, no I/O, no backend deps.

pub mod painter;
pub mod theme;

mod button;
mod gesture;
mod mint_checkout;
mod order_fulfilment;
mod quantity_stepper;
mod squad_picker;
mod wallet_connect;
mod wallet_list;

pub use button::{Button, ButtonVariant};
pub use gesture::{Gesture, Gestures, SwipeDir};
pub use mint_checkout::{
    mint_checkout, CheckoutAction, CheckoutResponse, CheckoutState, Eligibility, MintCheckoutVm,
};
pub use order_fulfilment::{
    order_fulfilment, FulfilmentAction, FulfilmentResponse, FulfilmentStatus, FulfilmentTx,
    OrderFulfilmentVm, OrderStatus,
};
#[allow(deprecated)]
pub use painter::frame_tap;
pub use painter::{draw_rounded_rect, Hit, Painter};
pub use quantity_stepper::{quantity_stepper, QuantityStepperVm, StepperAction, StepperResponse};
pub use squad_picker::{
    squad_picker, SquadCandidate, SquadCommit, SquadPickerAction, SquadPickerResponse,
    SquadPickerVm,
};
pub use theme::Theme;
pub use wallet_connect::{
    wallet_connect, WalletAction, WalletConnectVm, WalletItem, WalletResponse, WalletState,
};
pub use wallet_list::{
    wallet_list, WalletListAction, WalletListResponse, WalletListState, WalletListVm, WalletRow,
};
