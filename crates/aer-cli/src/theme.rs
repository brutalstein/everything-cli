use std::env;

use ratatui::style::Color;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Glyphs {
    pub home: &'static str,
    pub workspace: &'static str,
    pub environment: &'static str,
    pub providers: &'static str,
    pub activity: &'static str,
    pub settings: &'static str,
    pub ok: &'static str,
    pub ready: &'static str,
    pub attention: &'static str,
    pub arrow: &'static str,
    pub command: &'static str,
    pub branch: &'static str,
    pub terminal: &'static str,
    pub shield: &'static str,
}

impl Glyphs {
    fn unicode() -> Self {
        Self {
            home: "⌂",
            workspace: "▣",
            environment: "◇",
            providers: "◆",
            activity: "◉",
            settings: "⚙",
            ok: "✓",
            ready: "●",
            attention: "!",
            arrow: "→",
            command: "⌘",
            branch: "⑂",
            terminal: "›_",
            shield: "◈",
        }
    }

    fn ascii() -> Self {
        Self {
            home: "H",
            workspace: "W",
            environment: "E",
            providers: "P",
            activity: "A",
            settings: "S",
            ok: "+",
            ready: "*",
            attention: "!",
            arrow: "->",
            command: ">",
            branch: "git",
            terminal: ">_",
            shield: "#",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_alt: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub glyphs: Glyphs,
}

impl Theme {
    #[must_use]
    pub fn discover() -> Self {
        let no_color = env::var_os("NO_COLOR").is_some();
        let ascii = env::var("EVERYTHING_ASCII")
            .ok()
            .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
        let truecolor = !no_color
            && env::var("COLORTERM")
                .ok()
                .is_some_and(|value| value.to_ascii_lowercase().contains("truecolor"));

        if no_color {
            return Self {
                background: Color::Reset,
                panel: Color::Reset,
                border: Color::DarkGray,
                text: Color::Reset,
                muted: Color::DarkGray,
                accent: Color::Reset,
                accent_alt: Color::Reset,
                success: Color::Reset,
                warning: Color::Reset,
                danger: Color::Reset,
                glyphs: if ascii { Glyphs::ascii() } else { Glyphs::unicode() },
            };
        }

        if truecolor {
            Self {
                background: Color::Rgb(3, 9, 16),
                panel: Color::Rgb(7, 16, 27),
                border: Color::Rgb(43, 62, 80),
                text: Color::Rgb(222, 231, 239),
                muted: Color::Rgb(111, 127, 145),
                accent: Color::Rgb(34, 211, 238),
                accent_alt: Color::Rgb(168, 85, 247),
                success: Color::Rgb(74, 222, 128),
                warning: Color::Rgb(250, 204, 21),
                danger: Color::Rgb(248, 113, 113),
                glyphs: if ascii { Glyphs::ascii() } else { Glyphs::unicode() },
            }
        } else {
            Self {
                background: Color::Black,
                panel: Color::Black,
                border: Color::DarkGray,
                text: Color::White,
                muted: Color::DarkGray,
                accent: Color::Cyan,
                accent_alt: Color::Magenta,
                success: Color::Green,
                warning: Color::Yellow,
                danger: Color::Red,
                glyphs: if ascii { Glyphs::ascii() } else { Glyphs::unicode() },
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            background: Color::Black,
            panel: Color::Black,
            border: Color::DarkGray,
            text: Color::White,
            muted: Color::DarkGray,
            accent: Color::Cyan,
            accent_alt: Color::Magenta,
            success: Color::Green,
            warning: Color::Yellow,
            danger: Color::Red,
            glyphs: Glyphs::unicode(),
        }
    }
}
