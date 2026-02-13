//! Shared UI Leptos Components
//!
//! Reusable Leptos components for Cardano applications.
//!
//! ## Available Components
//!
//! - `ImageCard` - Basic image card with optional name overlay
//! - `AssetCard` - Cardano NFT asset card with IIIF URL generation (wraps ImageCard)
//! - `MemoryCard` - Flippable card for memory matching game (wraps AssetCard)
//! - `ConnectionStatus` - WebSocket/realtime connection indicator
//! - `AssetCache` - Non-visual component for preloading NFT images
//!
//! ## Styles
//!
//! Include the component styles once at your app root:
//!
//! ```ignore
//! use ui_components::STYLES;
//!
//! #[component]
//! fn App() -> impl IntoView {
//!     view! {
//!         <style>{STYLES}</style>
//!         // ... rest of app
//!     }
//! }
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use ui_components::{ImageCard, AssetCard, MemoryCard, ConnectionStatus, CardSize};
//!
//! // Basic image card
//! <ImageCard
//!     image_url="https://..."
//!     name="Image Name"
//!     show_name=true
//! />
//!
//! // Cardano NFT with automatic IIIF URL
//! <AssetCard
//!     asset_id="{policy_id}{asset_name_hex}"
//!     name="Pirate #189"
//!     show_name=true
//! />
//!
//! // Memory game card
//! <MemoryCard
//!     asset_id="..."
//!     name="Asset Name"
//!     size=CardSize::Sm
//!     flipped=is_flipped
//!     on_click=move |_| { flip(); }
//! />
//!
//! // Connection status
//! <ConnectionStatus
//!     status=conn_status
//!     on_reconnect=move |_| { reconnect(); }
//! />
//! ```

mod accordion;
mod alert;
mod asset_cache;
mod asset_card;
mod asset_detail_card;
mod asset_grid;
mod asset_picker;
mod badge;
mod button;
mod button_group;
mod card;
mod color_swatch;
mod connection_status;
mod draggable_stack;
mod drop_editor;
mod empty_state;
mod form_group;
mod header;
mod helpers;
pub mod image_cache;
mod image_card;
mod info_grid;
mod loading_overlay;
mod memory_card;
mod modal;
mod modal_context;
mod modal_stack;
mod pagination;
mod player_card;
mod progress_bar;
mod rating;
mod role_dots;
mod select;
mod skeleton;
mod stat_pill;
mod styles;
mod tabs;
mod text_input;
mod textarea;
mod toast;
mod use_draggable;
mod user_avatar;

// Wallet feature - components that depend on wallet-pallas types
#[cfg(feature = "wallet")]
mod policy_folder;
#[cfg(feature = "wallet")]
mod wallet_nft_gallery;

// Asset modal (no wallet feature needed - just needs cardano-assets)
mod asset_modal;

pub use accordion::{Accordion, AccordionItem};
pub use alert::{Alert, AlertVariant};
pub use asset_cache::{AssetCache, PreloadAsset};
pub use asset_card::{generate_iiif_url, AssetCard, IiifSize};
pub use asset_detail_card::AssetDetailCard;
pub use asset_grid::AssetGrid;
pub use asset_picker::{AssetPicker, PickerAsset};
pub use badge::{Badge, BadgeSize, BadgeVariant};
pub use button::{Button, ButtonSize, ButtonVariant};
pub use button_group::ButtonGroup;
pub use card::Card;
pub use color_swatch::{ColorSwatch, SwatchSize};
pub use connection_status::{ConnectionState, ConnectionStatus};
pub use draggable_stack::{DraggableStack, ItemDragState, StackDirection};
pub use drop_editor::DropEditor;
pub use empty_state::EmptyState;
pub use form_group::FormGroup;
pub use header::PageHeader;
pub use helpers::children_fn;
pub use image_card::{parse_card_size, CardSize, ImageCard};
pub use info_grid::{InfoGrid, InfoRow};
pub use loading_overlay::{LoadingOverlay, Spinner, SpinnerSize};
pub use memory_card::MemoryCard;
pub use modal::Modal;
pub use modal_stack::{ModalStack, ModalStackContext};
pub use pagination::{use_adaptive_pagination, use_pagination, Pagination, PaginationState};
pub use player_card::PlayerCard;
pub use progress_bar::ProgressBar;
pub use rating::{Rating, RatingSize};
pub use role_dots::{RoleDot, RoleDots};
pub use select::{Select, SelectOption};
pub use skeleton::{Skeleton, SkeletonVariant};
pub use stat_pill::{StatPill, StatPillColor, StatPillColorInput, StatPillSize};
pub use styles::STYLES;
pub use tabs::{TabDef, TabPanel, TabPanelControlled, Tabs, TabsContext};
pub use text_input::{InputType, TextInput};
pub use textarea::Textarea;
pub use toast::{
    try_use_toasts, use_toasts, Toast, ToastContainer, ToastContext, ToastKind, ToastProvider,
    DEFAULT_TOAST_DURATION_MS,
};
pub use use_draggable::{use_draggable, DragAttrs, DragState, Draggable, Reorder};
pub use user_avatar::{AvatarSize, UserAvatar};

// Wallet feature exports
#[cfg(feature = "wallet")]
pub use policy_folder::PolicyFolder;
#[cfg(feature = "wallet")]
pub use wallet_nft_gallery::WalletNftGallery;

// Asset modal export
pub use asset_modal::AssetModal;
