use dioxus::prelude::*;
use crate::app::Route;

const GETTING_STARTED: &str = include_str!("../../guides/getting-started.md");
const CUSTOM_SPRITES: &str  = include_str!("../../guides/custom-sprites.md");
const STATE_MACHINES: &str  = include_str!("../../guides/state-machines.md");
const CONFIGURATION: &str   = include_str!("../../guides/configuration.md");

fn markdown_to_html(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(md, opts);
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

#[component]
pub fn GuidePage(slug: String) -> Element {
    let content = match slug.as_str() {
        "getting-started" => GETTING_STARTED,
        "custom-sprites"  => CUSTOM_SPRITES,
        "state-machines"  => STATE_MACHINES,
        "configuration"   => CONFIGURATION,
        _ => "# 404\nGuide not found.",
    };

    rsx! {
        article { class: "max-w-3xl mx-auto px-6 py-12",
            Link {
                to: Route::GuideIndex {},
                class: "inline-flex items-center gap-1 text-indigo-600 text-sm font-semibold mb-10 hover:text-indigo-800 transition-colors",
                "← Guides"
            }
            div {
                class: "prose prose-slate prose-headings:font-bold prose-a:text-indigo-600 prose-code:bg-slate-100 prose-code:rounded prose-code:px-1 prose-code:text-sm prose-pre:bg-slate-900 prose-pre:text-slate-100 prose-pre:rounded-xl max-w-none [&_img]:rounded-xl [&_img]:border [&_img]:border-slate-200 [&_img]:my-6 [&_img]:w-full [&_img]:shadow-sm",
                dangerous_inner_html: markdown_to_html(content)
            }
        }
    }
}
