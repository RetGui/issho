use crate::AccessKey;
use smallvec::SmallVec;

#[derive(Debug, Default, Clone)]
pub struct SelectionData {
    pub is_mandatory: bool,
    pub multiple_selectable: bool,
    pub selected_children: SmallVec<[AccessKey; 4]>,
}
