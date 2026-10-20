use super::*;
use dialoguer::{Input, MultiSelect, Select};

/// Ephemeral selection prompts, not an alternate-screen application. Standard
/// terminal styling/NO_COLOR and Escape cancellation come from dialoguer.
pub(super) async fn select(options: &mut PairOptions, home: &Path) -> Result<bool> {
    let mut candidates = options.workspaces.clone();
    if candidates.is_empty() {
        for name in ["byteowlz", "work"] {
            let p = home.join(name);
            if p.is_dir() {
                candidates.push(p);
            }
        }
    }
    loop {
        let mut labels: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
        labels.push("+ Add directory".into());
        let mut defaults = vec![true; candidates.len()];
        defaults.push(false);
        let Some(chosen) = MultiSelect::new()
            .with_prompt("Workspaces · Space toggles, Enter continues, Esc cancels")
            .items(&labels)
            .defaults(&defaults)
            .interact_opt()?
        else {
            return Ok(false);
        };
        if chosen.contains(&candidates.len()) {
            let input: String = Input::new()
                .with_prompt("Directory (absolute, relative or ~/path)")
                .interact_text()?;
            let selected = canonical_roots(&[PathBuf::from(input)], home)?;
            for p in selected {
                if !candidates.contains(&p) {
                    candidates.push(p);
                }
            }
            continue;
        }
        options.workspaces = chosen.into_iter().map(|i| candidates[i].clone()).collect();
        if !options.workspaces.is_empty() {
            break;
        }
        eprintln!("Select at least one Workspace, or press Esc to cancel.");
    }
    options.access.get_or_insert(Access::Full);
    let existing = grants(options, &options.workspaces);
    let labels = [
        if matches!(existing.sessions, Sessions::Observe) {
            "Observe Sessions"
        } else {
            "Session control"
        },
        if existing.read_roots.is_empty() {
            "Read/write Workspace files"
        } else {
            "Read-only Workspace files"
        },
        "Terminal as your OS user (NOT restricted to Workspaces)",
    ];
    let defaults = [
        !matches!(existing.sessions, Sessions::None),
        !existing.read_roots.is_empty() || !existing.write_roots.is_empty(),
        existing.full_user_terminal,
    ];
    // Full-access defaults grant nothing until explicit, default-No confirmation.
    let Some(chosen) = MultiSelect::new()
        .with_prompt("Access · Space toggles, Enter continues, Esc cancels")
        .items(&labels)
        .defaults(&defaults)
        .interact_opt()?
    else {
        return Ok(false);
    };
    options.sessions = Some(if chosen.contains(&0) {
        if matches!(existing.sessions, Sessions::Observe) {
            Sessions::Observe
        } else {
            Sessions::Control
        }
    } else {
        Sessions::None
    });
    options.files = Some(if chosen.contains(&1) {
        if !existing.read_roots.is_empty() || options.access == Some(Access::ReadOnly) {
            Files::Read
        } else {
            Files::Write
        }
    } else {
        Files::None
    });
    options.terminal = Some(chosen.contains(&2));
    let preferred = match options.network {
        Some(network) => network,
        None => {
            if super::runtime::address(Network::Tailscale).await.is_ok() {
                Network::Tailscale
            } else {
                eprintln!(
                    "Tailscale is unavailable; Local network is preselected. Start Tailscale to use your tailnet."
                );
                Network::Lan
            }
        }
    };
    let Some(network) = Select::new()
        .with_prompt("Connection")
        .items(&["Tailscale", "Local network"])
        .default(if preferred == Network::Lan { 1 } else { 0 })
        .interact_opt()?
    else {
        return Ok(false);
    };
    options.network = Some(if network == 0 {
        Network::Tailscale
    } else {
        Network::Lan
    });
    Ok(true)
}
