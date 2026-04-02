use dioxus::prelude::*;

const GETTING_STARTED: &str = include_str!("../../guides/getting-started.md");
const CUSTOM_SPRITES: &str  = include_str!("../../guides/custom-sprites.md");
const STATE_MACHINES: &str  = include_str!("../../guides/state-machines.md");
const CONFIGURATION: &str   = include_str!("../../guides/configuration.md");

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
        div { class: "max-w-2xl mx-auto px-6 py-16 prose prose-slate",
            dangerous_inner_html: markdown_to_html(content)
        }
    }
}

fn markdown_to_html(md: &str) -> String {
    // Simple markdown: convert headings and paragraphs
    let mut html = String::new();
    for line in md.lines() {
        if let Some(rest) = line.strip_prefix("# ") {
            html.push_str(&format!("<h1 class=\"text-3xl font-bold mb-6\">{}</h1>\n", rest));
        } else if let Some(rest) = line.strip_prefix("## ") {
            html.push_str(&format!("<h2 class=\"text-xl font-bold mt-8 mb-3\">{}</h2>\n", rest));
        } else if let Some(rest) = line.strip_prefix("- ") {
            html.push_str(&format!("<li>{}</li>\n", rest));
        } else if line.is_empty() {
            html.push_str("<br/>\n");
        } else {
            html.push_str(&format!("<p class=\"mb-4\">{}</p>\n", line));
        }
    }
    html
}
