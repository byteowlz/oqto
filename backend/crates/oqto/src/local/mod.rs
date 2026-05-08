//! Local runtime module for running services directly on the host.
//!
//! Host OS integration now lives in the dedicated `oqto-host` crate. This
//! module keeps the previous `crate::local::*` paths stable while Oqto is
//! decomposed further.

pub use oqto_host::*;
pub mod linux_users {
    pub use oqto_host::linux_users::*;
}

mod user_mmry;
mod user_sldr;

pub use user_mmry::{UserMmryConfig, UserMmryManager};
pub use user_sldr::{UserSldrConfig, UserSldrManager};
