//! Terminal capability negotiation and the semantic rendering vocabulary.
//!
//! `docs/23_CLI_AND_USER_EXPERIENCE.md` requires the interface to degrade in
//! layers and to keep a specific terminal library out of the domain
//! architecture. This module is that abstraction: it resolves what the attached
//! terminal can actually do, then turns semantic intent — a status, a field, a
//! trust boundary — into bytes appropriate for it.
//!
//! Two rules shape everything here:
//!
//! - **Color and glyphs are supplementary.** Every state also carries a text
//!   label, so meaning survives `NO_COLOR`, a pipe, a screen reader, and a
//!   terminal without Unicode.
//! - **Rendering is pure.** Every function takes an explicit [`Surface`] and
//!   returns a `String`, so the whole visual language is testable without a
//!   TTY. Nothing here reads the environment after detection.
//!
//! The full-screen layer of the degradation ladder is deliberately absent: this
//! product ships the line-oriented mode, and no full-screen terminal dependency
//! is permitted in the shipped binary.

use std::{
    env,
    fmt::Write as _,
    io::{self, IsTerminal},
};

/// Width assumed when the terminal does not report how wide it is.
///
/// Nothing reports width portably without a new dependency or `unsafe`, so the
/// shell's exported `COLUMNS` is used when present and this conservative value
/// otherwise. Every layout here is designed to be correct at this width.
const ASSUMED_WIDTH: usize = 80;

/// Narrowest width the layout still targets. Below this, content becomes a
/// single column with no alignment padding rather than broken panels.
const MIN_WIDTH: usize = 40;

/// Widest layout used regardless of terminal size. Long measure hurts scanning,
/// and a status view is not improved by a 300-column rule.
const MAX_WIDTH: usize = 100;

/// Explicit user control over color, required by the accessibility contract.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorChoice {
    /// Color only when writing to an interactive terminal that wants it.
    #[default]
    Auto,
    /// Color even when the stream is redirected.
    Always,
    /// Never emit escape sequences.
    Never,
}

impl ColorChoice {
    /// Parses the `--color` argument value.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending value when it is not one of the
    /// three modes.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => Err(format!(
                "unknown color mode `{other}`; expected auto, always or never"
            )),
        }
    }
}

/// How wide the attached terminal is, in behavioral terms.
///
/// These breakpoints decide behavior, not decoration: whether a two-column
/// field survives, or has to become a plain list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layout {
    /// One column, no alignment padding, no borders.
    Narrow,
    /// Aligned fields and full-width rules.
    Standard,
    /// Aligned fields with room for a trailing detail column.
    Wide,
}

/// What the attached terminal can do, resolved once at startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Surface {
    interactive: bool,
    color: bool,
    unicode: bool,
    width: usize,
}

impl Surface {
    /// Negotiates capabilities against the real environment.
    ///
    /// Detection happens once and never again: re-probing per render would make
    /// output depend on when in the process it was produced.
    #[must_use]
    pub fn detect(choice: ColorChoice) -> Self {
        let interactive = io::stdout().is_terminal() && io::stdin().is_terminal();
        Self {
            interactive,
            color: resolve_color(choice, interactive),
            unicode: resolve_unicode(),
            width: resolve_width(),
        }
    }

    /// A surface with every enhancement off: what a pipe, a CI job and a screen
    /// reader get. It is the baseline the renderer tests assert against, so it
    /// is constructed explicitly rather than detected.
    #[cfg(test)]
    pub(crate) const fn plain() -> Self {
        Self {
            interactive: false,
            color: false,
            unicode: false,
            width: ASSUMED_WIDTH,
        }
    }

    /// A surface with every enhancement on, at an explicit width.
    #[cfg(test)]
    pub(crate) const fn rich(width: usize) -> Self {
        Self {
            interactive: true,
            color: true,
            unicode: true,
            width,
        }
    }

    /// Whether a human is waiting at a terminal.
    #[must_use]
    pub const fn interactive(&self) -> bool {
        self.interactive
    }

    /// The responsive breakpoint this width falls into.
    #[must_use]
    pub const fn layout(&self) -> Layout {
        if self.width < 60 {
            Layout::Narrow
        } else if self.width < 100 {
            Layout::Standard
        } else {
            Layout::Wide
        }
    }

    /// Applies a semantic role to text.
    ///
    /// Without color the text is returned untouched, which is why no caller may
    /// rely on the role alone to carry meaning.
    #[must_use]
    pub fn paint(&self, role: Role, text: &str) -> String {
        if !self.color || text.is_empty() {
            return text.to_owned();
        }
        format!("\u{1b}[{}m{text}\u{1b}[0m", role.sgr())
    }

