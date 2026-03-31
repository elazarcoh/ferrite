use dioxus::prelude::*;
use crate::pages::{home::Home, guide_index::GuideIndex, guide_page::GuidePage};
use crate::components::nav::NavLayout;

#[derive(Routable, Clone, PartialEq)]
pub enum Route {
    #[layout(NavLayout)]
    #[route("/")]
    Home {},
    #[route("/guides")]
    GuideIndex {},
    #[route("/guides/:slug")]
    GuidePage { slug: String },
    #[end_layout]
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
pub fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    rsx! {
        div { class: "max-w-xl mx-auto px-6 py-24 text-center",
            h1 { class: "text-4xl font-bold text-slate-900 mb-4", "404" }
            p { class: "text-slate-600", "Page not found: /{segments.join(\"/\")}" }
        }
    }
}
