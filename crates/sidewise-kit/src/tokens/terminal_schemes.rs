//! Imported terminal colour schemes, as OKLCH data. Follow Navo is not data: it reads the live
//! theme tokens. Catppuccin ships Latte for light mode and Mocha for dark mode.

use super::Mode;
use super::terminal::Oklch;

/// A terminal colour scheme the Settings picker offers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TerminalScheme {
    /// Follow the live Navo theme; resolved from the theme tokens.
    Navo,
    /// Atom One Dark.
    OneDark,
    /// Catppuccin, Latte in light mode and Mocha in dark mode.
    Catppuccin,
    /// Tokyo Night.
    TokyoNight,
    /// Gruvbox dark.
    Gruvbox,
    /// Dracula.
    Dracula,
}

impl TerminalScheme {
    /// Every scheme, in picker order.
    pub const ALL: [Self; 6] = [
        Self::Navo,
        Self::OneDark,
        Self::Catppuccin,
        Self::TokyoNight,
        Self::Gruvbox,
        Self::Dracula,
    ];

    /// Stable id used by settings and scheme files.
    pub fn id(self) -> &'static str {
        match self {
            Self::Navo => "navo",
            Self::OneDark => "one-dark",
            Self::Catppuccin => "catppuccin",
            Self::TokyoNight => "tokyo-night",
            Self::Gruvbox => "gruvbox",
            Self::Dracula => "dracula",
        }
    }

    /// Name shown on a scheme card.
    pub fn label(self) -> &'static str {
        match self {
            Self::Navo => "Follow Navo theme",
            Self::OneDark => "One Dark",
            Self::Catppuccin => "Catppuccin",
            Self::TokyoNight => "Tokyo Night",
            Self::Gruvbox => "Gruvbox",
            Self::Dracula => "Dracula",
        }
    }

    /// The fixed palette for an imported scheme; `None` for Follow Navo, which reads the theme.
    pub(super) fn palette(self, mode: Mode) -> Option<&'static SchemePalette> {
        match self {
            Self::Navo => None,
            Self::OneDark => Some(&ONE_DARK),
            Self::Catppuccin => Some(if mode.is_dark() {
                &CATPPUCCIN_MOCHA
            } else {
                &CATPPUCCIN_LATTE
            }),
            Self::TokyoNight => Some(&TOKYO_NIGHT),
            Self::Gruvbox => Some(&GRUVBOX),
            Self::Dracula => Some(&DRACULA),
        }
    }
}

/// A scheme's published colours: the tokens that are not derived from them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct SchemePalette {
    pub background: Oklch,
    pub text: Oklch,
    pub cursor: Oklch,
    pub selection: Oklch,
    pub ansi: [Oklch; 16],
}

const ONE_DARK: SchemePalette = SchemePalette {
    background: Oklch::new(0.2925, 0.0157, 264.30),
    text: Oklch::new(0.7621, 0.0202, 262.99),
    cursor: Oklch::new(0.6562, 0.1826, 262.74),
    selection: Oklch::new(0.3863, 0.0235, 265.69),
    ansi: [
        Oklch::new(0.2925, 0.0157, 264.30), // 0 #282c34
        Oklch::new(0.6709, 0.1448, 17.00),  // 1 #e06c75
        Oklch::new(0.7683, 0.1103, 133.00), // 2 #98c379
        Oklch::new(0.8249, 0.0970, 82.26),  // 3 #e5c07b
        Oklch::new(0.7304, 0.1213, 245.30), // 4 #61afef
        Oklch::new(0.6936, 0.1635, 318.21), // 5 #c678dd
        Oklch::new(0.7231, 0.0916, 206.31), // 6 #56b6c2
        Oklch::new(0.7621, 0.0202, 262.99), // 7 #abb2bf
        Oklch::new(0.4983, 0.0225, 262.91), // 8 #5c6370
        Oklch::new(0.6709, 0.1448, 17.00),  // 9 #e06c75
        Oklch::new(0.7683, 0.1103, 133.00), // 10 #98c379
        Oklch::new(0.8249, 0.0970, 82.26),  // 11 #e5c07b
        Oklch::new(0.7304, 0.1213, 245.30), // 12 #61afef
        Oklch::new(0.6936, 0.1635, 318.21), // 13 #c678dd
        Oklch::new(0.7231, 0.0916, 206.31), // 14 #56b6c2
        Oklch::new(1.0000, 0.0000, 89.88),  // 15 #ffffff
    ],
};

