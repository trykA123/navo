#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfLook {
    pub hit_rail: bool,
    pub page_map: bool,
}

impl Default for PdfLook {
    fn default() -> Self {
        Self {
            hit_rail: true,
            page_map: false,
        }
    }
}
