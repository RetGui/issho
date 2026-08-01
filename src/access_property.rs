/// A retained accessibility property that can notify a native platform when it changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AccessProperty {
    Name,
    Value,
    Checked,
}

/// A platform-neutral accessibility property value.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum AccessPropertyValue<'a> {
    Text(&'a str),
    Bool(bool),
}
