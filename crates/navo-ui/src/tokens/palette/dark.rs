use super::{ColorTheme, Tokens, lch};

const OPEN_SEA: Tokens = Tokens {
    bg: lch(19., 0.014, 225.),
    surface: lch(22.5, 0.014, 225.),
    raised: lch(26.5, 0.014, 225.),
    border: lch(33., 0.014, 225.),
    selected: lch(33., 0.058, 218.),
    text: lch(93.8, 0.014, 225.),
    muted: lch(72., 0.017, 225.),
    border_strong: lch(54.8, 0.017, 225.),
    accent: lch(76., 0.11, 218.),
    accent_text: lch(76., 0.11, 218.),
    danger: lch(73., 0.166, 22.),
    success: lch(85., 0.13, 160.),
    warning: lch(82., 0.13, 80.),
    info: lch(76., 0.12, 262.),
};

const RED_BILL: Tokens = Tokens {
    bg: lch(19., 0.012, 250.),
    surface: lch(22.5, 0.012, 250.),
    raised: lch(26.5, 0.012, 250.),
    border: lch(33., 0.012, 250.),
    selected: lch(33., 0.064, 250.),
    text: lch(93.4, 0.012, 250.),
    muted: lch(72., 0.014, 250.),
    border_strong: lch(54.8, 0.014, 250.),
    accent: lch(76., 0.12, 250.),
    accent_text: lch(76., 0.12, 250.),
    danger: lch(72.6, 0.17, 18.),
    success: lch(85., 0.13, 150.),
    warning: lch(82., 0.13, 95.),
    info: lch(76., 0.12, 235.),
};

const BLACK_CAP: Tokens = Tokens {
    bg: lch(19., 0.01, 245.),
    surface: lch(22.5, 0.01, 245.),
    raised: lch(26.5, 0.01, 245.),
    border: lch(33., 0.01, 245.),
    selected: lch(33., 0.068, 48.),
    text: lch(93., 0.01, 245.),
    muted: lch(72., 0.011, 245.),
    border_strong: lch(54.8, 0.011, 245.),
    accent: lch(76., 0.14, 48.),
    accent_text: lch(76., 0.14, 48.),
    danger: lch(71.8, 0.17, 15.),
    success: lch(85., 0.13, 160.),
    warning: lch(82., 0.13, 95.),
    info: lch(76., 0.12, 245.),
};

const FINE_MESH: Tokens = Tokens {
    bg: lch(19., 0.006, 85.),
    surface: lch(22.5, 0.006, 85.),
    raised: lch(26.5, 0.006, 85.),
    border: lch(33., 0.006, 85.),
    selected: lch(33., 0.056, 195.),
    text: lch(94.2, 0.006, 85.),
    muted: lch(72., 0.007, 85.),
    border_strong: lch(54.8, 0.007, 85.),
    accent: lch(76., 0.1, 195.),
    accent_text: lch(76., 0.1, 195.),
    danger: lch(73., 0.166, 22.),
    success: lch(85., 0.13, 150.),
    warning: lch(82., 0.13, 85.),
    info: lch(76., 0.12, 245.),
};

const GRAIN: Tokens = Tokens {
    bg: lch(19., 0.01, 80.),
    surface: lch(22.5, 0.01, 80.),
    raised: lch(26.5, 0.01, 80.),
    border: lch(33., 0.01, 80.),
    selected: lch(33., 0.062, 72.),
    text: lch(93., 0.01, 80.),
    muted: lch(72., 0.011, 80.),
    border_strong: lch(54.8, 0.011, 80.),
    accent: lch(76., 0.11, 72.),
    accent_text: lch(76., 0.11, 72.),
    danger: lch(72.2, 0.17, 25.),
    success: lch(85., 0.13, 150.),
    warning: lch(82., 0.13, 95.),
    info: lch(76., 0.12, 235.),
};

const INK_AND_AMBER: Tokens = Tokens {
    bg: lch(19., 0.012, 255.),
    surface: lch(22.5, 0.012, 255.),
    raised: lch(26.5, 0.012, 255.),
    border: lch(33., 0.012, 255.),
    selected: lch(33., 0.062, 255.),
    text: lch(93.4, 0.012, 255.),
    muted: lch(72., 0.014, 255.),
    border_strong: lch(54.8, 0.014, 255.),
    accent: lch(76., 0.11, 255.),
    accent_text: lch(76., 0.11, 255.),
    danger: lch(72.6, 0.168, 20.),
    success: lch(85., 0.13, 158.),
    warning: lch(82., 0.13, 90.),
    info: lch(76., 0.12, 225.),
};

pub(super) fn tokens(theme: ColorTheme) -> Tokens {
    match theme {
        ColorTheme::OpenSea => OPEN_SEA,
        ColorTheme::RedBill => RED_BILL,
        ColorTheme::BlackCap => BLACK_CAP,
        ColorTheme::FineMesh => FINE_MESH,
        ColorTheme::Grain => GRAIN,
        ColorTheme::InkAndAmber => INK_AND_AMBER,
    }
}
