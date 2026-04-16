use ratatui::style::Color;
use serde::Deserialize;

/// All semantic color roles used throughout the TUI.
/// Each field is a hex color string in YAML (e.g. "#61A2FF") that gets
/// deserialized into a `ratatui::style::Color::Rgb`.
#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    #[serde(deserialize_with = "deserialize_color")]
    pub border_focused: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub border_normal: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub text_subtle: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub text_error: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub text_warning: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub text_primary: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub text_user: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub text_agent: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub thought_dim: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub thought_tool: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub thought_reasoning: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub thought_result: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub highlight_bg: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub file_text: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub file_dir: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub file_highlight_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border_focused: Color::Rgb(0x61, 0xA2, 0xFF), // blue60
            border_normal: Color::Rgb(0x48, 0x59, 0x75),  // blueGrey100
            text_subtle: Color::Rgb(0x8E, 0x9F, 0xBC),    // blueGrey60
            text_error: Color::Rgb(0xF6, 0x72, 0x6A),     // red60
            text_warning: Color::Rgb(0xFA, 0xCB, 0x3D),   // yellow40
            text_primary: Color::Rgb(0x61, 0xA2, 0xFF),   // blue60
            text_user: Color::Rgb(0x24, 0xC2, 0x92),      // green60
            text_agent: Color::Rgb(0xEE, 0x72, 0xA6),     // pink60
            thought_dim: Color::Rgb(0x51, 0x63, 0x81),    // blueGrey95
            thought_tool: Color::Rgb(0x16, 0xC5, 0xC0),   // teal60
            thought_reasoning: Color::Rgb(0xFA, 0xCB, 0x3D), // yellow40
            thought_result: Color::Rgb(0x24, 0xC2, 0x92),    // green60
            highlight_bg: Color::Rgb(0x24, 0x31, 0x47),    // blueGrey125
            file_text: Color::Rgb(0xCA, 0xD3, 0xE2),      // blueGrey30
            file_dir: Color::Rgb(0x16, 0xC5, 0xC0),       // teal60
            file_highlight_bg: Color::Rgb(0x2B, 0x39, 0x4F), // blueGrey120
        }
    }
}

impl Theme {
    /// Load a theme from a YAML file. Falls back to built-in defaults on error.
    pub fn load(path: &str) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match serde_yaml::from_str::<Theme>(&contents) {
                Ok(theme) => theme,
                Err(e) => {
                    eprintln!("Warning: failed to parse theme {path}: {e}. Using defaults.");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }
}

fn deserialize_color<'de, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    parse_hex_color(&s).map_err(serde::de::Error::custom)
}

fn parse_hex_color(s: &str) -> Result<Color, String> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return Err(format!("expected 6-char hex color, got \"{s}\""));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| format!("bad red in \"{s}\": {e}"))?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| format!("bad green in \"{s}\": {e}"))?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| format!("bad blue in \"{s}\": {e}"))?;
    Ok(Color::Rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_hex() {
        assert_eq!(parse_hex_color("#FF0000").unwrap(), Color::Rgb(255, 0, 0));
        assert_eq!(parse_hex_color("00FF00").unwrap(), Color::Rgb(0, 255, 0));
    }

    #[test]
    fn parse_invalid_hex() {
        assert!(parse_hex_color("#GG0000").is_err());
        assert!(parse_hex_color("#FFF").is_err());
    }

    #[test]
    fn default_theme_is_valid() {
        let t = Theme::default();
        assert!(matches!(t.border_focused, Color::Rgb(_, _, _)));
    }

    #[test]
    fn deserialize_from_yaml() {
        let yaml = r##"
border_focused: "#61A2FF"
border_normal: "#485975"
text_subtle: "#8E9FBC"
text_error: "#F6726A"
text_warning: "#FACB3D"
text_primary: "#61A2FF"
text_user: "#24C292"
text_agent: "#EE72A6"
thought_dim: "#516381"
thought_tool: "#16C5C0"
thought_reasoning: "#FACB3D"
thought_result: "#24C292"
highlight_bg: "#243147"
file_text: "#CAD3E2"
file_dir: "#16C5C0"
file_highlight_bg: "#2B394F"
"##;
        let theme: Theme = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(theme.border_focused, Color::Rgb(0x61, 0xA2, 0xFF));
        assert_eq!(theme.text_error, Color::Rgb(0xF6, 0x72, 0x6A));
    }
}
