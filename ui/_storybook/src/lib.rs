//! Shared UI Storybook
//!
//! Development-only showcase for shared-ui components using Leptos.

pub mod api;
mod stories;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// ============================================================================
// Story Enum
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Story {
    Welcome,
    // Components - Layout
    AccordionComponent,
    CardComponent,
    ModalComponent,
    ModalStackComponent,
    TabsComponent,
    HeaderComponent,
    // Components - Feedback
    LoadingOverlayComponent,
    SkeletonComponent,
    AlertComponent,
    // Components - User
    UserAvatarComponent,
    RoleDotsComponent,
    PlayerCardComponent,
    // Components - Data Display
    ImageCardComponent,
    AssetCardComponent,
    AssetGridComponent,
    AssetPickerComponent,
    AssetDetailCardComponent,
    AssetCacheComponent,
    ConnectionStatusComponent,
    MemoryCardComponent,
    StatPillComponent,
    BadgeComponent,
    EmptyStateComponent,
    ProgressBarComponent,
    InfoGridComponent,
    ColorSwatchComponent,
    RatingComponent,
    // Components - Forms
    ButtonComponent,
    ButtonGroupComponent,
    SelectComponent,
    TextInputComponent,
    TextareaComponent,
    FormGroupComponent,
    // Components - Editors
    DropEditorComponent,
    // Hooks
    UseDraggableHook,
    // Wallet Core
    WalletProviders,
    ConnectionStates,
    WalletDetection,
    WalletConnection,
    WalletBalance,
    WalletNfts,
    WalletLeptos,
    // UI Flow
    FlowOverview,
    FlowState,
    FlowOperations,
    // UI Loader
    LoadingStates,
    LoaderConfig,
    // UI Toast
    ToastTypes,
    ToastUsage,
}

