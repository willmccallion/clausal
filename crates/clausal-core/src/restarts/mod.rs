//! Built-in restart strategies.

pub mod geometric;
pub mod glucose;
pub mod luby;
pub mod never;

pub use geometric::Geometric;
pub use glucose::Glucose;
pub use luby::Luby;
pub use never::Never;
