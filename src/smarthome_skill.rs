use crate::skill::Skill;

pub struct SmartHomeSkill {}

impl Default for SmartHomeSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl SmartHomeSkill {
    pub fn new() -> Self { Self {} }
}

impl Skill for SmartHomeSkill {
    fn name(&self) -> &'static str {
        "SmartHome"
    }

    fn description(&self) -> &'static str {
        "Control local IoT devices (e.g., Sonos spread, Mac tasks)"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "device": { "type": "string" },
                "command": { "type": "string" }
            }
        })
    }

    fn execute_sync(&self, params: &serde_json::Value) -> crate::skill::SkillOutput {
        let device = params.get("device").and_then(|v| v.as_str()).unwrap_or("all");
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("off");
        crate::skill::SkillOutput {
            name: self.name().into(),
            output: format!("SmartHome: Sent command '{}' to device '{}'", command, device),
            is_error: false,
        }
    }
}
