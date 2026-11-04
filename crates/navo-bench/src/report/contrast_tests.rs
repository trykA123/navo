use super::style::CSS;

/// Text pairs the page uses; every one needs 4.5:1 in both themes.
const TEXT_PAIRS: [(&str, &str); 11] = [
    ("text", "bg"),
    ("text", "surface"),
    ("text", "raised"),
    ("muted", "bg"),
    ("muted", "surface"),
    ("muted", "raised"),
    ("pass", "pass-bg"),
    ("fail", "fail-bg"),
    ("pass", "surface"),
    ("fail", "surface"),
    ("text", "surface"),
];

/// Bars and focus rings are not text: 3:1 is enough.
const PART_PAIRS: [(&str, &str); 1] = [("bar", "raised")];

fn token(block: &str, name: &str) -> (f64, f64, f64) {
    let start = block.find(&format!("--{name}:oklch(")).unwrap() + name.len() + 9;
    let inner = block[start..].split(')').next().unwrap();
    let parts: Vec<f64> = inner.split(' ').map(|p| p.parse().unwrap()).collect();
    (parts[0], parts[1], parts[2])
}

fn luminance((l, c, h): (f64, f64, f64)) -> f64 {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let cube = |x: f64| x * x * x;
    let (lm, mm, sm) = (
        cube(l + 0.3963377774 * a + 0.2158037573 * b),
        cube(l - 0.1055613458 * a - 0.0638541728 * b),
        cube(l - 0.0894841775 * a - 1.2914855480 * b),
    );
    let r = 4.0767416621 * lm - 3.3077115913 * mm + 0.2309699292 * sm;
    let g = -1.2684380046 * lm + 2.6097574011 * mm - 0.3413193965 * sm;
    let bl = -0.0041960863 * lm - 0.7034186147 * mm + 1.7076147010 * sm;
    let clamp = |x: f64| x.clamp(0.0, 1.0);
    0.2126 * clamp(r) + 0.7152 * clamp(g) + 0.0722 * clamp(bl)
}

fn ratio(block: &str, (fg, bg): (&str, &str)) -> f64 {
    let (a, b) = (luminance(token(block, fg)), luminance(token(block, bg)));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn blocks() -> [&'static str; 2] {
    let (light, dark) = CSS
        .split_once("@media (prefers-color-scheme:dark)")
        .unwrap();
    [light, dark]
}

#[test]
fn every_text_pair_has_4_5_to_1_in_both_themes() {
    for block in blocks() {
        for pair in TEXT_PAIRS {
            assert!(ratio(block, pair) >= 4.5, "{pair:?} {}", ratio(block, pair));
        }
        for pair in PART_PAIRS {
            assert!(ratio(block, pair) >= 3.0, "{pair:?}");
        }
    }
}