    /// A section heading: a label, then a rule filling the remaining width.
    #[must_use]
    pub fn heading(&self, label: &str) -> String {
        if matches!(self.layout(), Layout::Narrow) {
            return self.paint(Role::Accent, label);
        }
        let fill = self.width.saturating_sub(label.chars().count() + 1);
        format!(
            "{} {}",
            self.paint(Role::Accent, label),
            self.paint(Role::Muted, &self.line_glyph().repeat(fill))
        )
    }

    /// A `key   value` row with the key column aligned to `key_width`.
    ///
    /// On a narrow terminal the alignment padding is dropped rather than pushing
    /// the value past the right edge: essential content must never require
    /// horizontal scrolling.
    #[must_use]
    pub fn field(&self, key: &str, value: &str, key_width: usize) -> String {
        if matches!(self.layout(), Layout::Narrow) {
            return format!("{} {value}", self.paint(Role::Muted, key));
        }
        let pad = key_width.saturating_sub(key.chars().count());
        format!(
            "{}{} {value}",
            self.paint(Role::Muted, key),
            " ".repeat(pad)
        )
    }

    /// A semantic status line: glyph, subject, optional detail, text label.
    ///
    /// The label is always present, so the glyph and the color stay additive
    /// rather than load-bearing.
    #[must_use]
    pub fn status(&self, status: Status, subject: &str, detail: Option<&str>) -> String {
        let mut line = String::with_capacity(subject.len() + 48);
        let _ = write!(
            line,
            "{} {subject}",
            self.paint(status.role(), status.glyph(self.unicode))
        );
        if let Some(detail) = detail {
            let _ = write!(line, "  {}", self.paint(Role::Muted, detail));
        }
        let _ = write!(line, "  {}", self.paint(status.role(), status.label()));
        line
    }

    /// A bordered panel, used for trust boundaries such as permission requests.
    ///
    /// Such a request must be visually distinct from ordinary output. On a
    /// narrow or ASCII-only terminal the border is dropped in favor of a
    /// labelled block, because a broken box is worse than no box.
    #[must_use]
    pub fn panel(&self, title: &str, lines: &[String]) -> String {
        let inner = self.width.saturating_sub(4);
        if matches!(self.layout(), Layout::Narrow) || !self.unicode {
            return self.plain_block(title, lines, inner);
        }

        let mut block = String::new();
        let head = format!("\u{250c} {title} ");
        let fill = self.width.saturating_sub(head.chars().count() + 1);
        let _ = writeln!(
            block,
            "{}",
            self.paint(
                Role::Warning,
                &format!("{head}{}\u{2510}", "\u{2500}".repeat(fill))
            )
        );
        let edge = self.paint(Role::Warning, "\u{2502}");
        for line in lines {
            for wrapped in wrap(line, inner) {
                let pad = inner.saturating_sub(display_width(&wrapped));
                let _ = writeln!(block, "{edge} {wrapped}{} {edge}", " ".repeat(pad));
            }
        }
        let _ = writeln!(
            block,
            "{}",
            self.paint(
                Role::Warning,
                &format!(
                    "\u{2514}{}\u{2518}",
                    "\u{2500}".repeat(self.width.saturating_sub(2))
                )
            )
        );
        block
    }

    /// The interactive input marker.
    #[must_use]
    pub fn prompt(&self) -> String {
        let glyph = if self.unicode { "\u{276f}" } else { ">" };
        format!("{} ", self.paint(Role::Accent, glyph))
    }

    fn plain_block(&self, title: &str, lines: &[String], inner: usize) -> String {
        let mut block = String::new();
        let _ = writeln!(
            block,
            "{}",
            self.paint(Role::Warning, &format!("[{title}]"))
        );
        for line in lines {
            for wrapped in wrap(line, inner) {
                let _ = writeln!(block, "  {wrapped}");
            }
        }
        block
    }

    fn line_glyph(&self) -> String {
        if self.unicode { "\u{2500}" } else { "-" }.to_owned()
    }
}

/// Semantic color roles. Provider and model identity are deliberately absent:
/// identity is not a status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// Ordinary content.
    Neutral,
    /// Secondary content: labels, units, timings.
    Muted,
    /// The element the eye should land on first.
    Accent,
    /// A verified, accepted outcome.
    Success,
    /// Something that needs a human decision.
    Warning,
    /// A failed outcome.
    Failure,
    /// Refused by policy rather than broken.
    Blocked,
}

impl Role {
    /// SGR parameters for this role.
    ///
    /// Only the eight-color range plus dim and bold is used: truecolor cannot be
    /// assumed, and these stay legible on both dark and light themes.
    const fn sgr(self) -> &'static str {
        match self {
            Self::Neutral => "0",
            Self::Muted => "2",
            Self::Accent => "1;36",
            Self::Success => "32",
            Self::Warning => "33",
            Self::Failure => "31",
            Self::Blocked => "35",
        }
    }
}

