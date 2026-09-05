//! Shared types for the Anthovai AI Platform.
//!
//! This crate is the root of the dependency graph: it may not depend on any
//! other crate in the workspace. See `docs/spec-v0.1/06-rust-workspace-architecture.md`.

pub mod config;
pub mod error;
pub mod ids;
pub mod plan;
pub mod tenant;
pub mod time;

pub use error::{DomainError, Result};
pub use ids::*;
pub use plan::{Feature, Plan, PlanLimits};
pub use tenant::{Actor, AgentScope, Permission, Role, Scope, TenantCtx};
pub use time::Clock;
