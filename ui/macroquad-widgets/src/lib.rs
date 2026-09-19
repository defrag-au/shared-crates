//! macroquad-widgets — VM-driven immediate-mode widgets for macroquad
//! buyer-facing surfaces (the txmints mint app). See [`Painter`] for the draw
//! surface and individual widget modules for the VM/action contracts.
//!
//! Pattern (mirrors `egui-widgets`): host projects a VM → widget renders →
//! widget returns actions → host dispatches. No async, no I/O, no backend deps.

pub mod painter;
pub mod theme;

#[cfg(feature = "chain")]
mod block_pulse;
#[cfg(feature = "chain")]
mod block_train;
mod button;
// Host-owned helpers, not widgets: these hold state across frames, which the
// charter forbids inside a widget precisely so it lives somewhere named.
pub mod assets;
mod fonts;
mod gesture;
mod mint_checkout;
mod order_fulfilment;
mod quantity_stepper;
mod slider;
mod squad_picker;
mod wallet_connect;
mod wallet_list;

pub use assets::{Asset, Loader, Progress, draw_loading};
#[cfg(feature = "chain")]
pub use block_pulse::{
    BlockPulseVm, OFFLINE_AFTER_SECS, PulseDetail, PulseState, PulseTicker, block_pulse, draw_mark,
    format_duration, format_number, fullness_percent,
};
#[cfg(feature = "chain")]
pub use block_train::{
    BlockTrainVm, DEFAULT_SETTLED_DEPTH, RiderDepth, RiderState, TrainResponse, TrainRider,
    TrainState, block_train, rider_depth, riders,
};
pub use button::{Button, ButtonVariant};
pub use fonts::{FontFiles, Fonts, Slot};
pub use gesture::{Gesture, Gestures, SwipeDir};
pub use mint_checkout::{
    CheckoutAction, CheckoutResponse, CheckoutState, Eligibility, MintCheckoutVm, mint_checkout,
};
pub use order_fulfilment::{
    FulfilmentAction, FulfilmentResponse, FulfilmentStatus, FulfilmentTx, OrderFulfilmentVm,
    OrderStatus, order_fulfilment,
};
#[allow(deprecated)]
pub use painter::frame_tap;
pub use painter::{Hit, Painter, draw_rounded_rect};
pub use quantity_stepper::{QuantityStepperVm, StepperAction, StepperResponse, quantity_stepper};
pub use slider::{AmountSliderVm, SliderStop, amount_slider};
pub use squad_picker::{
    SquadCandidate, SquadCommit, SquadPickerAction, SquadPickerResponse, SquadPickerVm,
    squad_picker,
};
pub use theme::Theme;
pub use wallet_connect::{
    WalletAction, WalletConnectVm, WalletItem, WalletResponse, WalletState, wallet_connect,
};
pub use wallet_list::{
    WalletListAction, WalletListResponse, WalletListState, WalletListVm, WalletRow, wallet_list,
};
