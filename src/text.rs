#[derive(Copy, Clone, Default, Eq, PartialEq, Hash, Debug)]
pub enum SupportedTextSelection {
    #[default]
    None,
    Single,
    Multiple,
}

pub struct TextData {
    
}