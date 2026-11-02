use super::{ColorTheme, Tokens, lch};

const OPEN_SEA: Tokens = Tokens {
    bg: lch(95.5, 0.012, 225.),
    surface: lch(98.5, 0.008, 225.),
    raised: lch(99.8, 0., 225.),
    border: lch(89., 0.014, 225.),
    selected: lch(90., 0.046, 218.),
    text: lch(22., 0.014, 225.),
    muted: lch(48.8, 0.017, 225.),
    border_strong: lch(64.8, 0.017, 225.),
    accent: lch(52., 0.092, 218.),
    accent_text: lch(48.8, 0.086, 218.),
    danger: lch(36., 0.144, 22.),
    success: lch(48., 0.108, 160.),
    warning: lch(49.6, 0.102, 80.),
    info: lch(49.6, 0.12, 262.),
};

const RED_BILL: Tokens = Tokens {
    bg: lch(95.5, 0.01, 250.),
    surface: lch(98.5, 0.007, 250.),
    raised: lch(99.8, 0., 250.),
    border: lch(89., 0.012, 250.),
    selected: lch(90., 0.048, 250.),
    text: lch(22., 0.012, 250.),
    muted: lch(48.4, 0.014, 250.),
    border_strong: lch(64.8, 0.014, 250.),
    accent: lch(52., 0.12, 250.),
    accent_text: lch(49.2, 0.12, 250.),
    danger: lch(36., 0.144, 18.),
    success: lch(47.6, 0.13, 150.),
    warning: lch(49.2, 0.1, 95.),
    info: lch(48.8, 0.102, 235.),
};

const BLACK_CAP: Tokens = Tokens {
    bg: lch(95.5, 0.008, 245.),
    surface: lch(98.5, 0.006, 245.),
    raised: lch(99.8, 0., 245.),
    border: lch(89., 0.01, 245.),
    selected: lch(90., 0.051, 48.),
    text: lch(22., 0.01, 245.),
    muted: lch(48., 0.011, 245.),
    border_strong: lch(64.8, 0.011, 245.),
    accent: lch(52., 0.14, 48.),
    accent_text: lch(50., 0.136, 48.),
    danger: lch(36., 0.144, 15.),
    success: lch(47.6, 0.106, 160.),
    warning: lch(48.8, 0.1, 95.),
    info: lch(48.4, 0.12, 245.),
};

const FINE_MESH: Tokens = Tokens {
    bg: lch(95.5, 0.005, 85.),
    surface: lch(98.5, 0.003, 85.),
    raised: lch(99.8, 0.002, 85.),
    border: lch(89., 0.006, 85.),
    selected: lch(90., 0.045, 195.),
    text: lch(22., 0.006, 85.),
    muted: lch(48.8, 0.007, 85.),
    border_strong: lch(64.8, 0.007, 85.),
    accent: lch(52., 0.088, 195.),
    accent_text: lch(48.4, 0.082, 195.),
    danger: lch(36., 0.144, 22.),
    success: lch(48., 0.13, 150.),
    warning: lch(49.6, 0.1, 85.),
    info: lch(49.2, 0.12, 245.),
};

const GRAIN: Tokens = Tokens {
    bg: lch(95.5, 0.004, 80.),
    surface: lch(98.5, 0.003, 80.),
    raised: lch(99.8, 0., 80.),
    border: lch(89., 0.005, 80.),
    selected: lch(90., 0.046, 72.),
    text: lch(22., 0.008, 80.),
    muted: lch(48.4, 0.009, 80.),
    border_strong: lch(64.8, 0.009, 80.),
    accent: lch(52., 0.11, 72.),
    accent_text: lch(49.6, 0.106, 72.),
    danger: lch(36., 0.146, 25.),
    success: lch(47.6, 0.13, 150.),
    warning: lch(48.8, 0.1, 95.),
    info: lch(48.4, 0.102, 235.),
};

const INK_AND_AMBER: Tokens = Tokens {
    bg: lch(95.5, 0.01, 255.),
    surface: lch(98.5, 0.007, 255.),
    raised: lch(99.8, 0., 255.),
    border: lch(89., 0.012, 255.),
    selected: lch(90., 0.046, 255.),
    text: lch(22., 0.012, 255.),
    muted: lch(48.4, 0.014, 255.),
    border_strong: lch(64.8, 0.014, 255.),
    accent: lch(52., 0.11, 255.),
    accent_text: lch(49.2, 0.11, 255.),
    danger: lch(36., 0.144, 20.),
    success: lch(47.6, 0.11, 158.),
    warning: lch(49.2, 0.1, 90.),
    info: lch(48.4, 0.09, 225.),
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
