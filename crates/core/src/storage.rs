//! What the core keeps on disk: the files and where they live, how it reads their TOML, the
//! config, the ad-hoc rules and the saved state.

pub(crate) mod ad_hoc_rules;
pub(crate) mod config;
pub(crate) mod document;
pub(crate) mod file;
pub(crate) mod saved_state;
