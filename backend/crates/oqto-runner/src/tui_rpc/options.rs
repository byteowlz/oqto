//! Filtered Session configuration. Never expose raw provider headers/credentials.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Details {
    pub model: Option<Model>,
    pub thinking_level: Option<String>,
    pub has_pending_messages: Option<bool>,
    pub pending_message_count: Option<u32>,
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Model {
    provider: String,
    id: String,
    #[serde(default)]
    name: String,
}
impl Model {
    fn valid(&self) -> bool {
        [&self.provider, &self.id]
            .iter()
            .all(|s| !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
            && self.name.len() <= 512
    }
}
impl TuiObserver {
    pub fn details(&self) -> Value {
        json!({"model":self.details.model.as_ref().filter(|m|m.valid()),
            "thinking_level":self.details.thinking_level.as_ref().filter(|s|s.len() <= 32),
            "queue_pending":self.details.has_pending_messages.or(self.details.pending_message_count.map(|n|n > 0)),
            "queue_count":self.details.pending_message_count})
    }
    /// This uses only advertised observation commands, not a native input lease.
    pub async fn model_options(&mut self) -> Result<Value> {
        self.snapshot_state().await?;
        let models = self.observe_options("get_available_models").await?;
        let list = models
            .get("scoped")
            .and_then(Value::as_array)
            .filter(|m| !m.is_empty())
            .or_else(|| models.get("models").and_then(Value::as_array))
            .context("Model catalog unavailable")?;
        ensure!(list.len() <= 2048, "Model catalog exceeds bound");
        let models: Vec<Model> = list
            .iter()
            .filter_map(|v| serde_json::from_value::<Model>(v.clone()).ok())
            .filter(Model::valid)
            .collect();
        let thinking = if self
            .commands
            .iter()
            .any(|c| c == "get_available_thinking_levels")
        {
            let data = self
                .observe_options("get_available_thinking_levels")
                .await?;
            data.get("levels")
                .and_then(Value::as_array)
                .map(|levels| {
                    levels
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|s| s.len() <= 32)
                        .take(16)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        self.snapshot_state().await?;
        Ok(
            json!({"models":models,"thinking_levels":thinking,"current":self.details(),
            "can_set_model":self.commands.iter().any(|c|c == "set_model"),
            "can_set_thinking":self.commands.iter().any(|c|c == "set_thinking_level")}),
        )
    }
    async fn observe_options(&mut self, command: &str) -> Result<Value> {
        ensure!(
            matches!(
                command,
                "get_available_models" | "get_available_thinking_levels"
            ) && self.commands.iter().any(|c| c == command),
            "Native configuration observation unsupported"
        );
        self.authority.revalidate(&self.binding)?;
        validate_source(&self.binding).await?;
        let mut reader = self.reader.take().context("TUI disconnected")?;
        let id = uuid::Uuid::new_v4().to_string();
        let result = tokio::time::timeout(SNAPSHOT_DEADLINE, async {
            reader
                .get_mut()
                .write_all(&serde_json::to_vec(&json!({"id":id,"type":command}))?)
                .await?;
            reader.get_mut().write_all(b"\n").await?;
            loop {
                let frame = read_frame(&mut reader).await?;
                match frame["type"].as_str() {
                    Some("event") => {
                        let _ = event(frame)?;
                    }
                    Some("lease") => ensure!(
                        !self.input_held() || frame["owner"] != "tui",
                        "Terminal reclaimed input"
                    ),
                    Some("response") => {
                        ensure!(
                            frame["id"] == id
                                && frame["command"] == command
                                && frame["success"] == true,
                            "Native configuration observation refused"
                        );
                        return Ok::<_, anyhow::Error>(frame["data"].clone());
                    }
                    _ => bail!("Unexpected configuration frame"),
                }
            }
        })
        .await
        .context("Configuration observation deadline")??;
        self.authority.revalidate(&self.binding)?;
        self.reader = Some(reader);
        Ok(result)
    }
    pub(super) async fn validate_setting(&mut self, action: &control::InputAction) -> Result<()> {
        match action {
            control::InputAction::SetModel { provider, model_id } => {
                let options = self.model_options().await?;
                ensure!(
                    !self.state.streaming && !self.state.compacting,
                    "Wait until Pi is idle before changing model"
                );
                ensure!(
                    options["can_set_model"] == true
                        && options["models"].as_array().is_some_and(|m| m
                            .iter()
                            .any(|v| v["provider"] == *provider && v["id"] == *model_id)),
                    "Model not in live Session catalog"
                );
            }
            control::InputAction::SetThinkingLevel { level } => {
                let options = self.model_options().await?;
                ensure!(
                    !self.state.streaming && !self.state.compacting,
                    "Wait until Pi is idle before changing thinking level"
                );
                ensure!(
                    options["can_set_thinking"] == true
                        && options["thinking_levels"]
                            .as_array()
                            .is_some_and(|l| l.iter().any(|v| v == level)),
                    "Thinking level not in live Session catalog"
                );
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_projection_never_serializes_provider_secrets() -> Result<()> {
        let raw = json!({"provider":"synthetic","id":"chat","name":"Chat","baseUrl":"https://invalid.example","apiKey":"synthetic-secret","headers":{"Authorization":"synthetic-header"}});
        let model: Model = serde_json::from_value(raw)?;
        let output = serde_json::to_string(&model)?;
        assert!(model.valid());
        assert!(
            !output.contains("secret")
                && !output.contains("headers")
                && !output.contains("baseUrl")
        );
        Ok(())
    }
}
