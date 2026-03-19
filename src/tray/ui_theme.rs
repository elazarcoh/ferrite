use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};

/// Apply the dark or light theme to the egui context.
/// Two separate calls: `set_visuals` for colors/rounding, `style_mut` for spacing.
pub fn apply_theme(ctx: &egui::Context, dark: bool) {
    let mut vis = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    // Corner radius configuration — same for both modes
    vis.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    vis.widgets.inactive.corner_radius = CornerRadius::same(4);
    vis.widgets.hovered.corner_radius = CornerRadius::same(4);
    vis.widgets.active.corner_radius = CornerRadius::same(4);
    vis.widgets.open.corner_radius = CornerRadius::same(4);

    // Accent (green selection highlight) — same for both modes
    let accent = Color32::from_rgb(72, 200, 120);
    vis.selection.bg_fill = accent;
    vis.selection.stroke = Stroke::new(1.0, accent);

    if dark {
        vis.window_fill = Color32::from_rgb(18, 18, 30);
        vis.panel_fill = Color32::from_rgb(24, 24, 42);
        vis.widgets.noninteractive.bg_fill = Color32::from_rgb(24, 24, 42);
        vis.widgets.inactive.bg_fill = Color32::from_rgb(35, 35, 60);
        vis.widgets.hovered.bg_fill = Color32::from_rgb(45, 45, 75);
        vis.widgets.noninteractive.bg_stroke =
            Stroke::new(1.0, Color32::from_rgba_premultiplied(100, 120, 200, 60));
        vis.widgets.inactive.bg_stroke =
            Stroke::new(1.0, Color32::from_rgba_premultiplied(100, 120, 200, 60));
        vis.override_text_color = Some(Color32::from_rgb(210, 215, 230));
    } else {
        vis.window_fill = Color32::from_rgb(244, 244, 248);
        vis.panel_fill = Color32::from_rgb(255, 255, 255);
        vis.widgets.noninteractive.bg_fill = Color32::from_rgb(255, 255, 255);
        vis.widgets.inactive.bg_fill = Color32::from_rgb(235, 235, 245);
        vis.widgets.hovered.bg_fill = Color32::from_rgb(220, 225, 245);
        vis.widgets.noninteractive.bg_stroke =
            Stroke::new(1.0, Color32::from_rgba_premultiplied(140, 150, 200, 120));
        vis.widgets.inactive.bg_stroke =
            Stroke::new(1.0, Color32::from_rgba_premultiplied(140, 150, 200, 120));
        vis.override_text_color = Some(Color32::from_rgb(30, 30, 50));
    }

    ctx.set_visuals(vis);

    // Spacing lives on Style, not Visuals — must be set separately.
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.spacing.window_margin = Margin::same(12);
    });
}

/// Render a small `?` label. On hover, shows `tooltip` text.
/// Usage: `ui.horizontal(|ui| { ui.label("Grid"); help_icon(ui, "..."); });`
pub fn help_icon(ui: &mut egui::Ui, tooltip: &str) {
    ui.add(egui::Label::new(
        egui::RichText::new(" ? ")
            .small()
            .color(ui.visuals().weak_text_color()),
    ))
    .on_hover_text(tooltip);
}

/// Render one line of small, italic, muted hint text below a widget.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(
        egui::RichText::new(text).small().italics().weak(),
    ));
}

/// Render a ☀/☾ toggle button. Returns `true` if the user clicked it.
/// Also calls `apply_theme(ctx, *dark)` immediately on click.
pub fn dark_light_toggle(ui: &mut egui::Ui, dark: &mut bool, ctx: &egui::Context) -> bool {
    let icon = if *dark { "☀" } else { "☾" };
    if ui.button(icon).on_hover_text(if *dark { "Switch to light mode" } else { "Switch to dark mode" }).clicked() {
        *dark = !*dark;
        apply_theme(ctx, *dark);
        return true;
    }
    false
}
