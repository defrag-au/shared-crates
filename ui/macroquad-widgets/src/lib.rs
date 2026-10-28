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
mod callout;
mod choice_stepper;
// Host-owned helpers, not widgets: these hold state across frames, which the
// charter forbids inside a widget precisely so it lives somewhere named.
pub mod assets;
mod detail_sheet;
mod floating_stick;
mod fonts;
mod gesture;
mod mint_checkout;
mod momentum;
mod order_fulfilment;
mod quantity_stepper;
mod radar;
mod route_graph;
mod segmented;
mod slider;
mod squad_picker;
mod wallet_connect;
mod wallet_list;

// The trading views. Gated as a feature rather than a separate crate: they are
// still VM-in/actions-out widgets for a buyer-facing surface, and the renderer
// symmetry is with `egui-widgets`' trading modules. `format`, `market_ink` and
// `marks` are infrastructure (no widget), listed as such in the catalogue.
#[cfg(feature = "trading")]
mod change_chip;
#[cfg(feature = "trading")]
mod feed;
#[cfg(feature = "trading")]
pub mod format;
#[cfg(feature = "trading")]
mod holder_board;
#[cfg(feature = "trading")]
mod holder_map;
#[cfg(feature = "trading")]
mod life_strip;
#[cfg(feature = "trading")]
pub mod market_ink;
#[cfg(feature = "trading")]
mod marks;
#[cfg(feature = "trading")]
mod price_chart;
#[cfg(feature = "trading")]
mod split_bar;
#[cfg(feature = "trading")]
mod stat_tiles;
#[cfg(feature = "trading")]
mod token_header;
#[cfg(feature = "trading")]
mod treemap;

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
pub use callout::{CalloutTone, CalloutVm, callout, tone_ink};
pub use choice_stepper::{ChoiceStepperVm, choice_stepper};
pub use detail_sheet::{
    BOTTOM_SHEET_BELOW, DetailRow, DetailSheetVm, RowTone, SheetCard, SheetForm, SheetResponse,
    SheetState, detail_sheet, info_button, sheet_card, sheet_chrome, sheet_content_width,
    sheet_rect,
};
pub use floating_stick::{
    Cardinal, FloatingStick, Release, StickFrame, StickView, floating_stick,
    response as stick_response,
};
pub use fonts::{FontFiles, Fonts, Slot};
pub use gesture::{Gesture, Gestures, SwipeDir, touch_points};
/// How [`Painter::text_in`] and [`Painter::stack_in`] align in their box.
pub use macroquad_text::Align;
pub use mint_checkout::{
    CheckoutAction, CheckoutResponse, CheckoutState, Eligibility, MintCheckoutVm, mint_checkout,
};
pub use momentum::Momentum;
pub use order_fulfilment::{
    FulfilmentAction, FulfilmentResponse, FulfilmentStatus, FulfilmentTx, OrderFulfilmentVm,
    OrderStatus, order_fulfilment,
};
#[allow(deprecated)]
pub use painter::frame_tap;
pub use painter::{Hit, Line, Painter, draw_rounded_rect};
pub use quantity_stepper::{QuantityStepperVm, StepperAction, StepperResponse, quantity_stepper};
pub use radar::{Blip, RadarVm, behind_sweep, radar, sweep_bearing};
pub use route_graph::{
    LegState, ROUTE_STRIP_H, RouteCard, RouteGraphVm, RouteMeter, RouteNode, route_graph,
    route_graph_height, route_strip,
};
pub use segmented::{pill_rects, segmented, segmented_pills};
pub use slider::{AmountSliderVm, SliderStop, amount_slider};
pub use squad_picker::{
    SquadCandidate, SquadCommit, SquadPickerAction, SquadPickerResponse, SquadPickerVm,
    squad_picker,
};
pub use theme::Theme;
/// The ramp step a VM names. Re-exported because a `CalloutVm` carries one:
/// a host that depends on this crate alone can name its step without also
/// depending on `ui-theme`.
pub use ui_theme::TextSize;
pub use wallet_connect::{
    WalletAction, WalletConnectVm, WalletItem, WalletResponse, WalletState, wallet_connect,
};
pub use wallet_list::{
    WalletListAction, WalletListResponse, WalletListState, WalletListVm, WalletRow, wallet_list,
};

#[cfg(feature = "trading")]
pub use change_chip::{ChangeChipVm, change_chip};
#[cfg(feature = "trading")]
pub use feed::{FEED_ROW_H, FEED_ROW_STEP, FeedRow, FeedVm, feed, feed_row};
#[cfg(feature = "trading")]
pub use holder_board::{
    BoardRow, HolderBoardVm, Holding, HoldingKind, KindInk, holder_board, holding_chip,
    holding_colour, holding_colour_with,
};
#[cfg(feature = "trading")]
pub use holder_map::{
    GROUPS, HolderMapLayout, HolderMapState, HolderMapVm, MapGroup, MapTile, group_of, holder_map,
};
#[cfg(feature = "trading")]
pub use life_strip::{LifeBucket, LifeStripVm, life_strip, price_band};
#[cfg(feature = "trading")]
pub use market_ink::MarketInk;
#[cfg(feature = "trading")]
pub use price_chart::{
    ChartLabels, ChartPoint, PriceChartVm, TradeMark, TradeSide, TradeSize, View, beat_at_x,
    chart_view, price_chart, time_at_x,
};
#[cfg(feature = "trading")]
pub use split_bar::{SplitBarVm, split_bar};
#[cfg(feature = "trading")]
pub use stat_tiles::{StatMark, StatTile, StatTilesVm, stat_tiles};
#[cfg(feature = "trading")]
pub use token_header::{HeaderRects, TokenHeaderVm, TokenLogo, header_rects, token_header};
#[cfg(feature = "trading")]
pub use treemap::{
    DEFAULT_MAX_ASPECT, SEEN_AREA, Tree, TreemapGroup, TreemapState, TreemapTile, TreemapVm, keep,
    treemap,
};
