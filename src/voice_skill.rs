use crate::skill::Skill;

pub struct VoiceSkill {
    is_active: bool,
}

impl Default for VoiceSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceSkill {
    pub fn new() -> Self {
        Self { is_active: false }
    }
    
    pub fn start_call(&mut self) -> String {
        self.is_active = true;
        "WebRTC Voice Call Initialized".into()
    }
    
    pub fn end_call(&mut self) -> String {
        self.is_active = false;
        "Voice Call Terminated".into()
    }
}

impl Skill for VoiceSkill {
    fn name(&self) -> &'static str {
        "Voice"
    }

    fn description(&self) -> &'static str {
        "Join or manage voice calls"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string" }
            }
        })
    }

    fn execute_sync(&self, params: &serde_json::Value) -> crate::skill::SkillOutput {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("status");
        let output = match action {
            "start" => "Starting voice session...".into(),
            "stop" => "Stopping voice session...".into(),
            _ => "Voice channel status: OK".into(),
        };
        crate::skill::SkillOutput {
            name: self.name().into(),
            output,
            is_error: false,
        }
    }
}
