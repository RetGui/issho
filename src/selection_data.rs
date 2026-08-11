#[derive(Debug, Clone)]
pub enum SelectionData {
    SelectionGroup(SelectionGroup),
    SelectionGroupItem(SelectionGroupItem),
}

#[derive(Debug, Default, Clone)]
pub struct SelectionGroup {
    pub is_mandatory: bool,
    pub multiple_selectable: bool,
}

#[derive(Debug, Default, Clone)]
pub struct SelectionGroupItem {
    pub is_selected: bool,
}
