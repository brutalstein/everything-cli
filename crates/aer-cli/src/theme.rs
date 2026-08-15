use ratatui::style::Color;

use crate::material_icons;

#[derive(Clone, Copy, Debug)]
pub struct Glyphs {
    pub home: &'static str,
    pub intent: &'static str,
    pub research: &'static str,
    pub engineering_ir: &'static str,
    pub workspace: &'static str,
    pub environment: &'static str,
    pub providers: &'static str,
    pub activity: &'static str,
    pub settings: &'static str,
    pub branch: &'static str,
    pub shield: &'static str,
    pub ready: &'static str,
    pub attention: &'static str,
    pub command: &'static str,
    pub arrow: &'static str,
    pub ok: &'static str,
}

impl Glyphs {
    fn material() -> Self {
        Self {
            home: material_icons::HOME.compact,
            intent: material_icons::INTENT.compact,
            research: material_icons::RESEARCH.compact,
            engineering_ir: material_icons::ENGINEERING_IR.compact,
            workspace: material_icons::WORKSPACE.compact,
            environment: material_icons::ENVIRONMENT.compact,
            providers: material_icons::PROVIDERS.compact,
            activity: material_icons::ACTIVITY.compact,
            settings: material_icons::SETTINGS.compact,
            branch: material_icons::BRANCH.compact,
            shield: material_icons::SHIELD.compact,
            ready: material_icons::READY.compact,
            attention: material_icons::ATTENTION.compact,
            command: material_icons::ENVIRONMENT.compact,
            arrow: material_icons::ARROW.compact,
            ok: material_icons::READY.compact,
        }
    }

    fn ascii() -> Self {
        Self {
            home: material_icons::HOME.ascii,
            intent: material_icons::INTENT.ascii,
            research: material_icons::RESEARCH.ascii,
            engineering_ir: material_icons::ENGINEERING_IR.ascii,
            workspace: material_icons::WORKSPACE.ascii,
            environment: material_icons::ENVIRONMENT.ascii,
            providers: material_icons::PROVIDERS.ascii,
            activity: material_icons::ACTIVITY.ascii,
            settings: material_icons::SETTINGS.ascii,
            branch: material_icons::BRANCH.ascii,
            shield: material_icons::SHIELD.ascii,
            ready: material_icons::READY.ascii,
            attention: material_icons::ATTENTION.ascii,
            command: material_icons::ENVIRONMENT.ascii,
            arrow: material_icons::ARROW.ascii,
            ok: material_icons::READY.ascii,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub text: Color,
    pub muted: Color,
    pub border: Color,
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
        let ascii = std::env::var_os("EVERYTHING_ASCII").is_some()
            || !material_icons::sources_integrity_ok();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let truecolor = std::env::var("COLORTERM")
            .map(|value| {
                let lower = value.to_ascii_lowercase();
                lower.contains("truecolor") || lower.contains("24bit")
            })
            .unwrap_or(false);

        if no_color {
            return Self {
                background: Color::Reset,
                panel: Color::Reset,
                panel_alt: Color::Reset,
                text: Color::Reset,
                muted: Color::DarkGray,
                border: Color::DarkGray,
                accent: Color::Reset,
                accent_alt: Color::Reset,
                success: Color::Reset,
                warning: Color::Reset,
                danger: Color::Reset,
                glyphs: if ascii {
                    Glyphs::ascii()
                } else {
                    Glyphs::material()
                },
            };
        }

        let (
            background,
            panel,
            panel_alt,
            text,
            muted,
            border,
            accent,
            accent_alt,
            success,
            warning,
            danger,
        ) = if truecolor {
            (
                Color::Rgb(7, 9, 14),
                Color::Rgb(12, 15, 23),
                Color::Rgb(17, 20, 31),
                Color::Rgb(236, 242, 250),
                Color::Rgb(124, 137, 158),
                Color::Rgb(44, 52, 69),
                Color::Rgb(74, 222, 255),
                Color::Rgb(153, 111, 255),
                Color::Rgb(80, 226, 168),
                Color::Rgb(247, 190, 80),
                Color::Rgb(255, 103, 132),
            )
        } else {
            (
                Color::Black,
                Color::Black,
                Color::Black,
                Color::White,
                Color::DarkGray,
                Color::DarkGray,
                Color::Cyan,
                Color::Magenta,
                Color::Green,
                Color::Yellow,
                Color::Red,
            )
        };

        Self {
            background,
            panel,
            panel_alt,
            text,
            muted,
            border,
            accent,
            accent_alt,
            success,
            warning,
            danger,
            glyphs: if ascii {
                Glyphs::ascii()
            } else {
                Glyphs::material()
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            background: Color::Black,
            panel: Color::Black,
            panel_alt: Color::Black,
            text: Color::White,
            muted: Color::DarkGray,
            border: Color::DarkGray,
            accent: Color::Cyan,
            accent_alt: Color::Magenta,
            success: Color::Green,
            warning: Color::Yellow,
            danger: Color::Red,
            glyphs: Glyphs::material(),
        }
    }
}
