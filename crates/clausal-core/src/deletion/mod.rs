//! Built-in learned-clause deletion policies.

pub mod activity_based;
pub mod lbd_based;

pub use activity_based::ActivityBased;
pub use lbd_based::LbdBased;
