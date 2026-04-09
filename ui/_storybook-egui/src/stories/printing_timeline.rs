//! Printing Timeline story — shows a card's reprint history across sets.

use egui_widgets::printing_timeline::{
    PrintingNode, PrintingTimelineConfig, PrintingTimelineState,
};

use crate::{ACCENT, TEXT_MUTED};

pub struct PrintingTimelineDemo {
    pub state: PrintingTimelineState,
    pub nodes: Vec<PrintingNode>,
    pub show_thumbnails: bool,
    pub node_width: f32,
}

impl Default for PrintingTimelineDemo {
    fn default() -> Self {
        // Lightning Bolt printing history (simplified)
        let nodes = vec![
            PrintingNode {
                set_code: "LEA".into(),
                set_name: "Limited Edition Alpha".into(),
                released_at: "1993-08-05".into(),
                rarity: "common".into(),
                collector_number: "161".into(),
                image_url: None,
                is_original: true,
            },
            PrintingNode {
                set_code: "LEB".into(),
                set_name: "Limited Edition Beta".into(),
                released_at: "1993-10-01".into(),
                rarity: "common".into(),
                collector_number: "163".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "3ED".into(),
                set_name: "Revised Edition".into(),
                released_at: "1994-04-01".into(),
                rarity: "common".into(),
                collector_number: "162".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "4ED".into(),
                set_name: "Fourth Edition".into(),
                released_at: "1995-04-01".into(),
                rarity: "common".into(),
                collector_number: "194".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "M10".into(),
                set_name: "Magic 2010".into(),
                released_at: "2009-07-17".into(),
                rarity: "common".into(),
                collector_number: "146".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "M11".into(),
                set_name: "Magic 2011".into(),
                released_at: "2010-07-16".into(),
                rarity: "common".into(),
                collector_number: "149".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "MM2".into(),
                set_name: "Modern Masters 2015".into(),
                released_at: "2015-05-22".into(),
                rarity: "uncommon".into(),
                collector_number: "123".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "A25".into(),
                set_name: "Masters 25".into(),
                released_at: "2018-03-16".into(),
                rarity: "uncommon".into(),
                collector_number: "141".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "STA".into(),
                set_name: "Strixhaven Mystical Archive".into(),
                released_at: "2021-04-23".into(),
                rarity: "rare".into(),
                collector_number: "42".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "2X2".into(),
                set_name: "Double Masters 2022".into(),
                released_at: "2022-07-08".into(),
                rarity: "uncommon".into(),
                collector_number: "117".into(),
                image_url: None,
                is_original: false,
            },
            PrintingNode {
                set_code: "CMM".into(),
                set_name: "Commander Masters".into(),
                released_at: "2023-08-04".into(),
                rarity: "uncommon".into(),
                collector_number: "206".into(),
                image_url: None,
                is_original: false,
            },
        ];

        Self {
            state: PrintingTimelineState::default(),
            nodes,
            show_thumbnails: false,
            node_width: 100.0,
        }
    }
}

pub fn show(ui: &mut egui::Ui, demo: &mut PrintingTimelineDemo) {
    ui.label(
        egui::RichText::new("Printing Timeline")
            .color(ACCENT)
            .strong()
            .size(16.0),
    );
    ui.label(
        egui::RichText::new("Lightning Bolt — reprint history across 30+ years of MtG")
            .color(TEXT_MUTED)
            .small(),
    );
    ui.add_space(8.0);

    // Controls
    ui.horizontal(|ui| {
        ui.checkbox(&mut demo.show_thumbnails, "Show thumbnails");
        ui.add(egui::Slider::new(&mut demo.node_width, 60.0..=160.0).text("Node width"));
    });
    ui.add_space(8.0);

    let config = PrintingTimelineConfig {
        show_thumbnails: demo.show_thumbnails,
        node_width: demo.node_width,
        ..PrintingTimelineConfig::default()
    };

    let resp = egui_widgets::printing_timeline::show(
        ui,
        &mut demo.state,
        &demo.nodes,
        &config,
    );

    // Show selection info below timeline
    ui.add_space(8.0);
    if let Some(idx) = demo.state.selected {
        let node = &demo.nodes[idx];
        ui.group(|ui| {
            ui.label(egui::RichText::new(format!("{} — {}", node.set_name, node.set_code)).strong());
            ui.label(format!("Released: {}", node.released_at));
            ui.label(format!("Rarity: {}", node.rarity));
            ui.label(format!("Collector #: {}", node.collector_number));
            if node.is_original {
                ui.label(egui::RichText::new("Original printing").color(ACCENT));
            }
        });
    } else {
        ui.label(egui::RichText::new("Click a node to see details").color(TEXT_MUTED));
    }

    // Show rarity evolution narrative
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("Rarity Evolution")
            .color(ACCENT)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "Lightning Bolt was printed as common for its first 15 years, \
             then shifted to uncommon in Masters sets, and appeared as rare \
             in the Mystical Archive — reflecting its growing recognition as \
             one of the most iconic cards in the game."
        )
        .color(TEXT_MUTED)
        .small(),
    );
}
