# Material Symbols Rounded — vendored terminal assets

This directory contains unmodified SVG source assets from Google's official
`google/material-design-icons` repository, under `symbols/web/<name>/materialsymbolsrounded/`.

The `everything` TUI does **not** require a Material Symbols or Nerd Font install.
`src/material_icons.rs` contains compact Unicode Braille masks mechanically derived
from these SVGs for terminal rendering. The SVGs remain the source assets and are
checked by SHA-256 in the test suite.

## Provenance

| Local file | Upstream symbol | Upstream Git blob | Local SHA-256 |
|---|---|---|---|
| `home.svg` | `home` | `57e3733fa57f46866f01d848b7e5d575dd462e44` | `b29e5deb4467a06ec02ac358516fd8e5955c0d704fba8776f055b638cfbab607` |
| `edit_note.svg` | `edit_note` | `17fee4ef15b588545d3cc97685c62acb672700f9` | `bf94c39fb722b98f35f32bb40c6ddc657354e044413bf43b19bb57e8e5b535e7` |
| `travel_explore.svg` | `travel_explore` | `30ceb4f1ad2c3eddf295b772998d189519204226` | `5bde86146ac7bf56a682f004b48ffc8fa5f8fab86b891de04704aa2f46c2d7a9` |
| `schema.svg` | `schema` | `81afd7859fadffb0e4cee5547c0b5c0aa448bc04` | `fa0266ed86080dc106fbb7b9aba4e63100113c478ea067b66cc98917de8b5390` |
| `folder_open.svg` | `folder_open` | `583343f617c01477ade289eee2506b2df6454a00` | `0c71f5c57aacdde741f36551aea966125ade2ee4b81d15f2396b1d85e0418f00` |
| `terminal.svg` | `terminal` | `bbd2b7d38f8f9fe4e9b2cba496892d0f8156f407` | `ba924a0c39561794685040e624daa32ac86a130323eeec67093611518cf03560` |
| `hub.svg` | `hub` | `9d3d0d9ee20f6ea371b6f3833f96228016fa6799` | `2943c260885f3777145671f4d5f449bdb5fc791996943276a23392d460e91175` |
| `history.svg` | `history` | `f3c6f839fa7f5daed4687375b69dc475f761add3` | `98a0aacd0f8de1393b7fd627f3a4958986c33009192d59f022269505df6dbe51` |
| `settings.svg` | `settings` | `269c81cff13aafd64655652b4debe49b3d37ef60` | `6cd47de90647ece6b00922f497f8c4ea9c1649548cf15828d1df9d78d5bdf30d` |
| `account_tree.svg` | `account_tree` | `3955a342ab88adc2967d21570545ba0eab233c5f` | `388a49190a16fd97a49cc2a3a8b7a052815fcd102d57abb45ea30384b6128083` |
| `check_circle.svg` | `check_circle` | `fa86f2ef9f9ec0658e9fef410885eb24a9da839a` | `5c90d9aaa77eacf87ed8a5cedb6f9a1b7eba6ef41e6fbf7c9a1eb221ad59d2c9` |
| `warning.svg` | `warning` | `21d5e0b3c01f657bc51661934807116d863afd7a` | `b6907a9a2d0f1bd6b57191c958c221a2c35cf105e880d04f2a7455791b7a9ea8` |
| `shield.svg` | `shield` | `b9493bc26848ede530f67cfafc4da7547c0fb64d` | `9ff9086efcb2f97c4ce83e4df676fff9704d1ca50c3af05757a97306411bfb2d` |
| `arrow_forward.svg` | `arrow_forward` | `9c7e26e02b239bef29593e73a89f3805e3398c63` | `8c22701bd8e563e8f8bf6b89f0fc87fbe0a38c503bfcba196d2ba26d5644a7c5` |

## Terminal projection

The compact masks are generated from an 8x4 alpha raster of the 24px rounded SVG,
then encoded as Unicode Braille cells. This is a display transformation only; it
carries no runtime or domain authority. `EVERYTHING_ASCII=1` selects explicit ASCII
fallback labels when a terminal cannot display Braille reliably.

## License

The upstream Material Symbols assets are distributed under Apache License 2.0.
A copy of the upstream license is stored as `LICENSE.apache-2.0.txt` in this directory.
