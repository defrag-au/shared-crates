#[cfg(target_arch = "wasm32")]
mod stories;

#[cfg(target_arch = "wasm32")]
pub use app::*;

#[cfg(target_arch = "wasm32")]
mod app {
    use eframe::wasm_bindgen::JsCast as _;
    use wasm_bindgen::prelude::*;

    use super::stories;

    // ========================================================================
    // Story Registry
    // ========================================================================

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Story {
        Distribution,
        Marquee,
        Buttons,
        ProgressBar,
        Sparkline,
        MetricCard,
        SevenSegment,
        FlipCounter,
        AsyncData,
        MeshPlayground,
        PerspectiveText,
        TcgCard,
        AssetCard,
        RadarChart,
        RangeBar,
        PipRow,
        CardBrowser,
        IconGallery,
        WalletButton,
        SwapModal,
    }

    impl Story {
        fn all() -> &'static [Self] {
            &[
                Self::Distribution,
                Self::Marquee,
                Self::Buttons,
                Self::ProgressBar,
                Self::Sparkline,
                Self::MetricCard,
                Self::SevenSegment,
                Self::FlipCounter,
                Self::AsyncData,
                Self::MeshPlayground,
                Self::PerspectiveText,
                Self::TcgCard,
                Self::AssetCard,
                Self::RadarChart,
                Self::RangeBar,
                Self::PipRow,
                Self::CardBrowser,
                Self::IconGallery,
                Self::WalletButton,
                Self::SwapModal,
            ]
        }