const CATPPUCCIN_LATTE: SchemePalette = SchemePalette {
    background: Oklch::new(0.9578, 0.0058, 264.53),
    text: Oklch::new(0.4355, 0.0430, 279.33),
    cursor: Oklch::new(0.7141, 0.1045, 33.10),
    selection: Oklch::new(0.8575, 0.0145, 268.48),
    ansi: [
        Oklch::new(0.4920, 0.0385, 279.30), // 0 #5c5f77
        Oklch::new(0.5505, 0.2155, 19.81),  // 1 #d20f39
        Oklch::new(0.6250, 0.1772, 140.44), // 2 #40a02b
        Oklch::new(0.7140, 0.1494, 67.78),  // 3 #df8e1d
        Oklch::new(0.5586, 0.2255, 262.09), // 4 #1e66f5
        Oklch::new(0.7256, 0.1739, 338.43), // 5 #ea76cb
        Oklch::new(0.6023, 0.0981, 201.10), // 6 #179299
        Oklch::new(0.7584, 0.0205, 273.15), // 7 #acb0be
        Oklch::new(0.5471, 0.0343, 279.08), // 8 #6c6f85
        Oklch::new(0.5505, 0.2155, 19.81),  // 9 #d20f39
        Oklch::new(0.6250, 0.1772, 140.44), // 10 #40a02b
        Oklch::new(0.7140, 0.1494, 67.78),  // 11 #df8e1d
        Oklch::new(0.5586, 0.2255, 262.09), // 12 #1e66f5
        Oklch::new(0.7256, 0.1739, 338.43), // 13 #ea76cb
        Oklch::new(0.6023, 0.0981, 201.10), // 14 #179299
        Oklch::new(0.8083, 0.0174, 271.20), // 15 #bcc0cc
    ],
};

const CATPPUCCIN_MOCHA: SchemePalette = SchemePalette {
    background: Oklch::new(0.2429, 0.0304, 283.91),
    text: Oklch::new(0.8787, 0.0426, 272.28),
    cursor: Oklch::new(0.9226, 0.0238, 30.49),
    selection: Oklch::new(0.4765, 0.0340, 278.64),
    ansi: [
        Oklch::new(0.4037, 0.0320, 280.15), // 0 #45475a
        Oklch::new(0.7556, 0.1297, 2.76),   // 1 #f38ba8
        Oklch::new(0.8577, 0.1092, 142.72), // 2 #a6e3a1
        Oklch::new(0.9193, 0.0704, 86.53),  // 3 #f9e2af
        Oklch::new(0.7664, 0.1113, 259.88), // 4 #89b4fa
        Oklch::new(0.8700, 0.0752, 336.30), // 5 #f5c2e7
        Oklch::new(0.8585, 0.0792, 182.75), // 6 #94e2d5
        Oklch::new(0.8168, 0.0403, 272.86), // 7 #bac2de
        Oklch::new(0.4765, 0.0340, 278.64), // 8 #585b70
        Oklch::new(0.7556, 0.1297, 2.76),   // 9 #f38ba8
        Oklch::new(0.8577, 0.1092, 142.72), // 10 #a6e3a1
        Oklch::new(0.9193, 0.0704, 86.53),  // 11 #f9e2af
        Oklch::new(0.7664, 0.1113, 259.88), // 12 #89b4fa
        Oklch::new(0.8700, 0.0752, 336.30), // 13 #f5c2e7
        Oklch::new(0.8585, 0.0792, 182.75), // 14 #94e2d5
        Oklch::new(0.7510, 0.0396, 273.93), // 15 #a6adc8
    ],
};

