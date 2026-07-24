use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct CatalogRoots {
    pub work_directory: PathBuf,
    pub account_home: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillLocation {
    ProjectAgents,
    ProjectPi,
    AccountAgents,
    AccountPi,
}

impl SkillLocation {
    fn scope(self) -> SkillScope {
        match self {
            Self::ProjectAgents | Self::ProjectPi => SkillScope::Repo,
            Self::AccountAgents | Self::AccountPi => SkillScope::Account,
        }
    }

    fn precedence(self) -> u8 {
        match self {
            Self::ProjectAgents => 0,
            Self::ProjectPi => 1,
            Self::AccountAgents => 2,
            Self::AccountPi => 3,
        }
    }

    fn discovers_root_markdown(self) -> bool {
        matches!(self, Self::ProjectPi | Self::AccountPi)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillScope {
    Repo,
    Account,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillMutability {
    Editable,
    Immutable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillPolicy {
    pub mutability: SkillMutability,
    pub required: bool,
}

impl Default for SkillPolicy {
    fn default() -> Self {
        Self {
            mutability: SkillMutability::Editable,
            required: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillDiagnostic {
    MissingFrontmatter,
    InvalidFrontmatter(String),
    MissingName,
    MissingDescription,
    UnsafePath,
    ReadFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectiveState {
    Effective,
    Overridden { by: PathBuf },
    Invalid,
}

#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: Option<String>,
    pub scope: SkillScope,
    pub location: SkillLocation,
    pub path: PathBuf,
    pub policy: SkillPolicy,
    pub state: EffectiveState,
    pub diagnostics: Vec<SkillDiagnostic>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillCatalog {
    pub skills: Vec<DiscoveredSkill>,
}

impl SkillCatalog {
    pub fn discover(
        roots: &CatalogRoots,
        policies: &HashMap<(SkillLocation, String), SkillPolicy>,
    ) -> Self {
        let locations = [
            (
                SkillLocation::ProjectAgents,
                roots.work_directory.join(".agents/skills"),
            ),
            (
                SkillLocation::ProjectPi,
                roots.work_directory.join(".pi/skills"),
            ),
            (
                SkillLocation::AccountAgents,
                roots.account_home.join(".agents/skills"),
            ),
            (
                SkillLocation::AccountPi,
                roots.account_home.join(".pi/agent/skills"),
            ),
        ];

        let mut skills = Vec::new();
        for (location, root) in locations {
            discover_location(&root, location, policies, &mut skills);
        }

        skills.sort_by(|left, right| {
            left.location
                .precedence()
                .cmp(&right.location.precedence())
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.path.cmp(&right.path))
        });

        let mut winners = BTreeMap::<String, PathBuf>::new();
        for skill in &mut skills {
            if !skill.diagnostics.is_empty() {
                skill.state = EffectiveState::Invalid;
                continue;
            }
            if let Some(winner) = winners.get(&skill.name) {
                skill.state = EffectiveState::Overridden { by: winner.clone() };
            } else {
                winners.insert(skill.name.clone(), skill.path.clone());
                skill.state = EffectiveState::Effective;
            }
        }

        Self { skills }
    }
}

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

fn discover_location(
    root: &Path,
    location: SkillLocation,
    policies: &HashMap<(SkillLocation, String), SkillPolicy>,
    output: &mut Vec<DiscoveredSkill>,
) {
    if !root.is_dir() {
        return;
    }

    let mut candidates = Vec::new();
    collect_skill_files(root, root, location, &mut candidates);
    candidates.sort();
    candidates.dedup();

    for skill_file in candidates {
        let relative = skill_file.strip_prefix(root).unwrap_or(&skill_file);
        if !is_safe_relative_path(relative) {
            output.push(invalid_path_entry(skill_file, location));
            continue;
        }
        output.push(parse_skill(skill_file, location, policies));
    }
}

fn collect_skill_files(
    root: &Path,
    current: &Path,
    location: SkillLocation,
    output: &mut Vec<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    let mut entries = entries.flatten().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            // `npx skills` and Oqto activation use links from the standard
            // discovery directory to a canonical skill directory. Follow only
            // a direct skill link; never recurse through a linked tree.
            let linked_skill_file = path.join("SKILL.md");
            if linked_skill_file.is_file() {
                output.push(linked_skill_file);
            } else if current == root
                && location.discovers_root_markdown()
                && path.extension().is_some_and(|extension| extension == "md")
                && path.is_file()
            {
                output.push(path);
            }
            continue;
        }
        if file_type.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.is_file() {
                output.push(skill_file);
            } else {
                collect_skill_files(root, &path, location, output);
            }
        } else if file_type.is_file()
            && current == root
            && location.discovers_root_markdown()
            && path.extension().is_some_and(|extension| extension == "md")
        {
            output.push(path);
        }
    }
}

fn parse_skill(
    path: PathBuf,
    location: SkillLocation,
    policies: &HashMap<(SkillLocation, String), SkillPolicy>,
) -> DiscoveredSkill {
    let fallback_name = if path.file_name().is_some_and(|name| name == "SKILL.md") {
        path.parent().and_then(Path::file_name)
    } else {
        path.file_stem()
    }
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| "unknown".to_string());

    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            return DiscoveredSkill {
                name: fallback_name,
                description: None,
                scope: location.scope(),
                location,
                path,
                policy: SkillPolicy::default(),
                state: EffectiveState::Invalid,
                diagnostics: vec![SkillDiagnostic::ReadFailed(error.to_string())],
            };
        }
    };

    let mut diagnostics = Vec::new();
    let frontmatter = match extract_frontmatter(&contents) {
        Ok(frontmatter) => match serde_yaml::from_str::<Frontmatter>(frontmatter) {
            Ok(frontmatter) => Some(frontmatter),
            Err(error) => {
                diagnostics.push(SkillDiagnostic::InvalidFrontmatter(error.to_string()));
                None
            }
        },
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            None
        }
    };

    let name = frontmatter
        .as_ref()
        .and_then(|frontmatter| frontmatter.name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            diagnostics.push(SkillDiagnostic::MissingName);
            fallback_name
        });
    let description = frontmatter
        .and_then(|frontmatter| frontmatter.description)
        .map(|description| description.trim().to_string())
        .filter(|description| !description.is_empty());
    if description.is_none() {
        diagnostics.push(SkillDiagnostic::MissingDescription);
    }

    let policy = policies
        .get(&(location, name.clone()))
        .copied()
        .unwrap_or_default();

    DiscoveredSkill {
        name,
        description,
        scope: location.scope(),
        location,
        path,
        policy,
        state: EffectiveState::Invalid,
        diagnostics,
    }
}

