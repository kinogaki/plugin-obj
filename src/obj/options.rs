//! How to read: the read's budget, and the sizes text is read and parsed
//! in.
use kinogaki_studio_api::format::ReadBudget;

/// The most corners over all elements: their offsets are 32-bit, with
/// `u32::MAX` free for [`crate::obj::NONE`].
pub(crate) const CORNERS: usize = (u32::MAX - 1) as usize;

/// How to read: the budget, and the sizes text is read and parsed in.
#[derive(Clone, Debug)]
pub struct ReadOptions {
    /// What the read may spend: the tables a block's counted records size
    /// are charged to it before they are made, a line longer than one of
    /// its allocations is refused, and its cancellation stops a read between
    /// blocks and their passes.
    pub budget: ReadBudget,
    /// Bytes read and parsed at a time when streaming from a file or reader.
    pub block_bytes: usize,
    /// The smallest chunk parsed as one job of the workers; smaller inputs
    /// parse as one.
    pub chunk_bytes: usize,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            budget: ReadBudget::default(),
            block_bytes: 32 << 20,
            chunk_bytes: 256 << 10,
        }
    }
}

impl ReadOptions {
    /// The longest line read: one allocation of the budget.
    pub(crate) fn line_bytes(&self) -> usize {
        usize::try_from(self.budget.limits().allocation).unwrap_or(usize::MAX)
    }
}