const TOKYO_NIGHT: SchemePalette = SchemePalette {
    background: Oklch::new(0.2263, 0.0214, 280.49),
    text: Oklch::new(0.8456, 0.0611, 274.76),
    cursor: Oklch::new(0.8456, 0.0611, 274.76),
    selection: Oklch::new(0.4064, 0.0939, 267.53),
    ansi: [
        Oklch::new(0.2029, 0.0160, 279.29), // 0 #15161e
        Oklch::new(0.7227, 0.1589, 10.28),  // 1 #f7768e
        Oklch::new(0.7953, 0.1395, 130.14), // 2 #9ece6a
        Oklch::new(0.7839, 0.1057, 75.43),  // 3 #e0af68
        Oklch::new(0.7190, 0.1322, 264.20), // 4 #7aa2f7
        Oklch::new(0.7515, 0.1344, 299.50), // 5 #bb9af7
        Oklch::new(0.8200, 0.1051, 235.72), // 6 #7dcfff
        Oklch::new(0.7666, 0.0537, 275.49), // 7 #a9b1d6
        Oklch::new(0.4094, 0.0546, 274.27), // 8 #414868
        Oklch::new(0.7227, 0.1589, 10.28),  // 9 #f7768e
        Oklch::new(0.7953, 0.1395, 130.14), // 10 #9ece6a
        Oklch::new(0.7839, 0.1057, 75.43),  // 11 #e0af68
        Oklch::new(0.7190, 0.1322, 264.20), // 12 #7aa2f7
        Oklch::new(0.7515, 0.1344, 299.50), // 13 #bb9af7
        Oklch::new(0.8200, 0.1051, 235.72), // 14 #7dcfff
        Oklch::new(0.8456, 0.0611, 274.76), // 15 #c0caf5
    ],
};

const GRUVBOX: SchemePalette = SchemePalette {
    background: Oklch::new(0.2768, 0.0000, 89.88),
    text: Oklch::new(0.8941, 0.0566, 89.24),
    cursor: Oklch::new(0.8941, 0.0566, 89.24),
    selection: Oklch::new(0.4110, 0.0115, 51.87),
    ansi: [
        Oklch::new(0.2768, 0.0000, 89.88),  // 0 #282828
        Oklch::new(0.5458, 0.2030, 28.66),  // 1 #cc241d
        Oklch::new(0.6564, 0.1354, 109.12), // 2 #98971a
        Oklch::new(0.7251, 0.1429, 77.71),  // 3 #d79921
        Oklch::new(0.5756, 0.0658, 199.49), // 4 #458588
        Oklch::new(0.5973, 0.1105, 352.22), // 5 #b16286
        Oklch::new(0.6450, 0.0939, 145.27), // 6 #689d6a
        Oklch::new(0.6903, 0.0346, 76.31),  // 7 #a89984
        Oklch::new(0.6192, 0.0286, 67.26),  // 8 #928374
        Oklch::new(0.6597, 0.2175, 30.39),  // 9 #fb4934
        Oklch::new(0.7652, 0.1581, 110.83), // 10 #b8bb26
        Oklch::new(0.8325, 0.1595, 82.99),  // 11 #fabd2f
        Oklch::new(0.6927, 0.0420, 169.77), // 12 #83a598
        Oklch::new(0.7054, 0.0976, 2.19),   // 13 #d3869b
        Oklch::new(0.7555, 0.1078, 137.68), // 14 #8ec07c
        Oklch::new(0.8941, 0.0566, 89.24),  // 15 #ebdbb2
    ],
};

const DRACULA: SchemePalette = SchemePalette {
    background: Oklch::new(0.2882, 0.0221, 277.51),
    text: Oklch::new(0.9775, 0.0079, 106.55),
    cursor: Oklch::new(0.9775, 0.0079, 106.55),
    selection: Oklch::new(0.4028, 0.0322, 277.83),
    ansi: [
        Oklch::new(0.2554, 0.0188, 280.49), // 0 #21222c
        Oklch::new(0.6822, 0.2063, 24.43),  // 1 #ff5555
        Oklch::new(0.8710, 0.2195, 148.02), // 2 #50fa7b
        Oklch::new(0.9553, 0.1342, 112.76), // 3 #f1fa8c
        Oklch::new(0.7420, 0.1485, 301.88), // 4 #bd93f9
        Oklch::new(0.7546, 0.1831, 346.81), // 5 #ff79c6
        Oklch::new(0.8826, 0.0934, 212.85), // 6 #8be9fd
        Oklch::new(0.9775, 0.0079, 106.55), // 7 #f8f8f2
        Oklch::new(0.5598, 0.0803, 270.09), // 8 #6272a4
        Oklch::new(0.7160, 0.1776, 22.64),  // 9 #ff6e6e
        Oklch::new(0.8939, 0.1948, 150.04), // 10 #69ff94
        Oklch::new(0.9802, 0.1114, 108.29), // 11 #ffffa5
        Oklch::new(0.8112, 0.1221, 307.03), // 12 #d6acff
        Oklch::new(0.7987, 0.1595, 339.03), // 13 #ff92df
        Oklch::new(0.9431, 0.0870, 195.78), // 14 #a4ffff
        Oklch::new(1.0000, 0.0000, 89.88),  // 15 #ffffff
    ],
};
