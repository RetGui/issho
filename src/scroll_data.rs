#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollAmount {
    SmallIncrement,
    LargeIncrement,
    SmallDecrement,
    LargeDecrement,
    NoChange,
    GoToPercentage(f64)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollEvent {
    pub horizontal: ScrollAmount,
    pub vertical: ScrollAmount,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollContainerData {
    pub vertical_size: f64,
    pub horizontal_size: f64,
    pub horizontal_percentage: Option<f64>,
    pub vertical_percentage: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollData {
    ScrollContainer(ScrollContainerData),
    None,
}