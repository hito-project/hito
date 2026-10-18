//! Placement tools: the commands that add elements to the model, written
//! as [`Tool`](crate::command::Tool)s so each element is one transaction
//! and one undo step (#19).

pub mod level;
