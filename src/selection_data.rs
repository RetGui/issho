use crate::AccessKey;
use smallvec::SmallVec;

#[derive(Debug, Clone)]
pub enum SelectionData {
    SelectionGroup(SelectionGroup),
    SelectionGroupItem(SelectionGroupItem),
}

#[derive(Debug, Default, Clone)]
pub struct SelectionGroup {
    pub is_mandatory: bool,
    pub multiple_selectable: bool,
    pub selected_children: SmallVec<[AccessKey; 4]>,
}

#[derive(Debug, Clone)]
pub struct SelectionGroupItem {
    pub selection_group: AccessKey,
    pub is_selected: bool,
}
