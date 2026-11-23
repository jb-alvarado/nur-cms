pub mod bindings;

mod component;
mod engine;
pub(crate) mod host;
mod mail_limiter;

pub use component::{PluginComponent, Runtime};

#[cfg(test)]
mod tests;
