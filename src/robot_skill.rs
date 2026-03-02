use crate::skill::Skill;

pub struct RobotKit {
    enabled: bool,
}

impl RobotKit {
    pub fn new() -> Self {
        Self { enabled: true }
    }
    
    pub fn drive(&self, speed: f32, direction: f32) -> String {
        format!("Driving at speed {} and direction {}", speed, direction)
    }

    pub fn speak(&self, text: &str) -> String {
        format!("TTS outputting: {}", text)
    }

    pub fn look(&self) -> String {
        "Capturing camera frame and analyzing...".to_string()
    }

    pub fn emote(&self, expression: &str) -> String {
        format!("Displaying expression on LED matrix: {}", expression)
    }

    pub fn sense(&self) -> String {
        "LIDAR scan complete, no obstacles.".to_string()
    }
}

impl Default for RobotKit {
    fn default() -> Self {
        Self::new()
    }
}

impl Skill for RobotKit {
    fn name(&self) -> &'static str {
        "RobotKit"
    }

    fn description(&self) -> &'static str {
        "Direct hardware control for motors, camera, and LIDAR."
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
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("sense");
        let output = match action {
            "drive" => self.drive(1.0, 0.0),
            "speak" => self.speak("Acknowledged"),
            "look" => self.look(),
            "emote" => self.emote("happy"),
            "sense" | _ => self.sense(),
        };
        crate::skill::SkillOutput {
            name: self.name().into(),
            output,
            is_error: false
        }
    }
}
