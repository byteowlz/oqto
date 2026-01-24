//! Onboarding data models.

use serde::{Deserialize, Serialize};

/// Onboarding stage enumeration.
///
/// Users progress through these stages during onboarding:
/// 1. Language - Select UI language
/// 2. Provider - Configure AI provider (API keys)
/// 3. Profile - Fill out USER.md preferences
/// 4. Personality - Customize agent personality (PERSONALITY.md)
/// 5. Tutorial - Guided tour of the UI
/// 6. Complete - Onboarding finished
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStage {
    #[default]
    Language,
    Provider,
    Profile,
    Personality,
    Tutorial,
    Complete,
}

impl std::fmt::Display for OnboardingStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OnboardingStage::Language => write!(f, "language"),
            OnboardingStage::Provider => write!(f, "provider"),
            OnboardingStage::Profile => write!(f, "profile"),
            OnboardingStage::Personality => write!(f, "personality"),
            OnboardingStage::Tutorial => write!(f, "tutorial"),
            OnboardingStage::Complete => write!(f, "complete"),
        }
    }
}

impl std::str::FromStr for OnboardingStage {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "language" => Ok(OnboardingStage::Language),
            "provider" => Ok(OnboardingStage::Provider),
            "profile" => Ok(OnboardingStage::Profile),
            "personality" => Ok(OnboardingStage::Personality),
            "tutorial" => Ok(OnboardingStage::Tutorial),
            "complete" => Ok(OnboardingStage::Complete),
            _ => Err(format!("Invalid onboarding stage: {}", s)),
        }
    }
}

/// User experience level.
///
/// Determines which UI components are available and how much
/// guidance is provided during onboarding and daily use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UserLevel {
    #[default]
    Beginner,
    Intermediate,
    Technical,
}

impl std::fmt::Display for UserLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserLevel::Beginner => write!(f, "beginner"),
            UserLevel::Intermediate => write!(f, "intermediate"),
            UserLevel::Technical => write!(f, "technical"),
        }
    }
}

impl std::str::FromStr for UserLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "beginner" => Ok(UserLevel::Beginner),
            "intermediate" => Ok(UserLevel::Intermediate),
            "technical" => Ok(UserLevel::Technical),
            _ => Err(format!("Invalid user level: {}", s)),
        }
    }
}

/// Unlockable UI components.
///
/// These components are progressively unlocked during onboarding
/// or as the user demonstrates familiarity with the platform.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UnlockedComponents {
    /// Main sidebar navigation
    #[serde(default)]
    pub sidebar: bool,
    /// Session list in sidebar
    #[serde(default)]
    pub session_list: bool,
    /// File tree panel
    #[serde(default)]
    pub file_tree: bool,
    /// Todo/TRX issue list
    #[serde(default)]
    pub todo_list: bool,
    /// Canvas for images and artifacts
    #[serde(default)]
    pub canvas: bool,
    /// Memory/notes panel
    #[serde(default)]
    pub memory: bool,
    /// TRX issue tracking view
    #[serde(default)]
    pub trx: bool,
    /// Terminal access
    #[serde(default)]
    pub terminal: bool,
    /// Model picker dropdown
    #[serde(default)]
    pub model_picker: bool,
    /// Project/workspace management
    #[serde(default)]
    pub projects: bool,
    /// Voice mode
    #[serde(default)]
    pub voice: bool,
    /// Settings panel
    #[serde(default)]
    pub settings: bool,
}

impl UnlockedComponents {
    /// Create with all components unlocked (for godmode).
    pub fn all_unlocked() -> Self {
        Self {
            sidebar: true,
            session_list: true,
            file_tree: true,
            todo_list: true,
            canvas: true,
            memory: true,
            trx: true,
            terminal: true,
            model_picker: true,
            projects: true,
            voice: true,
            settings: true,
        }
    }

    /// Create with minimal components unlocked (for new users).
    pub fn minimal() -> Self {
        Self::default()
    }

    /// Unlock a specific component by name.
    pub fn unlock(&mut self, component: &str) -> bool {
        match component {
            "sidebar" => {
                self.sidebar = true;
                true
            }
            "session_list" | "session-list" => {
                self.session_list = true;
                true
            }
            "file_tree" | "file-tree" => {
                self.file_tree = true;
                true
            }
            "todo_list" | "todo-list" => {
                self.todo_list = true;
                true
            }
            "canvas" => {
                self.canvas = true;
                true
            }
            "memory" => {
                self.memory = true;
                true
            }
            "trx" => {
                self.trx = true;
                true
            }
            "terminal" => {
                self.terminal = true;
                true
            }
            "model_picker" | "model-picker" => {
                self.model_picker = true;
                true
            }
            "projects" => {
                self.projects = true;
                true
            }
            "voice" => {
                self.voice = true;
                true
            }
            "settings" => {
                self.settings = true;
                true
            }
            _ => false,
        }
    }

    /// Check if a component is unlocked.
    pub fn is_unlocked(&self, component: &str) -> bool {
        match component {
            "sidebar" => self.sidebar,
            "session_list" | "session-list" => self.session_list,
            "file_tree" | "file-tree" => self.file_tree,
            "todo_list" | "todo-list" => self.todo_list,
            "canvas" => self.canvas,
            "memory" => self.memory,
            "trx" => self.trx,
            "terminal" => self.terminal,
            "model_picker" | "model-picker" => self.model_picker,
            "projects" => self.projects,
            "voice" => self.voice,
            "settings" => self.settings,
            _ => false,
        }
    }
}

