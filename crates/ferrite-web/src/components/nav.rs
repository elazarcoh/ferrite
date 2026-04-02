use dioxus::prelude::*;
use crate::app::Route;

#[component]
pub fn NavLayout() -> Element {
    rsx! {
        nav { class: "sticky top-0 z-50 bg-slate-900 text-white px-6 py-4 flex items-center gap-8 shadow",
            span { class: "font-pixel text-indigo-400 text-sm", "Ferrite" }
            Link { to: Route::Home {}, class: "text-slate-300 hover:text-white text-sm", "Home" }
            Link { to: Route::GuideIndex {}, class: "text-slate-300 hover:text-white text-sm", "Guides" }
            a {
                href: "https://github.com/elazarcoh/ferrite",
                target: "_blank",
                class: "ml-auto text-slate-400 hover:text-white text-sm",
                "GitHub"
            }
        }
        Outlet::<Route> {}
        crate::components::footer::Footer {}
    }
}
