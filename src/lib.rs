// src/lib.rs

pub mod interface;
pub mod jobs;
pub mod server;
pub mod broker;
pub mod results;
pub mod tasks;

// Re-export key items for easier access
pub use server::{Server, ServerOpts, TaskOpts};
pub use jobs::{Job, JobOpts, JobCtx};
pub use tasks::tasks::{sum_processor, SumPayload};