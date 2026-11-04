/// What a right-button press does in the file list.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RightButton {
    #[default]
    TotalCommander,
    Windows,
}
