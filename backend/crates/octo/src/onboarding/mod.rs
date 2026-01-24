//! Onboarding module for progressive user setup.
//!
//! This module manages the onboarding state for users, including:
//! - Stage progression (language, provider, profile, personality, tutorial, complete)
//! - Component unlock tracking (sidebar, file tree, terminal, etc.)
//! - User level detection (beginner, intermediate, technical)
//! - Godmode for power users to skip onboarding

mod models;
mod service;

pub use models::*;
pub use service::OnboardingService;
