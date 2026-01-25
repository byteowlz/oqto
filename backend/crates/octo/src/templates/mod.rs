//! Onboarding templates module.
//!
//! Manages templates for user onboarding (ONBOARD.md, PERSONALITY.md, USER.md, AGENTS.md).
//! Templates can come from:
//! 1. Remote git repo (default: byteowlz/octo-templates)
//! 2. Local filesystem path
//! 3. Embedded fallback (compiled into binary)

mod config;
mod service;

pub use config::{OnboardingTemplatesConfig, TemplatePreset};
pub use service::OnboardingTemplatesService;
