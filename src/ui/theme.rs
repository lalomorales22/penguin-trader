use eframe::egui::{self, Color32, Stroke, Vec2, Visuals};

/// Dark theme palette shared across the dashboard.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Color32,
    pub text: Color32,
    pub green: Color32,
    pub yellow: Color32,
    pub red: Color32,
    pub blue: Color32,
}

impl Theme {
    pub fn new() -> Self {
        Theme {
            background: Color32::from_rgb(10, 10, 10),      // #0a0a0a
            text: Color32::from_rgb(224, 224, 224),         // #e0e0e0
            green: Color32::from_rgb(0, 255, 136),          // #00ff88
            yellow: Color32::from_rgb(255, 170, 0),         // #ffaa00
            red: Color32::from_rgb(255, 51, 51),            // #ff3333
            blue: Color32::from_rgb(74, 158, 255),          // #4a9eff
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = Visuals::dark();
        visuals.override_text_color = Some(self.text);
        visuals.widgets.noninteractive.bg_fill = self.background;
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(22, 22, 22);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(30, 30, 30);
        visuals.widgets.active.bg_fill = Color32::from_rgb(34, 34, 34);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.2, self.text);
        visuals.panel_fill = self.background;
        visuals.hyperlink_color = self.blue;
        visuals.selection.bg_fill = self.blue.linear_multiply(0.5);
        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.button_padding = Vec2::new(10.0, 8.0);
        style.spacing.item_spacing = Vec2::new(10.0, 8.0);
        style.visuals.menu_rounding = 6.0.into();
        ctx.set_style(style);
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_spec() {
        let theme = Theme::new();
        assert_eq!(theme.background, Color32::from_rgb(10, 10, 10));
        assert_eq!(theme.text, Color32::from_rgb(224, 224, 224));
        assert_eq!(theme.green, Color32::from_rgb(0, 255, 136));
        assert_eq!(theme.yellow, Color32::from_rgb(255, 170, 0));
        assert_eq!(theme.red, Color32::from_rgb(255, 51, 51));
        assert_eq!(theme.blue, Color32::from_rgb(74, 158, 255));
    }
}