/// The semantic status vocabulary of the user-experience contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// Completed and verified.
    Accepted,
    /// Available but not started.
    Ready,
    /// Waiting on a human decision.
    Attention,
    /// Completed and rejected.
    Failed,
    /// Waiting on something external.
    Waiting,
    /// Refused by policy.
    Blocked,
}

/// Every status in the vocabulary, for exhaustive rendering checks.
#[cfg(test)]
pub(crate) const ALL_STATUSES: [Status; 6] = [
    Status::Accepted,
    Status::Ready,
    Status::Attention,
    Status::Failed,
    Status::Waiting,
    Status::Blocked,
];

impl Status {
    /// The glyph for terminals that can render it, with an ASCII fallback.
    #[must_use]
    pub const fn glyph(self, unicode: bool) -> &'static str {
        match (self, unicode) {
            (Self::Accepted, true) => "\u{2713}",
            (Self::Accepted, false) => "+",
            (Self::Ready, true) => "\u{25cb}",
            (Self::Ready, false) => "-",
            (Self::Attention, _) => "!",
            (Self::Failed, true) => "\u{00d7}",
            (Self::Failed, false) => "x",
            (Self::Waiting, true) => "\u{2026}",
            (Self::Waiting, false) => ".",
            (Self::Blocked, _) => "#",
        }
    }

    /// The text label. Always rendered, never optional.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Ready => "ready",
            Self::Attention => "needs attention",
            Self::Failed => "failed",
            Self::Waiting => "waiting",
            Self::Blocked => "blocked",
        }
    }

    /// The color role this status is painted with.
    #[must_use]
    pub const fn role(self) -> Role {
        match self {
            Self::Accepted => Role::Success,
            Self::Ready | Self::Waiting => Role::Muted,
            Self::Attention => Role::Warning,
            Self::Failed => Role::Failure,
            Self::Blocked => Role::Blocked,
        }
    }
}

/// Wraps text at word boundaries, splitting a word only when it cannot fit on a
/// line of its own.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_owned()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let word_width = word.chars().count();
        if current.is_empty() {
            let mut rest = word;
            while rest.chars().count() > width {
                let head: String = rest.chars().take(width).collect();
                rest = &rest[head.len()..];
                lines.push(head);
            }
            current.push_str(rest);
        } else if current.chars().count() + 1 + word_width > width {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// The visible width of already-rendered text, ignoring escape sequences.
#[must_use]
pub fn display_width(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            for escape in chars.by_ref() {
                if escape == 'm' {
                    break;
                }
            }
            continue;
        }
        width += 1;
    }
    width
}

fn resolve_color(choice: ColorChoice, interactive: bool) -> bool {
    match choice {
        ColorChoice::Never => false,
        ColorChoice::Always => true,
        // `NO_COLOR` is honored whatever its value: its presence is the signal.
        ColorChoice::Auto => {
            interactive
                && env::var_os("NO_COLOR").is_none()
                && env::var("TERM").as_deref() != Ok("dumb")
        }
    }
}

fn resolve_unicode() -> bool {
    if env::var_os("EVERYTHING_ASCII").is_some_and(|value| value != "0") {
        return false;
    }
    // Modern terminal emulators advertise themselves, and the POSIX locale
    // variables carry the encoding elsewhere. A terminal that says nothing gets
    // ASCII, because a replacement box is worse than a plain character.
    if env::var_os("WT_SESSION").is_some() || env::var_os("TERM_PROGRAM").is_some() {
        return true;
    }
    ["LC_ALL", "LC_CTYPE", "LANG"].iter().any(|name| {
        env::var(name).is_ok_and(|value| {
            let value = value.to_ascii_uppercase();
            value.contains("UTF-8") || value.contains("UTF8")
        })
    })
}

