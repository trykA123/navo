#![forbid(unsafe_code)]

mod catalog;
mod global;

pub use catalog::{Catalog, CatalogError, FILES, LookupError, Platform};
pub use fluent_bundle::FluentArgs;
pub use global::{catalog, tr, tr_args};

#[macro_export]
macro_rules! t {
    ($id:expr $(,)?) => {
        $crate::tr($id)
    };
    ($id:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::FluentArgs::new();
        $(args.set(stringify!($name), $value);)+
        $crate::tr_args($id, &args)
    }};
}

#[cfg(test)]
mod tests;
