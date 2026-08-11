use smolvec::SmolVec;
use crate::AccessKey;

#[derive(Debug, Default, Clone)]
pub struct SelectionData {
    pub is_mandatory: bool,
    pub multiple_selectable: bool,
    pub selected_children: SmolVec<AccessKey>,
}