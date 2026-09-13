//! What a project has actually cost, against what it was supposed to.
//!
//! The document side of this product already works: a delivery note or an
//! invoice photographed on site is read, its lines are pulled out, and the
//! arithmetic on it is checked. But each document is read and then forgotten,
//! which makes it a reader rather than a cost system.
//!
//! This is the part that remembers. Documents become cost entries against a
//! project; the project carries a budget and the phases it draws money in; and
//! the two are compared continuously rather than at handover. The failure this
//! exists to prevent is the one contractors describe in the same words every
//! time: *we found out the job lost money when we closed it*.
//!
//! Two questions, and they are genuinely different:
//!
//! - **Is it over budget?** Cost against budget. Late, but unambiguous.
//! - **Is money leaving faster than work arrives?** Money drawn against work
//!   completed. This one is early, and it is the one that catches a contractor
//!   who has been paid for 60% of a job that is 40% built.
//!
//! Nothing here talks to a database or to OCR. It takes numbers and returns a
//! verdict, so the rules can be read and tested on their own — which matters,
//! because these are the numbers someone will make a payment decision on.

pub mod money;
pub mod project;

pub use money::{Money, ParseMoneyError};
pub use project::{Alert, BudgetLine, CostEntry, Phase, Project, ProjectStatus, Severity};
