//! The commands the frontend can call.
//!
//! Every command is a plain function that takes the shared state and
//! returns a value that can be serialised to JSON. The commands are
//! grouped by the part of the launcher they belong to:
//!
//! | Module | What it does |
//! |--------|--------------|
//! | [`versions`] | The list of versions, the manifest and the state of an installation |
//! | [`accounts`] | The player accounts |
//! | [`instances`] | The game instances |
//! | [`launch`] | Starting and stopping the game |
//! | [`settings`] | The settings of the launcher |
//! | [`benchmarks`] | The results of the measurements |
//!
//! The commands do not contain business logic. They translate a call of
//! the frontend into a call of the other crates and translate the result
//! back. Everything that is worth testing lives below this layer.

pub mod accounts;
pub mod benchmarks;
pub mod instances;
pub mod launch;
pub mod settings;
pub mod versions;
