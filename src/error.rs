use crate::AccessKey;

/// Errors encountered by the accessibility tree.
pub enum IsshoError {
    /// The node no longer exists.
    MissingAccessNode(AccessKey),
}
