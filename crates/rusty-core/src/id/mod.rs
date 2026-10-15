macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            #[must_use]
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }

            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

id_type!(WindowId);
id_type!(PaneId);
id_type!(TabId);

#[cfg(test)]
mod tests;
