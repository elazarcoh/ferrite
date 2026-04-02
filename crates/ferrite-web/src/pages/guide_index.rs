use dioxus::prelude::*;
use crate::app::Route;

#[component]
pub fn GuideIndex() -> Element {
    let guides = [
        ("getting-started", "Getting Started"),
        ("custom-sprites", "Custom Sprites"),
        ("state-machines", "State Machines"),
        ("configuration", "Configuration"),
    ];
    rsx! {
        div { class: "max-w-2xl mx-auto px-6 py-16",
            h1 { class: "text-3xl font-bold text-slate-900 mb-8", "Guides" }
            ul { class: "space-y-4",
                for (slug, title) in guides {
                    li {
                        Link {
                            to: Route::GuidePage { slug: slug.to_string() },
                            class: "block p-4 bg-sky-50 hover:bg-indigo-50 rounded-xl text-indigo-700 font-semibold transition",
                            "{title}"
                        }
                    }
                }
            }
        }
    }
}
