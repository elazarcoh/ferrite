use dioxus::prelude::*;
use crate::app::Route;

#[component]
pub fn Home() -> Element {
    rsx! {
        // Hero
        section { class: "bg-slate-900 text-white py-24 px-6 text-center",
            h1 { class: "font-pixel text-indigo-400 text-3xl mb-4 leading-snug",
                "Ferrite"
            }
            p { class: "text-slate-300 text-lg max-w-xl mx-auto mb-8",
                "A tiny animated pet that lives on your Windows desktop."
            }
            a {
                href: "https://github.com/elazarcoh/ferrite/releases/latest",
                class: "inline-block bg-indigo-600 hover:bg-indigo-500 text-white font-semibold px-8 py-3 rounded-lg transition",
                "Download for Windows"
            }
        }

        // Pet hint
        section { class: "max-w-3xl mx-auto px-6 py-12 text-center",
            p { class: "text-slate-500 text-base",
                "There's a little sheep wandering this page — try dragging and throwing it!"
            }
        }

        // Features
        section { class: "bg-sky-50 py-16 px-6",
            div { class: "max-w-4xl mx-auto grid grid-cols-1 md:grid-cols-3 gap-8 text-center",
                FeatureCard {
                    title: "Animated",
                    body: "Smooth frame-by-frame sprite animations with Aseprite support."
                }
                FeatureCard {
                    title: "Custom Sprites",
                    body: "Import your own spritesheet and bring any character to life."
                }
                FeatureCard {
                    title: "Scriptable",
                    body: "Control behavior with a simple TOML state machine DSL."
                }
            }
        }

        // Download CTA
        section { class: "py-16 px-6 text-center",
            h2 { class: "text-slate-900 text-2xl font-bold mb-4", "Ready to try it?" }
            a {
                href: "https://github.com/elazarcoh/ferrite/releases/latest",
                class: "inline-block bg-indigo-600 hover:bg-indigo-500 text-white font-semibold px-8 py-3 rounded-lg transition mr-4",
                "Download"
            }
            Link {
                to: Route::GuideIndex {},
                class: "inline-block border border-indigo-600 text-indigo-600 hover:bg-indigo-50 font-semibold px-8 py-3 rounded-lg transition",
                "Read the Guides"
            }
        }
    }
}

#[component]
fn FeatureCard(title: &'static str, body: &'static str) -> Element {
    rsx! {
        div { class: "bg-white rounded-2xl shadow p-8",
            h3 { class: "text-slate-900 font-bold text-lg mb-2", "{title}" }
            p { class: "text-slate-600 text-sm", "{body}" }
        }
    }
}