fn extract_frontmatter(contents: &str) -> Result<&str, SkillDiagnostic> {
    let normalized = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Err(SkillDiagnostic::MissingFrontmatter);
    };
    let Some(end) = rest.find("\n---") else {
        return Err(SkillDiagnostic::MissingFrontmatter);
    };
    Ok(&rest[..end])
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn invalid_path_entry(path: PathBuf, location: SkillLocation) -> DiscoveredSkill {
    DiscoveredSkill {
        name: "unsafe-path".to_string(),
        description: None,
        scope: location.scope(),
        location,
        path,
        policy: SkillPolicy::default(),
        state: EffectiveState::Invalid,
        diagnostics: vec![SkillDiagnostic::UnsafePath],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_skill(path: &Path, name: &str, description: Option<&str>) {
        fs::create_dir_all(path.parent().expect("skill parent")).expect("create skill dir");
        let description = description
            .map(|description| format!("description: {description}\n"))
            .unwrap_or_default();
        fs::write(
            path,
            format!("---\nname: {name}\n{description}---\n\n# {name}\n"),
        )
        .expect("write skill");
    }

    fn roots(temp: &TempDir) -> CatalogRoots {
        CatalogRoots {
            work_directory: temp.path().join("workdir"),
            account_home: temp.path().join("home"),
        }
    }

    #[test]
    fn discovers_all_standard_locations_and_pi_root_markdown_only() {
        let temp = TempDir::new().expect("tempdir");
        let roots = roots(&temp);
        write_skill(
            &roots
                .work_directory
                .join(".agents/skills/repo-agent/SKILL.md"),
            "repo-agent",
            Some("repo agent"),
        );
        write_skill(
            &roots.work_directory.join(".pi/skills/repo-pi/SKILL.md"),
            "repo-pi",
            Some("repo pi"),
        );
        write_skill(
            &roots.work_directory.join(".pi/skills/root-skill.md"),
            "root-skill",
            Some("root pi"),
        );
        write_skill(
            &roots.work_directory.join(".agents/skills/ignored.md"),
            "ignored",
            Some("ignored root markdown"),
        );
        write_skill(
            &roots
                .account_home
                .join(".agents/skills/account-agent/SKILL.md"),
            "account-agent",
            Some("account agent"),
        );
        write_skill(
            &roots
                .account_home
                .join(".pi/agent/skills/account-pi/SKILL.md"),
            "account-pi",
            Some("account pi"),
        );

        let catalog = SkillCatalog::discover(&roots, &HashMap::new());
        let names = catalog
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "repo-agent",
                "repo-pi",
                "root-skill",
                "account-agent",
                "account-pi"
            ]
        );
    }

    #[test]
    fn repo_agents_skill_wins_and_shadowed_entries_remain_visible() {
        let temp = TempDir::new().expect("tempdir");
        let roots = roots(&temp);
        write_skill(
            &roots.work_directory.join(".agents/skills/shared/SKILL.md"),
            "shared",
            Some("local"),
        );
        write_skill(
            &roots.account_home.join(".agents/skills/shared/SKILL.md"),
            "shared",
            Some("global"),
        );

        let catalog = SkillCatalog::discover(&roots, &HashMap::new());
        assert_eq!(catalog.skills.len(), 2);
        assert_eq!(catalog.skills[0].state, EffectiveState::Effective);
        assert_eq!(
            catalog.skills[1].state,
            EffectiveState::Overridden {
                by: catalog.skills[0].path.clone()
            }
        );
    }

    #[test]
    fn missing_description_is_invalid_and_does_not_shadow_valid_skill() {
        let temp = TempDir::new().expect("tempdir");
        let roots = roots(&temp);
        write_skill(
            &roots.work_directory.join(".agents/skills/broken/SKILL.md"),
            "same",
            None,
        );
        write_skill(
            &roots.account_home.join(".agents/skills/valid/SKILL.md"),
            "same",
            Some("valid"),
        );

        let catalog = SkillCatalog::discover(&roots, &HashMap::new());
        assert!(matches!(catalog.skills[0].state, EffectiveState::Invalid));
        assert!(
            catalog.skills[0]
                .diagnostics
                .contains(&SkillDiagnostic::MissingDescription)
        );
        assert_eq!(catalog.skills[1].state, EffectiveState::Effective);
    }

    #[test]
    fn applies_explicit_immutable_policy() {
        let temp = TempDir::new().expect("tempdir");
        let roots = roots(&temp);
        write_skill(
            &roots.account_home.join(".agents/skills/system/SKILL.md"),
            "system",
            Some("managed"),
        );
        let policy = SkillPolicy {
            mutability: SkillMutability::Immutable,
            required: true,
        };
        let policies =
            HashMap::from([((SkillLocation::AccountAgents, "system".to_string()), policy)]);

        let catalog = SkillCatalog::discover(&roots, &policies);
        assert_eq!(catalog.skills[0].policy, policy);
    }

    #[cfg(unix)]
    #[test]
    fn discovers_a_linked_canonical_skill_without_recursing_linked_trees() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("tempdir");
        let roots = roots(&temp);
        let canonical = temp.path().join("workspace-library/frontend");
        write_skill(
            &canonical.join("SKILL.md"),
            "frontend",
            Some("linked skill"),
        );
        let links = roots.work_directory.join(".agents/skills");
        fs::create_dir_all(&links).expect("create links dir");
        symlink(&canonical, links.join("frontend")).expect("link canonical skill");

        let catalog = SkillCatalog::discover(&roots, &HashMap::new());
        assert_eq!(catalog.skills.len(), 1);
        assert_eq!(catalog.skills[0].name, "frontend");
        assert_eq!(catalog.skills[0].state, EffectiveState::Effective);
    }

    #[test]
    fn rejects_parent_and_absolute_paths() {
        assert!(!is_safe_relative_path(Path::new("../skill/SKILL.md")));
        assert!(!is_safe_relative_path(Path::new("/skill/SKILL.md")));
        assert!(is_safe_relative_path(Path::new("skill/SKILL.md")));
    }

    #[test]
    fn ordering_is_deterministic() {
        let temp = TempDir::new().expect("tempdir");
        let roots = roots(&temp);
        for name in ["zeta", "alpha", "middle"] {
            write_skill(
                &roots
                    .work_directory
                    .join(format!(".agents/skills/{name}/SKILL.md")),
                name,
                Some(name),
            );
        }

        let catalog = SkillCatalog::discover(&roots, &HashMap::new());
        let names = catalog
            .skills
            .iter()
            .map(|skill| &skill.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["alpha", "middle", "zeta"]);
    }
}
