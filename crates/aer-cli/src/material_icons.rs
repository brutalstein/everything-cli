//! Terminal projections mechanically derived from vendored Google Material Symbols Rounded SVGs.
//!
//! The source SVGs under `assets/material-symbols/rounded/` are the authority.
//! Compact Braille masks are 8x4 alpha projections of those assets, allowing the
//! TUI to render recognizable icon geometry without requiring a user-installed
//! icon font. `EVERYTHING_ASCII=1` remains the explicit fallback.

#[derive(Clone, Copy, Debug)]
pub struct MaterialIcon {
    pub compact: &'static str,
    pub ascii: &'static str,
    asset: &'static [u8],
    sha256: &'static str,
}

impl MaterialIcon {
    pub const fn new(
        compact: &'static str,
        ascii: &'static str,
        asset: &'static [u8],
        sha256: &'static str,
    ) -> Self {
        Self {
            compact,
            ascii,
            asset,
            sha256,
        }
    }
}

macro_rules! icon {
    ($compact:literal, $ascii:literal, $file:literal, $sha256:literal) => {
        MaterialIcon::new(
            $compact,
            $ascii,
            include_bytes!(concat!("../assets/material-symbols/rounded/", $file)),
            $sha256,
        )
    };
}

pub const HOME: MaterialIcon = icon!(
    "⢰⣿⣿⡆",
    "[H]",
    "home.svg",
    "b29e5deb4467a06ec02ac358516fd8e5955c0d704fba8776f055b638cfbab607"
);
pub const INTENT: MaterialIcon = icon!(
    "⠸⠿⣧⠦",
    "[I]",
    "edit_note.svg",
    "bf94c39fb722b98f35f32bb40c6ddc657354e044413bf43b19bb57e8e5b535e7"
);
pub const RESEARCH: MaterialIcon = icon!(
    "⢾⣿⣿⣇",
    "[R]",
    "travel_explore.svg",
    "5bde86146ac7bf56a682f004b48ffc8fa5f8fab86b891de04704aa2f46c2d7a9"
);
pub const ENGINEERING_IR: MaterialIcon = icon!(
    "⢸⣿⠶⠆",
    "[IR]",
    "schema.svg",
    "fa0266ed86080dc106fbb7b9aba4e63100113c478ea067b66cc98917de8b5390"
);
pub const WORKSPACE: MaterialIcon = icon!(
    "⣿⣿⣿⡷",
    "[W]",
    "folder_open.svg",
    "0c71f5c57aacdde741f36551aea966125ade2ee4b81d15f2396b1d85e0418f00"
);
pub const ENVIRONMENT: MaterialIcon = icon!(
    "⣿⣿⣿⣿",
    "[T]",
    "terminal.svg",
    "ba924a0c39561794685040e624daa32ac86a130323eeec67093611518cf03560"
);
pub const PROVIDERS: MaterialIcon = icon!(
    "⢶⡾⢷⡶",
    "[P]",
    "hub.svg",
    "2943c260885f3777145671f4d5f449bdb5fc791996943276a23392d460e91175"
);
pub const ACTIVITY: MaterialIcon = icon!(
    "⢸⣿⣿⠆",
    "[A]",
    "history.svg",
    "98a0aacd0f8de1393b7fd627f3a4958986c33009192d59f022269505df6dbe51"
);
pub const SETTINGS: MaterialIcon = icon!(
    "⢾⣿⣿⡷",
    "[S]",
    "settings.svg",
    "6cd47de90647ece6b00922f497f8c4ea9c1649548cf15828d1df9d78d5bdf30d"
);
pub const BRANCH: MaterialIcon = icon!(
    "⠛⠻⣿⣿",
    "[G]",
    "account_tree.svg",
    "388a49190a16fd97a49cc2a3a8b7a052815fcd102d57abb45ea30384b6128083"
);
pub const READY: MaterialIcon = icon!(
    "⢾⣿⣿⡷",
    "[OK]",
    "check_circle.svg",
    "5c90d9aaa77eacf87ed8a5cedb6f9a1b7eba6ef41e6fbf7c9a1eb221ad59d2c9"
);
pub const ATTENTION: MaterialIcon = icon!(
    "⣠⣾⣷⣄",
    "[!]",
    "warning.svg",
    "b6907a9a2d0f1bd6b57191c958c221a2c35cf105e880d04f2a7455791b7a9ea8"
);
pub const SHIELD: MaterialIcon = icon!(
    "⠸⣏⣹⠇",
    "[#]",
    "shield.svg",
    "9ff9086efcb2f97c4ce83e4df676fff9704d1ca50c3af05757a97306411bfb2d"
);
pub const ARROW: MaterialIcon = icon!(
    "⠰⠶⡷⠆",
    "[>]",
    "arrow_forward.svg",
    "8c22701bd8e563e8f8bf6b89f0fc87fbe0a38c503bfcba196d2ba26d5644a7c5"
);

pub const ALL: [(&str, MaterialIcon); 14] = [
    ("home", HOME),
    ("intent", INTENT),
    ("research", RESEARCH),
    ("engineering_ir", ENGINEERING_IR),
    ("workspace", WORKSPACE),
    ("environment", ENVIRONMENT),
    ("providers", PROVIDERS),
    ("activity", ACTIVITY),
    ("settings", SETTINGS),
    ("branch", BRANCH),
    ("ready", READY),
    ("attention", ATTENTION),
    ("shield", SHIELD),
    ("arrow", ARROW),
];

#[must_use]
pub fn sources_integrity_ok() -> bool {
    use sha2::{Digest, Sha256};

    ALL.iter().all(|(_, icon)| {
        if !icon.asset.starts_with(b"<svg") {
            return false;
        }
        let digest = Sha256::digest(icon.asset);
        let actual = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        actual == icon.sha256
    })
}

#[cfg(test)]
mod tests {
    use super::{ALL, sources_integrity_ok};

    #[test]
    fn vendored_material_assets_match_recorded_sha256_and_are_svg() {
        assert!(sources_integrity_ok());
        for (name, icon) in ALL {
            assert!(!icon.compact.trim().is_empty(), "{name} compact projection");
            assert!(!icon.ascii.trim().is_empty(), "{name} ASCII fallback");
        }
    }
}