        fn label(&self) -> &'static str {
            match self {
                Self::Distribution => "Distribution",
                Self::Marquee => "Marquee",
                Self::Buttons => "Buttons",
                Self::ProgressBar => "Progress Bar",
                Self::Sparkline => "Sparkline",
                Self::MetricCard => "Metric Card",
                Self::SevenSegment => "Seven Segment",
                Self::FlipCounter => "Flip Counter",
                Self::AsyncData => "Async Data",
                Self::MeshPlayground => "Mesh Playground",
                Self::PerspectiveText => "Perspective Text",
                Self::TcgCard => "TCG Card",
                Self::AssetCard => "Asset Card",
                Self::RadarChart => "Radar Chart",
                Self::RangeBar => "Range Bar",
                Self::PipRow => "Pip Row",
                Self::CardBrowser => "Card Browser",
                Self::IconGallery => "Icon Gallery",
                Self::WalletButton => "Wallet Button",
                Self::SwapModal => "Swap Modal",
            }
        }

        fn category(&self) -> &'static str {
            match self {
                Self::Distribution | Self::Marquee | Self::Buttons => "Primitives",
                Self::ProgressBar
                | Self::Sparkline
                | Self::MetricCard
                | Self::SevenSegment
                | Self::FlipCounter
                | Self::AsyncData
                | Self::MeshPlayground
                | Self::PerspectiveText
                | Self::TcgCard
                | Self::AssetCard
                | Self::RadarChart
                | Self::RangeBar
                | Self::PipRow
                | Self::CardBrowser
                | Self::IconGallery => "Data Visualization",
                Self::WalletButton => "Wallet",
                Self::SwapModal => "Swap",
            }
        }

        fn description(&self) -> &'static str {
            match self {
                Self::Distribution => "Concentric orbital rings supply distribution chart",
                Self::Marquee => "Scrolling ticker with delta-time animation and static centering",
                Self::Buttons => "UiButtonExt trait \u{2014} pointer cursor on hover for buttons",
                Self::ProgressBar => "Determinate and countdown progress bars with custom colors",
                Self::Sparkline => {
                    "Inline line chart with fill gradient, mean line, and hover inspection"
                }
                Self::MetricCard => {
                    "Dashboard stat card with trend indicators and embedded sparklines"
                }
                Self::SevenSegment => "Retro LED-style 7-segment display with animated counter",
                Self::FlipCounter => "Split-flap airport board style counter with flip animations",
                Self::AsyncData => "egui_inbox driving widgets from simulated API polling",
                Self::MeshPlayground => {
                    "Raw Mesh API: quads, gradients, trapezoids, rotation, strips"
                }
                Self::PerspectiveText => {
                    "Galley mesh vertex transforms: scale, wave, perspective flip"
                }
                Self::TcgCard => {
                    "Trading card rendering with perspective tilt, holographic effects, and card flip"
                }
                Self::AssetCard => {
                    "Asset card widget: square, hex, rounded square — with holographic foil, stats, and 3D tilt"
                }
                Self::RadarChart => {
                    "Spider/radar chart for N-dimensional normalized data with bezier curves"
                }
                Self::RangeBar => {
                    "Horizontal range bar with labeled tick marks, gradient fill, and auto-staggered labels"
                }
                Self::PipRow => {
                    "Label + horizontal pip bar for distributions, market depth, and ranked data"
                }
                Self::CardBrowser => {
                    "Master-detail card grid with selection, detail panel, and caller-driven rendering"
                }
                Self::IconGallery => {
                    "Phosphor icon font gallery with size/color controls and contextual examples"
                }
                Self::WalletButton => "CIP-30 wallet connection button with state management",
                Self::SwapModal => "DEX swap modal with preview, culture buys, and progress states",
            }
        }
    }

    // ========================================================================
    // Theme
    // ========================================================================

    const BG_SIDEBAR: egui::Color32 = egui::Color32::from_rgb(20, 20, 40);
    pub const BG_MAIN: egui::Color32 = egui::Color32::from_rgb(26, 26, 46);
    pub const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(100, 100, 130);
    const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(220, 220, 235);
    pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(68, 255, 68);
    const BG_SELECTED: egui::Color32 = egui::Color32::from_rgb(40, 40, 60);

    fn configure_style(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = BG_MAIN;
        style.visuals.window_fill = BG_MAIN;
        style.visuals.override_text_color = Some(TEXT_PRIMARY);
        ctx.set_style(style);
    }

    // ========================================================================
    // App
    // ========================================================================

    struct StorybookApp {
        current_story: Story,
        // Per-story state
        distribution_chart: egui_widgets::DistributionChart,
        marquee: egui_widgets::Marquee,
        marquee_messages: Vec<egui_widgets::MarqueeItem>,
        progress_bar_state: stories::progress_bar::ProgressBarState,
        sparkline_state: stories::sparkline::SparklineState,
        seven_segment_state: stories::seven_segment::SevenSegmentState,
        flip_counter_state: stories::flip_counter::FlipCounterState,
        async_data_state: stories::async_data::AsyncDataState,
        mesh_playground_state: stories::mesh_playground::MeshPlaygroundState,
        perspective_text_state: stories::perspective_text::PerspectiveTextState,
        tcg_card_state: stories::tcg_card::TcgCardState,
        asset_card_state: stories::asset_card::AssetCardState,
        radar_chart_state: stories::radar_chart::RadarChartState,
        range_bar_state: stories::range_bar::RangeBarState,
        pip_row_state: stories::pip_row::PipRowState,
        card_browser_state: stories::card_browser::CardBrowserStoryState,
        icon_gallery_state: stories::icon_gallery::IconGalleryState,
        wallet_btn: egui_widgets::WalletButton,
        wallet_connector: egui_widgets::wallet::WalletConnector,
        swap_modal: egui_widgets::SwapModal,
        swap_progress: egui_widgets::SwapProgress,
    }

    impl StorybookApp {
        fn new(cc: &eframe::CreationContext<'_>) -> Self {
            configure_style(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.add_image_loader(std::sync::Arc::new(
                egui_widgets::image_loader::browser::BrowserImageLoader::default(),
            ));
            egui_widgets::install_phosphor_font(&cc.egui_ctx);

            Self {
                current_story: Story::Distribution,
                distribution_chart: egui_widgets::DistributionChart::new(),
                marquee: egui_widgets::Marquee::default(),
                marquee_messages: vec![egui_widgets::MarqueeItem {
                    text: "Welcome to the egui Widgets Storybook".into(),
                    color: ACCENT,
                }],
                progress_bar_state: stories::progress_bar::ProgressBarState::default(),
                sparkline_state: stories::sparkline::SparklineState::default(),
                seven_segment_state: stories::seven_segment::SevenSegmentState::default(),
                flip_counter_state: stories::flip_counter::FlipCounterState::default(),
                async_data_state: stories::async_data::AsyncDataState::default(),
                mesh_playground_state: stories::mesh_playground::MeshPlaygroundState::default(),
                perspective_text_state: stories::perspective_text::PerspectiveTextState::default(),
                tcg_card_state: stories::tcg_card::TcgCardState::default(),
                asset_card_state: stories::asset_card::AssetCardState::default(),
                radar_chart_state: stories::radar_chart::RadarChartState::default(),
                range_bar_state: stories::range_bar::RangeBarState::default(),
                pip_row_state: stories::pip_row::PipRowState::default(),
                card_browser_state: stories::card_browser::CardBrowserStoryState::default(),
                icon_gallery_state: stories::icon_gallery::IconGalleryState::default(),
                wallet_btn: egui_widgets::WalletButton::new(),
                wallet_connector: egui_widgets::wallet::WalletConnector::new(),
                swap_modal: egui_widgets::SwapModal::new(egui_widgets::SwapModalConfig {
                    token_name: "TestToken".into(),
                    token_ticker: Some("TST".into()),
                    culture_buys: vec![
                        egui_widgets::CultureBuy {
                            ada_amount: 51,
                            label: "Area 51".into(),
                        },
                        egui_widgets::CultureBuy {
                            ada_amount: 69,
                            label: "Nice".into(),
                        },
                        egui_widgets::CultureBuy {
                            ada_amount: 420,
                            label: "Blaze".into(),
                        },
                    ],
                    theme: egui_widgets::SwapModalTheme::default(),
                }),
                swap_progress: egui_widgets::SwapProgress::Idle,
            }
        }

        fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
            let mut current_category = "";
            for story in Story::all() {
                if story.category() != current_category {
                    current_category = story.category();
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(current_category)
                            .color(TEXT_MUTED)
                            .small()
                            .strong(),
                    );
                }
                let is_selected = self.current_story == *story;
                let text = if is_selected {
                    egui::RichText::new(story.label()).color(ACCENT).strong()
                } else {
                    egui::RichText::new(story.label()).color(TEXT_PRIMARY)
                };
                let fill = if is_selected {
                    BG_SELECTED
                } else {
                    egui::Color32::TRANSPARENT
                };
                if ui
                    .add(
                        egui::Button::new(text)
                            .fill(fill)
                            .frame(false)
                            .min_size(egui::vec2(ui.available_width(), 24.0)),
                    )
                    .clicked()
                {
                    self.current_story = *story;
                }
            }
        }
    }

    impl eframe::App for StorybookApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::SidePanel::left("stories")
                .default_width(180.0)
                .resizable(false)
                .frame(egui::Frame::side_top_panel(&ctx.style()).fill(BG_SIDEBAR))
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.heading(egui::RichText::new("egui Widgets").color(ACCENT));
                    ui.separator();
                    self.draw_sidebar(ui);
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(&ctx.style()).fill(BG_MAIN))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading(self.current_story.label());
                        ui.label(
                            egui::RichText::new(self.current_story.description()).color(TEXT_MUTED),
                        );
                        ui.separator();
                        ui.add_space(8.0);

                        match self.current_story {
                            Story::Distribution => {
                                stories::distribution::show(ui, &mut self.distribution_chart)
                            }
                            Story::Marquee => stories::marquee::show(
                                ui,
                                &mut self.marquee,
                                &mut self.marquee_messages,
                            ),
                            Story::Buttons => stories::buttons::show(ui),
                            Story::ProgressBar => {
                                stories::progress_bar::show(ui, &mut self.progress_bar_state)
                            }
                            Story::Sparkline => {
                                stories::sparkline::show(ui, &mut self.sparkline_state)
                            }
                            Story::MetricCard => stories::metric_card::show(ui),
                            Story::SevenSegment => {
                                stories::seven_segment::show(ui, &mut self.seven_segment_state)
                            }
                            Story::FlipCounter => {
                                stories::flip_counter::show(ui, &mut self.flip_counter_state)
                            }
                            Story::AsyncData => {
                                stories::async_data::show(ui, &mut self.async_data_state)
                            }
                            Story::MeshPlayground => {
                                stories::mesh_playground::show(ui, &mut self.mesh_playground_state)
                            }
                            Story::PerspectiveText => stories::perspective_text::show(
                                ui,
                                &mut self.perspective_text_state,
                            ),
                            Story::TcgCard => stories::tcg_card::show(ui, &mut self.tcg_card_state),
                            Story::AssetCard => {
                                stories::asset_card::show(ui, &mut self.asset_card_state)
                            }
                            Story::RadarChart => {
                                stories::radar_chart::show(ui, &mut self.radar_chart_state)
                            }
                            Story::RangeBar => {
                                stories::range_bar::show(ui, &mut self.range_bar_state)
                            }
                            Story::PipRow => stories::pip_row::show(ui, &mut self.pip_row_state),
                            Story::CardBrowser => {
                                stories::card_browser::show(ui, &mut self.card_browser_state)
                            }
                            Story::IconGallery => {
                                stories::icon_gallery::show(ui, &mut self.icon_gallery_state)
                            }
                            Story::WalletButton => stories::wallet::show(
                                ui,
                                &mut self.wallet_btn,
                                &mut self.wallet_connector,
                            ),
                            Story::SwapModal => stories::swap::show(
                                ctx,
                                ui,
                                &mut self.swap_modal,
                                &mut self.swap_progress,
                            ),
                        }
                    });
                });
        }
    }

    // ========================================================================
    // Entry Point
    // ========================================================================

    #[wasm_bindgen(start)]
    pub fn main() {
        console_error_panic_hook::set_once();
        eframe::WebLogger::init(log::LevelFilter::Debug).ok();

        let web_options = eframe::WebOptions::default();

        wasm_bindgen_futures::spawn_local(async {
            let document = web_sys::window()
                .expect("no window")
                .document()
                .expect("no document");
            let canvas = document
                .get_element_by_id("egui_canvas")
                .expect("no egui_canvas element")
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .expect("not a canvas");

            eframe::WebRunner::new()
                .start(
                    canvas,
                    web_options,
                    Box::new(|cc| Ok(Box::new(StorybookApp::new(cc)))),
                )
                .await
                .expect("failed to start eframe");
        });
    }
}
