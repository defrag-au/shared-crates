//! User Avatar component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{AvatarSize, UserAvatar};

// Sample Discord avatar URLs
const AVATAR_1: &str = "https://cdn.discordapp.com/guilds/1283465958945456149/users/179744071361757184/avatars/7e67374c51a831be5f10516a3df195f8.png";
const AVATAR_2: &str =
    "https://cdn.discordapp.com/avatars/806487443955384381/e68cb992f315a06ebf7d9c0963ca511c.png";
const AVATAR_3: &str =
    "https://cdn.discordapp.com/avatars/142538195202998272/f625b4e5b163d06bf49b657435958853.png";

#[component]
pub fn UserAvatarStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"User Avatar"</h2>
                <p>"A circular avatar component with fallback support. Perfect for displaying user profile pictures."</p>
            </div>

            // Size variants
            <div class="story-section">
                <h3>"Sizes"</h3>
                <p class="story-description">"Available in four sizes: Sm (24px), Md (40px), Lg (56px), and Xl (80px)."</p>
                <div class="story-canvas">
                    <div style="display: flex; align-items: center; gap: 1.5rem;">
                        <div style="text-align: center;">
                            <UserAvatar src=AVATAR_1 size=AvatarSize::Sm />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Sm"</p>
                        </div>
                        <div style="text-align: center;">
                            <UserAvatar src=AVATAR_1 size=AvatarSize::Md />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Md (default)"</p>
                        </div>
                        <div style="text-align: center;">
                            <UserAvatar src=AVATAR_1 size=AvatarSize::Lg />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Lg"</p>
                        </div>
                        <div style="text-align: center;">
                            <UserAvatar src=AVATAR_1 size=AvatarSize::Xl />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Xl"</p>
                        </div>
                    </div>
                </div>
            </div>

            // Fallback states
            <div class="story-section">
                <h3>"Fallback States"</h3>
                <p class="story-description">"When no image is provided, shows a default icon or custom fallback content."</p>
                <div class="story-canvas">
                    <div style="display: flex; align-items: center; gap: 1.5rem;">
                        <div style="text-align: center;">
                            <UserAvatar size=AvatarSize::Lg />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Default"</p>
                        </div>
                        <div style="text-align: center;">
                            <UserAvatar size=AvatarSize::Lg fallback="JD" />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Initials"</p>
                        </div>
                        <div style="text-align: center;">
                            <UserAvatar size=AvatarSize::Lg fallback="🏴‍☠️" />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Emoji"</p>
                        </div>
                        <div style="text-align: center;">
                            <UserAvatar size=AvatarSize::Lg fallback="?" />
                            <p style="font-size: 0.75rem; color: #888; margin-top: 0.5rem;">"Unknown"</p>
                        </div>
                    </div>
                </div>
            </div>

            // Different images
            <div class="story-section">
                <h3>"Avatar Gallery"</h3>
                <p class="story-description">"Avatars with different images."</p>
                <div class="story-canvas">
                    <div style="display: flex; gap: 0.75rem;">
                        <UserAvatar src=AVATAR_1 alt="User 1" />
                        <UserAvatar src=AVATAR_2 alt="User 2" />
                        <UserAvatar src=AVATAR_3 alt="User 3" />
                        <UserAvatar fallback="+" />
                    </div>
                </div>
            </div>

            // In context
            <div class="story-section">
                <h3>"In Context"</h3>
                <p class="story-description">"Avatars used in a user list layout."</p>
                <div class="story-canvas">
                    <div style="display: flex; flex-direction: column; gap: 0.75rem; max-width: 300px;">
                        <div style="display: flex; align-items: center; gap: 0.75rem; padding: 0.5rem; background: rgba(255,255,255,0.02); border-radius: 6px;">
                            <UserAvatar src=AVATAR_1 />
                            <div>
                                <div style="font-weight: 500;">"Captain Jack"</div>
                                <div style="font-size: 0.75rem; color: #888;">"Online"</div>
                            </div>
                        </div>
                        <div style="display: flex; align-items: center; gap: 0.75rem; padding: 0.5rem; background: rgba(255,255,255,0.02); border-radius: 6px;">
                            <UserAvatar fallback="AB" />
                            <div>
                                <div style="font-weight: 500;">"Anne Bonny"</div>
                                <div style="font-size: 0.75rem; color: #888;">"Away"</div>
                            </div>
                        </div>
                        <div style="display: flex; align-items: center; gap: 0.75rem; padding: 0.5rem; background: rgba(255,255,255,0.02); border-radius: 6px;">
                            <UserAvatar src=AVATAR_2 />
                            <div>
                                <div style="font-weight: 500;">"Blackbeard"</div>
                                <div style="font-size: 0.75rem; color: #888;">"Offline"</div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="src"
                            values="String (optional)"
                            description="URL of the avatar image. If not provided, shows fallback."
                        />
                        <AttributeCard
                            name="alt"
                            values="String (optional)"
                            description="Alt text for the image for accessibility."
                        />
                        <AttributeCard
                            name="size"
                            values="AvatarSize (Sm, Md, Lg, Xl)"
                            description="Size variant. Defaults to Md (40px)."
                        />
                        <AttributeCard
                            name="fallback"
                            values="String (optional)"
                            description="Custom fallback content (initials, emoji). Defaults to user icon."
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{UserAvatar, AvatarSize};

// With image
view! {
    <UserAvatar
        src="https://example.com/avatar.jpg"
        alt="John Doe"
        size=AvatarSize::Lg
    />
}

// Without image (default fallback)
view! {
    <UserAvatar alt="Unknown User" />
}

// With custom fallback (initials)
view! {
    <UserAvatar fallback="JD" />
}

// Different sizes
view! {
    <UserAvatar src=url size=AvatarSize::Sm />  // 24px
    <UserAvatar src=url size=AvatarSize::Md />  // 40px (default)
    <UserAvatar src=url size=AvatarSize::Lg />  // 56px
    <UserAvatar src=url size=AvatarSize::Xl />  // 80px
}"##}</pre>
            </div>
        </div>
    }
}
