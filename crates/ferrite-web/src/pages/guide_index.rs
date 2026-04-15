use dioxus::prelude::*;
use crate::app::Route;

#[component]
pub fn GuideIndex() -> Element {
    // (slug, title, subtitle, gradient-start, gradient-end)
    let guides = [
        ("getting-started", "Getting Started",  "Install and run your first pet",   "#6366f1", "#8b5cf6"),
        ("custom-sprites",  "Custom Sprites",   "Import your own artwork",           "#0ea5e9", "#06b6d4"),
        ("state-machines",  "State Machines",   "Animate pet behaviour",             "#f59e0b", "#f97316"),
        ("configuration",   "Configuration",    "Tweak speed, scale and more",       "#10b981", "#059669"),
    ];

    rsx! {
        div { class: "max-w-3xl mx-auto px-6 py-16",
            p { class: "text-sm font-semibold text-indigo-600 tracking-widest uppercase mb-2",
                "Ferrite"
            }
            h1 { class: "text-4xl font-extrabold text-slate-900 mb-3", "Guides" }
            p { class: "text-slate-500 text-lg mb-10",
                "Everything you need to know about Ferrite"
            }
            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-6",
                for (slug, title, subtitle, c1, c2) in guides {
                    Link {
                        to: Route::GuidePage { slug: slug.to_string() },
                        class: "group block rounded-2xl border border-slate-200 overflow-hidden hover:shadow-xl hover:-translate-y-0.5 transition-all duration-150",
                        // Screenshot image with gradient fallback — CSS background-image
                        // tries the PNG first; falls back to the gradient if file is absent.
                        div {
                            style: format!(
                                "height:112px; background-image: url('/ferrite/assets/guides/{slug}.png'), linear-gradient(135deg, {c1}, {c2}); background-size: cover; background-position: center top;"
                            ),
                        }
                        div { class: "p-5",
                            h2 { class: "font-bold text-slate-900 group-hover:text-indigo-700 transition-colors text-lg",
                                "{title}"
                            }
                            p { class: "text-sm text-slate-500 mt-1", "{subtitle}" }
                        }
                    }
                }
            }
        }
    }
}
