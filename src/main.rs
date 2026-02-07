use dioxus::{desktop::{WindowBuilder}, prelude::*};
use dioxus::desktop::{Config};
const MAIN_CSS: Asset = asset!("/src/main.css");

fn main() {
    dioxus::LaunchBuilder::new()
        .with_cfg(
            Config::default()
                .with_window(WindowBuilder::new()
                    .with_title("IRS - IMAGE RENAME SPLIT")
                    .with_maximized(true)
                )
                .with_menu(None)
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Controls {}

    }
}

#[component]
fn Controls() -> Element {
    rsx! {
        div {
            id: "controls",
            button { 
                id: "open-button",
                "OPEN" 
            }
            button { 
                id: "clear-button",
                "CLEAR"
                
            }
            button {
                id: "rename-split-button",
                "RENAME & SPLIT" 
            }
        }
        
    }
}