fn resolve_width() -> usize {
    env::var("COLUMNS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|width| *width >= MIN_WIDTH)
        .unwrap_or(ASSUMED_WIDTH)
        .min(MAX_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_modes_override_detection_in_both_directions() {
        assert!(!resolve_color(ColorChoice::Never, true));
        assert!(resolve_color(ColorChoice::Always, false));
    }

    #[test]
    fn a_redirected_stream_never_gets_escape_sequences() {
        assert!(!resolve_color(ColorChoice::Auto, false));
    }

    #[test]
    fn an_unknown_color_mode_is_rejected_and_names_the_offending_value() {
        let error = ColorChoice::parse("rainbow").expect_err("invalid mode must be rejected");
        assert!(error.contains("rainbow"), "{error}");
        assert_eq!(ColorChoice::parse("never"), Ok(ColorChoice::Never));
    }

    #[test]
    fn a_plain_surface_emits_no_escape_sequences_anywhere() {
        let surface = Surface::plain();
        let rendered = format!(
            "{}{}{}{}{}",
            surface.heading("state"),
            surface.field("branch", "main", 12),
            surface.status(Status::Failed, "verification", Some("3 checks")),
            surface.panel("Permission required", &["Run: cargo publish".to_owned()]),
            surface.prompt()
        );
        assert!(
            !rendered.contains('\u{1b}'),
            "plain output must stay copyable: {rendered:?}"
        );
    }

    #[test]
    fn every_status_carries_a_text_label_so_color_is_never_the_only_signal() {
        let surface = Surface::plain();
        for status in ALL_STATUSES {
            let line = surface.status(status, "subject", None);
            assert!(
                line.contains(status.label()),
                "{status:?} lost its label: {line}"
            );
        }
    }

    #[test]
    fn every_status_has_an_ascii_glyph_for_terminals_without_unicode() {
        for status in ALL_STATUSES {
            let ascii = status.glyph(false);
            assert!(
                ascii.is_ascii() && !ascii.is_empty(),
                "{status:?} has no ASCII fallback"
            );
        }
    }

    #[test]
    fn distinct_statuses_stay_distinguishable_without_color() {
        let mut seen = std::collections::BTreeSet::new();
        for status in ALL_STATUSES {
            assert!(
                seen.insert(status.label()),
                "{status:?} reuses another label"
            );
        }
    }

    #[test]
    fn rules_and_panels_respect_the_negotiated_width() {
        let surface = Surface::rich(64);
        assert_eq!(display_width(&surface.heading("state")), 64);
        let panel = surface.panel("Permission required", &["Run: npm publish".to_owned()]);
        for line in panel.lines() {
            assert!(
                display_width(line) <= 64,
                "panel line overflows the terminal: {line:?}"
            );
        }
    }

    #[test]
    fn a_narrow_terminal_drops_borders_instead_of_breaking_them() {
        let surface = Surface::rich(44);
        assert_eq!(surface.layout(), Layout::Narrow);
        let panel = surface.panel("Permission required", &["Run: cargo publish".to_owned()]);
        assert!(
            !panel.contains('\u{250c}'),
            "narrow mode must not draw boxes: {panel}"
        );
        assert!(panel.contains("Permission required"));
        assert!(panel.contains("cargo publish"));
    }

    #[test]
    fn an_ascii_terminal_never_receives_box_drawing_characters() {
        let surface = Surface {
            unicode: false,
            ..sized(80)
        };
        let rendered = format!(
            "{}{}{}",
            surface.heading("state"),
            surface.prompt(),
            surface.panel("Permission required", &["Run: rm -rf build".to_owned()])
        );
        assert!(rendered.is_ascii(), "non-ASCII leaked: {rendered:?}");
    }

    /// A colorless surface of a given width, for assertions about layout shape
    /// rather than about painting.
    const fn sized(width: usize) -> Surface {
        Surface {
            interactive: true,
            color: false,
            unicode: true,
            width,
        }
    }

    #[test]
    fn a_narrow_terminal_drops_alignment_padding_rather_than_the_value() {
        assert_eq!(sized(44).field("branch", "main", 30), "branch main");
        assert_eq!(sized(80).field("branch", "main", 12), "branch       main");
    }

    #[test]
    fn wrapping_breaks_on_words_and_never_loses_content() {
        let lines = wrap("alpha beta gamma delta", 11);
        assert_eq!(lines, vec!["alpha beta", "gamma delta"]);
        assert_eq!(lines.join(" "), "alpha beta gamma delta");
    }

    #[test]
    fn wrapping_splits_a_word_that_cannot_fit_at_all() {
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn display_width_ignores_escape_sequences() {
        let painted = Surface::rich(80).paint(Role::Success, "ok");
        assert!(painted.contains('\u{1b}'));
        assert_eq!(display_width(&painted), 2);
    }

    #[test]
    fn width_negotiation_stays_inside_the_designed_bounds() {
        let width = resolve_width();
        assert!((MIN_WIDTH..=MAX_WIDTH).contains(&width), "{width}");
    }

    #[test]
    fn layout_breakpoints_are_behavioral_and_ordered() {
        assert_eq!(Surface::rich(40).layout(), Layout::Narrow);
        assert_eq!(Surface::rich(80).layout(), Layout::Standard);
        assert_eq!(Surface::rich(120).layout(), Layout::Wide);
    }
}
