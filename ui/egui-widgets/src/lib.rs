pub use egui_inbox;

pub mod animated_counter;
pub mod buttons;
pub mod donut_chart;
pub mod flip_counter;
pub mod image_loader;
pub mod listing_grid;
pub mod marquee;
pub mod metric_card;
pub mod progress_bar;
pub mod screenshot;
pub mod seven_segment;
pub mod sparkline;
pub mod swap_modal;
pub mod theme;
pub mod utils;
#[cfg(target_arch = "wasm32")]
pub mod wallet;
#[cfg(target_arch = "wasm32")]
pub mod wallet_button;

pub use animated_counter::AnimatedCounter;
pub use buttons::UiButtonExt;
pub use donut_chart::{
    format_value as format_chart_value, legend_row, DistBand, DistributionChart,
};
pub use flip_counter::FlipCounter;
pub use listing_grid::{ListingCard, ListingGrid, ListingGridConfig};
pub use marquee::{Marquee, MarqueeConfig, MarqueeItem};
pub use metric_card::{MetricCard, Trend};
pub use progress_bar::ProgressBar;
pub use screenshot::ScreenshotButton;
pub use seven_segment::SevenSegmentDisplay;
pub use sparkline::Sparkline;
pub use swap_modal::{
    CultureBuy, SwapModal, SwapModalAction, SwapModalConfig, SwapModalTheme, SwapPreviewData,
    SwapProgress,
};
pub use theme::FontStrategy;
pub use utils::{format_duration, format_number, section_heading, stat_card, truncate_hex};
#[cfg(target_arch = "wasm32")]
pub use wallet_button::{WalletAction, WalletButton, WalletButtonTheme};