impl Story {
    fn all() -> &'static [Story] {
        &[
            Story::Welcome,
            // Layout
            Story::AccordionComponent,
            Story::CardComponent,
            Story::ModalComponent,
            Story::ModalStackComponent,
            Story::TabsComponent,
            Story::HeaderComponent,
            // Feedback
            Story::LoadingOverlayComponent,
            Story::SkeletonComponent,
            Story::AlertComponent,
            // User
            Story::UserAvatarComponent,
            Story::RoleDotsComponent,
            Story::PlayerCardComponent,
            // Data Display
            Story::ImageCardComponent,
            Story::AssetCardComponent,
            Story::AssetGridComponent,
            Story::AssetPickerComponent,
            Story::AssetDetailCardComponent,
            Story::AssetCacheComponent,
            Story::ConnectionStatusComponent,
            Story::MemoryCardComponent,
            Story::StatPillComponent,
            Story::BadgeComponent,
            Story::EmptyStateComponent,
            Story::ProgressBarComponent,
            Story::InfoGridComponent,
            Story::ColorSwatchComponent,
            Story::RatingComponent,
            // Forms
            Story::ButtonComponent,
            Story::ButtonGroupComponent,
            Story::SelectComponent,
            Story::TextInputComponent,
            Story::TextareaComponent,
            Story::FormGroupComponent,
            // Editors
            Story::DropEditorComponent,
            // Hooks
            Story::UseDraggableHook,
            // Wallet
            Story::WalletProviders,
            Story::ConnectionStates,
            Story::WalletDetection,
            Story::WalletConnection,
            Story::WalletBalance,
            Story::WalletNfts,
            Story::WalletLeptos,
            Story::FlowOverview,
            Story::FlowState,
            Story::FlowOperations,
            Story::LoadingStates,
            Story::LoaderConfig,
            Story::ToastTypes,
            Story::ToastUsage,
        ]
    }

    fn label(&self) -> &'static str {
        match self {
            Story::Welcome => "Welcome",
            // Layout
            Story::AccordionComponent => "Accordion",
            Story::CardComponent => "Card",
            Story::ModalComponent => "Modal",
            Story::ModalStackComponent => "Modal Stack",
            Story::TabsComponent => "Tabs",
            Story::HeaderComponent => "Page Header",
            // Feedback
            Story::LoadingOverlayComponent => "Loading Overlay",
            Story::SkeletonComponent => "Skeleton",
            Story::AlertComponent => "Alert",
            // User
            Story::UserAvatarComponent => "User Avatar",
            Story::RoleDotsComponent => "Role Dots",
            Story::PlayerCardComponent => "Player Card",
            // Data Display
            Story::ImageCardComponent => "Image Card",
            Story::AssetCardComponent => "Asset Card",
            Story::AssetGridComponent => "Asset Grid",
            Story::AssetPickerComponent => "Asset Picker",
            Story::AssetDetailCardComponent => "Asset Detail Card",
            Story::AssetCacheComponent => "Asset Cache",
            Story::ConnectionStatusComponent => "Connection Status",
            Story::MemoryCardComponent => "Memory Card",
            Story::StatPillComponent => "Stat Pill",
            Story::BadgeComponent => "Badge",
            Story::EmptyStateComponent => "Empty State",
            Story::ProgressBarComponent => "Progress Bar",
            Story::InfoGridComponent => "Info Grid",
            Story::ColorSwatchComponent => "Color Swatch",
            Story::RatingComponent => "Rating",
            // Forms
            Story::ButtonComponent => "Button",
            Story::ButtonGroupComponent => "Button Group",
            Story::SelectComponent => "Select",
            Story::TextInputComponent => "Text Input",
            Story::TextareaComponent => "Textarea",
            Story::FormGroupComponent => "Form Group",
            // Editors
            Story::DropEditorComponent => "Drop Editor",
            // Hooks
            Story::UseDraggableHook => "use_draggable",
            // Wallet
            Story::WalletProviders => "Wallet Providers",
            Story::ConnectionStates => "Connection States",
            Story::WalletDetection => "Live Detection",
            Story::WalletConnection => "Connection Flow",
            Story::WalletBalance => "Balance & Assets",
            Story::WalletNfts => "NFT Gallery",
            Story::WalletLeptos => "Leptos Context",
            Story::FlowOverview => "Overview",
            Story::FlowState => "FlowState Trait",
            Story::FlowOperations => "Operations",
            Story::LoadingStates => "Loading States",
            Story::LoaderConfig => "Loader Config",
            Story::ToastTypes => "Toast Types",
            Story::ToastUsage => "Toast Usage",
        }
    }

    fn category(&self) -> &'static str {
        match self {
            Story::Welcome => "Getting Started",
            // Layout components
            Story::AccordionComponent
            | Story::CardComponent
            | Story::ModalComponent
            | Story::ModalStackComponent
            | Story::TabsComponent
            | Story::HeaderComponent => "Layout",
            // Feedback components
            Story::LoadingOverlayComponent | Story::SkeletonComponent | Story::AlertComponent => {
                "Feedback"
            }
            // User components
            Story::UserAvatarComponent | Story::RoleDotsComponent | Story::PlayerCardComponent => {
                "User"
            }
            // Data display components
            Story::ImageCardComponent
            | Story::AssetCardComponent
            | Story::AssetGridComponent
            | Story::AssetPickerComponent
            | Story::AssetDetailCardComponent
            | Story::AssetCacheComponent
            | Story::ConnectionStatusComponent
            | Story::MemoryCardComponent
            | Story::StatPillComponent
            | Story::BadgeComponent
            | Story::EmptyStateComponent
            | Story::ProgressBarComponent
            | Story::InfoGridComponent
            | Story::ColorSwatchComponent
            | Story::RatingComponent => "Data Display",
            // Form components
            Story::ButtonComponent
            | Story::ButtonGroupComponent
            | Story::SelectComponent
            | Story::TextInputComponent
            | Story::TextareaComponent
            | Story::FormGroupComponent => "Forms",
            // Editor components
            Story::DropEditorComponent => "Editors",
            // Hooks
            Story::UseDraggableHook => "Hooks",
            Story::WalletProviders
            | Story::ConnectionStates
            | Story::WalletDetection
            | Story::WalletConnection
            | Story::WalletBalance
            | Story::WalletNfts
            | Story::WalletLeptos => "Wallet Core",
            Story::FlowOverview | Story::FlowState | Story::FlowOperations => "UI Flow",
            Story::LoadingStates | Story::LoaderConfig => "UI Loader",
            Story::ToastTypes | Story::ToastUsage => "UI Toast",
        }
    }
}

// ============================================================================
// Main Entry
// ============================================================================

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    // Initialize tracing to browser console
    tracing_wasm::set_as_global_default();

    // Mount to #app element (not body) to work with the index.html structure
    let app_element = document()
        .get_element_by_id("app")
        .expect("should find #app element")
        .dyn_into::<web_sys::HtmlElement>()
        .expect("#app should be an HtmlElement");
    mount_to(app_element, App).forget();
}

