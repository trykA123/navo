use gpui_kit::Modifiers;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct Combo {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub system: bool,
}

impl Combo {
    pub const CTRL: Self = Self::new(true, false, false);
    pub const SHIFT: Self = Self::new(false, false, true);
    pub const ALT: Self = Self::new(false, true, false);
    pub const CTRL_SHIFT: Self = Self::new(true, false, true);

    #[must_use]
    pub const fn new(control: bool, alt: bool, shift: bool) -> Self {
        Self {
            control,
            alt,
            shift,
            system: false,
        }
    }

    #[must_use]
    pub fn of(modifiers: &Modifiers) -> Option<Self> {
        let combo = Self {
            control: modifiers.control,
            alt: modifiers.alt,
            shift: modifiers.shift,
            system: modifiers.platform || modifiers.function,
        };
        (combo != Self::default()).then_some(combo)
    }

    #[must_use]
    pub fn matches(self, modifiers: &Modifiers) -> bool {
        Self::of(modifiers) == Some(self)
    }

    #[must_use]
    pub fn with_shift(self) -> Self {
        Self {
            shift: true,
            ..self
        }
    }

    #[must_use]
    pub fn name(self) -> String {
        let parts = [
            (self.control, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.system, "Win"),
        ];
        parts
            .into_iter()
            .filter(|(is_held, _)| *is_held)
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
            .join("+")
    }
}
