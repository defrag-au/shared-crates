//! Tabs component story

use crate::stories::helpers::AttributeCard;
use leptos::prelude::*;
use ui_components::{TabPanel, Tabs};

#[component]
pub fn TabsStory() -> impl IntoView {
    view! {
        <div>
            <div class="story-header">
                <h2>"Tabs"</h2>
                <p>"Tab navigation components for switching between content panels. Uses TabPanel children for content."</p>
            </div>

            // Basic Tabs section
            <div class="story-section">
                <h3>"Basic Tabs"</h3>
                <p style="color: #888; margin-bottom: 1rem;">"Tabs automatically provides context to TabPanel children for visibility control."</p>
                <div class="story-canvas">
                    {
                        let (tab, set_tab) = signal("tab1".to_string());
                        view! {
                            <Tabs
                                active=tab
                                on_change=Callback::new(move |id| set_tab.set(id))
                                tabs=vec![
                                    ("tab1".to_string(), "First Tab".to_string()),
                                    ("tab2".to_string(), "Second Tab".to_string()),
                                    ("tab3".to_string(), "Third Tab".to_string()),
                                ]
                            >
                                <TabPanel value="tab1">"Content for the first tab"</TabPanel>
                                <TabPanel value="tab2">"Content for the second tab"</TabPanel>
                                <TabPanel value="tab3">"Content for the third tab"</TabPanel>
                            </Tabs>
                        }
                    }
                </div>
            </div>

            // External control
            <div class="story-section">
                <h3>"External Control"</h3>
                <p style="color: #888; margin-bottom: 1rem;">"Tab state can be controlled externally via signals."</p>
                <div class="story-canvas">
                    {
                        let (active_tab, set_active_tab) = signal("overview".to_string());
                        view! {
                            <Tabs
                                active=active_tab
                                on_change=Callback::new(move |id| set_active_tab.set(id))
                                tabs=vec![
                                    ("overview".to_string(), "Overview".to_string()),
                                    ("settings".to_string(), "Settings".to_string()),
                                    ("logs".to_string(), "Logs".to_string()),
                                ]
                            >
                                <TabPanel value="overview">"Overview panel content - shows dashboard and stats"</TabPanel>
                                <TabPanel value="settings">"Settings panel content - configure your preferences"</TabPanel>
                                <TabPanel value="logs">"Logs panel content - view activity history"</TabPanel>
                            </Tabs>

                            <div style="margin-top: 1rem; display: flex; gap: 0.5rem;">
                                <button class="btn btn--secondary btn--sm" on:click=move |_| set_active_tab.set("overview".into())>"Go to Overview"</button>
                                <button class="btn btn--secondary btn--sm" on:click=move |_| set_active_tab.set("settings".into())>"Go to Settings"</button>
                                <button class="btn btn--secondary btn--sm" on:click=move |_| set_active_tab.set("logs".into())>"Go to Logs"</button>
                            </div>
                        }
                    }
                </div>
            </div>

            // Props section
            <div class="story-section">
                <h3>"Props - Tabs"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="active"
                            values="Signal<String>"
                            description="Signal for the currently active tab value"
                        />
                        <AttributeCard
                            name="on_change"
                            values="Callback<String>"
                            description="Callback when tab selection changes"
                        />
                        <AttributeCard
                            name="tabs"
                            values="Vec<(String, String)>"
                            description="Tab definitions as (value, label) tuples"
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="TabPanel components for content"
                        />
                    </div>
                </div>
            </div>

            <div class="story-section">
                <h3>"Props - TabPanel"</h3>
                <div class="story-canvas">
                    <div class="story-grid">
                        <AttributeCard
                            name="value"
                            values="String"
                            description="Value that matches the tab to show this panel"
                        />
                        <AttributeCard
                            name="children"
                            values="Children"
                            description="Panel content"
                        />
                    </div>
                </div>
            </div>

            // Code example
            <div class="story-section">
                <h3>"Usage"</h3>
                <pre class="code-block">{r##"use ui_components::{Tabs, TabPanel};

let (active, set_active) = signal("tab1".to_string());

view! {
    <Tabs
        active=active
        on_change=Callback::new(move |id| set_active.set(id))
        tabs=vec![
            ("tab1".to_string(), "Tab 1".to_string()),
            ("tab2".to_string(), "Tab 2".to_string()),
        ]
    >
        <TabPanel value="tab1">"Tab 1 content"</TabPanel>
        <TabPanel value="tab2">"Tab 2 content"</TabPanel>
    </Tabs>
}"##}</pre>
            </div>
        </div>
    }
}