#[component]
fn App() -> impl IntoView {
    let (current_story, set_current_story) = signal(Story::Welcome);

    view! {
        // Include ui-components styles
        <style>{ui_components::STYLES}</style>
        <div class="storybook">
            <Sidebar current_story=current_story set_current_story=set_current_story />
            <main class="storybook-main">
                <StoryContent story=current_story />
            </main>
        </div>
    }
}

#[component]
fn Sidebar(
    current_story: ReadSignal<Story>,
    set_current_story: WriteSignal<Story>,
) -> impl IntoView {
    use leptos::prelude::CollectView;
    // Group stories by category
    let categories: Vec<(&'static str, Vec<Story>)> = {
        let mut cats: Vec<(&str, Vec<Story>)> = Vec::new();
        for story in Story::all() {
            let cat = story.category();
            if let Some((_, stories)) = cats.iter_mut().find(|(c, _)| *c == cat) {
                stories.push(*story);
            } else {
                cats.push((cat, vec![*story]));
            }
        }
        cats
    };

    view! {
        <aside class="storybook-sidebar">
            <h1>"Shared UI"</h1>
            {categories.into_iter().map(|(category, stories)| {
                view! {
                    <div class="nav-section">
                        <h2>{category}</h2>
                        <ul>
                            {stories.into_iter().map(|story| {
                                let is_active = move || current_story.get() == story;
                                view! {
                                    <li>
                                        <a
                                            href="#"
                                            class:active=is_active
                                            on:click=move |ev| {
                                                ev.prevent_default();
                                                tracing::info!("Clicked story: {:?}", story);
                                                set_current_story.set(story);
                                            }
                                        >
                                            {story.label()}
                                        </a>
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    </div>
                }
            }).collect_view()}
        </aside>
    }
}

#[component]
fn StoryContent(story: ReadSignal<Story>) -> impl IntoView {
    view! {
        <Show when=move || story.get() == Story::Welcome fallback=|| ()>
            <stories::WelcomeStory />
        </Show>
        // Layout
        <Show when=move || story.get() == Story::AccordionComponent fallback=|| ()>
            <stories::AccordionStory />
        </Show>
        <Show when=move || story.get() == Story::CardComponent fallback=|| ()>
            <stories::CardStory />
        </Show>
        <Show when=move || story.get() == Story::ModalComponent fallback=|| ()>
            <stories::ModalStory />
        </Show>
        <Show when=move || story.get() == Story::ModalStackComponent fallback=|| ()>
            <stories::ModalStackStory />
        </Show>
        <Show when=move || story.get() == Story::TabsComponent fallback=|| ()>
            <stories::TabsStory />
        </Show>
        <Show when=move || story.get() == Story::HeaderComponent fallback=|| ()>
            <stories::HeaderStory />
        </Show>
        // Feedback
        <Show when=move || story.get() == Story::LoadingOverlayComponent fallback=|| ()>
            <stories::LoadingOverlayStory />
        </Show>
        <Show when=move || story.get() == Story::SkeletonComponent fallback=|| ()>
            <stories::SkeletonStory />
        </Show>
        <Show when=move || story.get() == Story::AlertComponent fallback=|| ()>
            <stories::AlertStory />
        </Show>
        // User
        <Show when=move || story.get() == Story::UserAvatarComponent fallback=|| ()>
            <stories::UserAvatarStory />
        </Show>
        <Show when=move || story.get() == Story::RoleDotsComponent fallback=|| ()>
            <stories::RoleDotsStory />
        </Show>
        <Show when=move || story.get() == Story::PlayerCardComponent fallback=|| ()>
            <stories::PlayerCardStory />
        </Show>
        // Data Display
        <Show when=move || story.get() == Story::ImageCardComponent fallback=|| ()>
            <stories::ImageCardStory />
        </Show>
        <Show when=move || story.get() == Story::AssetCardComponent fallback=|| ()>
            <stories::AssetCardStory />
        </Show>
        <Show when=move || story.get() == Story::AssetGridComponent fallback=|| ()>
            <stories::AssetGridStory />
        </Show>
        <Show when=move || story.get() == Story::AssetPickerComponent fallback=|| ()>
            <stories::AssetPickerStory />
        </Show>
        <Show when=move || story.get() == Story::AssetDetailCardComponent fallback=|| ()>
            <stories::AssetDetailCardStory />
        </Show>
        <Show when=move || story.get() == Story::AssetCacheComponent fallback=|| ()>
            <stories::AssetCacheStory />
        </Show>
        <Show when=move || story.get() == Story::ConnectionStatusComponent fallback=|| ()>
            <stories::ConnectionStatusStory />
        </Show>
        <Show when=move || story.get() == Story::MemoryCardComponent fallback=|| ()>
            <stories::MemoryCardStory />
        </Show>
        <Show when=move || story.get() == Story::StatPillComponent fallback=|| ()>
            <stories::StatPillStory />
        </Show>
        <Show when=move || story.get() == Story::BadgeComponent fallback=|| ()>
            <stories::BadgeStory />
        </Show>
        <Show when=move || story.get() == Story::EmptyStateComponent fallback=|| ()>
            <stories::EmptyStateStory />
        </Show>
        <Show when=move || story.get() == Story::ProgressBarComponent fallback=|| ()>
            <stories::ProgressBarStory />
        </Show>
        <Show when=move || story.get() == Story::InfoGridComponent fallback=|| ()>
            <stories::InfoGridStory />
        </Show>
        <Show when=move || story.get() == Story::ColorSwatchComponent fallback=|| ()>
            <stories::ColorSwatchStory />
        </Show>
        <Show when=move || story.get() == Story::RatingComponent fallback=|| ()>
            <stories::RatingStory />
        </Show>
        // Forms
        <Show when=move || story.get() == Story::ButtonComponent fallback=|| ()>
            <stories::ButtonStory />
        </Show>
        <Show when=move || story.get() == Story::ButtonGroupComponent fallback=|| ()>
            <stories::ButtonGroupStory />
        </Show>
        <Show when=move || story.get() == Story::SelectComponent fallback=|| ()>
            <stories::SelectStory />
        </Show>
        <Show when=move || story.get() == Story::TextInputComponent fallback=|| ()>
            <stories::TextInputStory />
        </Show>
        <Show when=move || story.get() == Story::TextareaComponent fallback=|| ()>
            <stories::TextareaStory />
        </Show>
        <Show when=move || story.get() == Story::FormGroupComponent fallback=|| ()>
            <stories::FormGroupStory />
        </Show>
        // Editors
        <Show when=move || story.get() == Story::DropEditorComponent fallback=|| ()>
            <stories::DropEditorStory />
        </Show>
        // Hooks
        <Show when=move || story.get() == Story::UseDraggableHook fallback=|| ()>
            <stories::UseDraggableStory />
        </Show>
        // Wallet
        <Show when=move || story.get() == Story::WalletProviders fallback=|| ()>
            <stories::WalletProvidersStory />
        </Show>
        <Show when=move || story.get() == Story::ConnectionStates fallback=|| ()>
            <stories::ConnectionStatesStory />
        </Show>
        <Show when=move || story.get() == Story::WalletDetection fallback=|| ()>
            <stories::WalletDetectionStory />
        </Show>
        <Show when=move || story.get() == Story::WalletConnection fallback=|| ()>
            <stories::WalletConnectionStory />
        </Show>
        <Show when=move || story.get() == Story::WalletBalance fallback=|| ()>
            <stories::WalletBalanceStory />
        </Show>
        <Show when=move || story.get() == Story::WalletNfts fallback=|| ()>
            <stories::WalletNftsStory />
        </Show>
        <Show when=move || story.get() == Story::WalletLeptos fallback=|| ()>
            <stories::WalletLeptosStory />
        </Show>
        <Show when=move || story.get() == Story::FlowOverview fallback=|| ()>
            <stories::FlowOverviewStory />
        </Show>
        <Show when=move || story.get() == Story::FlowState fallback=|| ()>
            <stories::FlowStateStory />
        </Show>
        <Show when=move || story.get() == Story::FlowOperations fallback=|| ()>
            <stories::FlowOperationsStory />
        </Show>
        <Show when=move || story.get() == Story::LoadingStates fallback=|| ()>
            <stories::LoadingStatesStory />
        </Show>
        <Show when=move || story.get() == Story::LoaderConfig fallback=|| ()>
            <stories::LoaderConfigStory />
        </Show>
        <Show when=move || story.get() == Story::ToastTypes fallback=|| ()>
            <stories::ToastTypesStory />
        </Show>
        <Show when=move || story.get() == Story::ToastUsage fallback=|| ()>
            <stories::ToastUsageStory />
        </Show>
    }
}