/// Full onboarding state for a user.
///
/// This is stored as JSON in the user's settings field.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OnboardingState {
    /// Whether onboarding is completed.
    #[serde(default)]
    pub completed: bool,

    /// Current onboarding stage.
    #[serde(default)]
    pub stage: OnboardingStage,

    /// Primary language code (e.g., "en", "de").
    #[serde(default)]
    pub language: Option<String>,

    /// Additional languages for polyglot users.
    #[serde(default)]
    pub languages: Vec<String>,

    /// Which UI components are unlocked.
    #[serde(default)]
    pub unlocked: UnlockedComponents,

    /// User experience level.
    #[serde(default)]
    pub user_level: UserLevel,

    /// Whether godmode was activated (skipped onboarding).
    #[serde(default)]
    pub godmode: bool,

    /// Timestamp when onboarding started.
    #[serde(default)]
    pub started_at: Option<String>,

    /// Timestamp when onboarding completed.
    #[serde(default)]
    pub completed_at: Option<String>,

    /// Tutorial step index (for resuming interrupted tutorials).
    #[serde(default)]
    pub tutorial_step: u32,
}

impl OnboardingState {
    /// Create a fresh onboarding state for a new user.
    pub fn new() -> Self {
        Self {
            started_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        }
    }

    /// Create a completed state (for users who activate godmode).
    pub fn godmode() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            completed: true,
            stage: OnboardingStage::Complete,
            unlocked: UnlockedComponents::all_unlocked(),
            user_level: UserLevel::Technical,
            godmode: true,
            started_at: Some(now.clone()),
            completed_at: Some(now),
            ..Default::default()
        }
    }

    /// Check if user needs onboarding.
    pub fn needs_onboarding(&self) -> bool {
        !self.completed && !self.godmode
    }

    /// Advance to the next stage.
    pub fn advance_stage(&mut self) {
        self.stage = match self.stage {
            OnboardingStage::Language => OnboardingStage::Provider,
            OnboardingStage::Provider => OnboardingStage::Profile,
            OnboardingStage::Profile => OnboardingStage::Personality,
            OnboardingStage::Personality => OnboardingStage::Tutorial,
            OnboardingStage::Tutorial => {
                self.complete();
                OnboardingStage::Complete
            }
            OnboardingStage::Complete => OnboardingStage::Complete,
        };
    }

    /// Mark onboarding as complete.
    pub fn complete(&mut self) {
        self.completed = true;
        self.stage = OnboardingStage::Complete;
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
        // Unlock all basic components on completion
        self.unlocked.sidebar = true;
        self.unlocked.session_list = true;
        self.unlocked.file_tree = true;
        self.unlocked.settings = true;
    }
}

/// Request to update onboarding state.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateOnboardingRequest {
    /// New stage to set.
    #[serde(default)]
    pub stage: Option<OnboardingStage>,

    /// Language to set.
    #[serde(default)]
    pub language: Option<String>,

    /// Languages to add.
    #[serde(default)]
    pub languages: Option<Vec<String>>,

    /// User level to set.
    #[serde(default)]
    pub user_level: Option<UserLevel>,

    /// Tutorial step to set.
    #[serde(default)]
    pub tutorial_step: Option<u32>,

    /// Whether to mark as complete.
    #[serde(default)]
    pub complete: Option<bool>,
}

/// Request to unlock a component.
#[derive(Debug, Clone, Deserialize)]
pub struct UnlockComponentRequest {
    /// Component name to unlock.
    pub component: String,
}

/// Response for onboarding state.
#[derive(Debug, Clone, Serialize)]
pub struct OnboardingResponse {
    /// Full onboarding state.
    #[serde(flatten)]
    pub state: OnboardingState,

    /// Whether the user needs to go through onboarding.
    pub needs_onboarding: bool,
}

impl From<OnboardingState> for OnboardingResponse {
    fn from(state: OnboardingState) -> Self {
        let needs_onboarding = state.needs_onboarding();
        Self {
            state,
            needs_onboarding,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_progression() {
        let mut state = OnboardingState::new();
        assert_eq!(state.stage, OnboardingStage::Language);
        assert!(state.needs_onboarding());

        state.advance_stage();
        assert_eq!(state.stage, OnboardingStage::Provider);

        state.advance_stage();
        assert_eq!(state.stage, OnboardingStage::Profile);

        state.advance_stage();
        assert_eq!(state.stage, OnboardingStage::Personality);

        state.advance_stage();
        assert_eq!(state.stage, OnboardingStage::Tutorial);

        state.advance_stage();
        assert_eq!(state.stage, OnboardingStage::Complete);
        assert!(!state.needs_onboarding());
    }

    #[test]
    fn test_godmode() {
        let state = OnboardingState::godmode();
        assert!(state.completed);
        assert!(state.godmode);
        assert!(!state.needs_onboarding());
        assert!(state.unlocked.terminal);
        assert!(state.unlocked.model_picker);
        assert_eq!(state.user_level, UserLevel::Technical);
    }

    #[test]
    fn test_component_unlock() {
        let mut unlocked = UnlockedComponents::minimal();
        assert!(!unlocked.is_unlocked("terminal"));

        assert!(unlocked.unlock("terminal"));
        assert!(unlocked.is_unlocked("terminal"));

        // Test kebab-case alias
        assert!(unlocked.unlock("file-tree"));
        assert!(unlocked.is_unlocked("file_tree"));

        // Unknown component
        assert!(!unlocked.unlock("unknown"));
    }

    #[test]
    fn test_serialization() {
        let state = OnboardingState::new();
        let json = serde_json::to_string(&state).unwrap();
        let parsed: OnboardingState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.stage, OnboardingStage::Language);
    }
}
