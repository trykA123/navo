use crate::prelude::*;

gpui_kit::actions!(command_center, [Toggle]);

pub fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "command_center");
}
