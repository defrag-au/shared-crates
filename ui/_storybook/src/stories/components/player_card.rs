//! Player Card component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{Button, ButtonVariant, PlayerCard};

// Sample Discord avatar URLs
const AVATAR_1: &str = "https://cdn.discordapp.com/guilds/1283465958945456149/users/179744071361757184/avatars/7e67374c51a831be5f10516a3df195f8.png";
const AVATAR_2: &str =
    "https://cdn.discordapp.com/avatars/806487443955384381/e68cb992f315a06ebf7d9c0963ca511c.png";
const AVATAR_3: &str =
    "https://cdn.discordapp.com/avatars/142538195202998272/f625b4e5b163d06bf49b657435958853.png";

// Sample tribe render URLs
const TRIBE_1: &str =
    "https://img.tribes.augmint.bot/blackflag/142538195202998272.png?t=1769293515482";
const TRIBE_2: &str =
    "https://img.tribes.augmint.bot/blackflag/700703522487795815.png?t=1769293515482";

#[component]
pub fn PlayerCardStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Player Card"</h2>
                <p>"A card component for displaying player/user information with avatar, optional hero image, metadata, and action buttons."</p>
            </div>

            // Basic example
            <div class="story-section">
                <h3>"Basic Usage"</h3>
                <p class="story-description">"A player card with hero image, avatar, and action buttons."</p>
                <div class="story-canvas">
                    <div style="max-width: 280px;">
                        <PlayerCard
                            name="Captain Jack"
                            subtitle="The Black Pearl"
                            badge="Admiral"
                            avatar_url=AVATAR_1
                            hero_url=TRIBE_1
                            meta=vec!["Position: (10, 5)".to_string(), "3 intents".to_string()]
                        >
                            <Button variant=ButtonVariant::Primary on_click=move || {}>"Details"</Button>
                            <Button variant=ButtonVariant::Secondary on_click=move || {}>"Render"</Button>
                        </PlayerCard>
                    </div>
                </div>
            </div>

            // Without hero image
            <div class="story-section">
                <h3>"Without Hero Image"</h3>
                <p class="story-description">"Card with avatar but no hero/banner image."</p>
                <div class="story-canvas">
                    <div style="max-width: 280px;">
                        <PlayerCard
                            name="Anne Bonny"
                            subtitle="The Revenge"
                            badge="Captain"
                            avatar_url=AVATAR_2
                            meta=vec!["Online".to_string()]
                        >
                            <Button variant=ButtonVariant::Primary on_click=move || {}>"View Profile"</Button>
                        </PlayerCard>
                    </div>
                </div>
            </div>

            // Minimal
            <div class="story-section">
                <h3>"Minimal"</h3>
                <p class="story-description">"Just the essentials - name only, with fallback avatar."</p>
                <div class="story-canvas">
                    <div style="max-width: 280px;">
                        <PlayerCard name="Unknown Pirate" />
                    </div>
                </div>
            </div>

            // Grid of players
            <div class="story-section">
                <h3>"Player Grid"</h3>
                <p class="story-description">"Multiple player cards in a responsive grid."</p>
                <div class="story-canvas">
                    <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; max-width: 900px;">
                        <PlayerCard
                            name="Captain Jack"
                            subtitle="The Black Pearl"
                            badge="Admiral"
                            avatar_url=AVATAR_1
                            hero_url=TRIBE_1
                        >
                            <Button variant=ButtonVariant::Primary on_click=move || {}>"Details"</Button>
                        </PlayerCard>
                        <PlayerCard
                            name="Blackbeard"
                            subtitle="Queen Anne's Revenge"
                            badge="Pirate Lord"
                            avatar_url=AVATAR_2
                            hero_url=TRIBE_2
                        >
                            <Button variant=ButtonVariant::Primary on_click=move || {}>"Details"</Button>
                        </PlayerCard>
                        <PlayerCard
                            name="Anne Bonny"
                            subtitle="The Revenge"
                            badge="Captain"
                            avatar_url=AVATAR_3
                        >
                            <Button variant=ButtonVariant::Primary on_click=move || {}>"Details"</Button>
                        </PlayerCard>
                    </div>
                </div>
            </div>

            // Clickable
            <div class="story-section">
                <h3>"Clickable Card"</h3>
                <p class="story-description">"Card with click handler (without action buttons)."</p>
                <div class="story-canvas">
                    <div style="max-width: 280px;">
                        <PlayerCard
                            name="Calico Jack"
                            subtitle="The William"
                            badge="Captain"
                            avatar_url=AVATAR_3
                            hero_url=TRIBE_2
                            on_click=move || {
                                web_sys::window()
                                    .and_then(|w| w.alert_with_message("Card clicked!").ok());
                            }
                        />
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="name"
                            values="String"
                            description="Player display name (required)"
                        />
                        <AttributeCard
                            name="subtitle"
                            values="String (optional)"
                            description="Subtitle text (entity name, ship name, role, etc.)"
                        />
                        <AttributeCard
                            name="badge"
                            values="String (optional)"
                            description="Badge text (tier, rank, status)"
                        />
                        <AttributeCard
                            name="avatar_url"
                            values="String (optional)"
                            description="URL for avatar image. Shows fallback icon if not provided."
                        />
                        <AttributeCard
                            name="hero_url"
                            values="String (optional)"
                            description="URL for hero/banner image at top of card"
                        />
                        <AttributeCard
                            name="meta"
                            values="Vec<String> (optional)"
                            description="Metadata items to display (position, status, etc.)"
                        />
                        <AttributeCard
                            name="on_click"
                            values="Callback<()> (optional)"
                            description="Click handler for the card. Makes card interactive."
                        />
                        <AttributeCard
                            name="children"
                            values="Children (optional)"
                            description="Action buttons slot, displayed at bottom of card"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{PlayerCard, Button, ButtonVariant};

// Full-featured card
view! {
    <PlayerCard
        name="Captain Jack"
        subtitle="The Black Pearl"
        badge="Admiral"
        avatar_url="https://..."
        hero_url="https://..."
        meta=vec!["Position: (10, 5)".to_string(), "3 intents".to_string()]
    >
        <Button variant=ButtonVariant::Primary on_click=details>
            "Details"
        </Button>
        <Button variant=ButtonVariant::Secondary on_click=render>
            "Render"
        </Button>
    </PlayerCard>
}

// Minimal card
view! {
    <PlayerCard name="Unknown Pirate" />
}

// Clickable card (no buttons)
view! {
    <PlayerCard
        name="Calico Jack"
        subtitle="The William"
        on_click=move || { /* handle click */ }
    />
}"##}</pre>
            </div>
        </div>
    }
}
