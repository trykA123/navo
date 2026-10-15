use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Location;

impl Serialize for Location {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Location {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(TextForm)
    }
}

struct TextForm;

impl Visitor<'_> for TextForm {
    type Value = Location;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a location in its text form")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Location, E> {
        Location::new(text).map_err(E::custom)
    }
}
