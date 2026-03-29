use dioxus::prelude::*;

#[component]
pub fn Footer() -> Element {
    rsx! {
        footer { class: "bg-slate-900 text-slate-500 text-center py-6 text-xs mt-16",
            p { "Ferrite — open source desktop pet for Windows" }
        }
    }
}
