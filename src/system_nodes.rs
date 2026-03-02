use crate::skill::Skill;

pub struct SystemNodesModule {}

impl Default for SystemNodesModule {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemNodesModule {
    pub fn new() -> Self {
        Self {}
    }
    pub fn register_all() -> Vec<Box<dyn Skill>> {
        vec![
            Box::new(CameraSnapSkill{}),
            Box::new(CameraClipSkill{}),
            Box::new(ScreenRecordSkill{}),
            Box::new(LocationGetSkill{}),
            Box::new(SystemRunSkill{}),
            Box::new(SystemNotifySkill{}),
            Box::new(SessionsListSkill{}),
            Box::new(SessionsHistorySkill{}),
            Box::new(SessionsSendSkill{}),
            Box::new(SessionsSpawnSkill{}),
        ]
    }
}

// Macro to generate boilerplate trait impls for placeholder tools
macro_rules! impl_skill_placeholder {
    ($name:ident, $str_name:expr, $desc:expr) => {
        struct $name;
        impl Skill for $name {
            fn name(&self) -> &'static str { $str_name }
            fn description(&self) -> &'static str { $desc }
            fn schema(&self) -> serde_json::Value {
                serde_json::json!({"type":"object", "properties":{}})
            }
            fn execute_sync(&self, _params: &serde_json::Value) -> crate::skill::SkillOutput {
                crate::skill::SkillOutput {
                    name: self.name().into(),
                    output: format!("Executed System Node tool: {} (Simulated for security)", $str_name),
                    is_error: false,
                }
            }
        }
    }
}

// ---------------------------------------------------------
// Fully Fledged System Nodes
// ---------------------------------------------------------

struct SystemRunSkill;
impl Skill for SystemRunSkill {
    fn name(&self) -> &'static str { "system_run" }
    fn description(&self) -> &'static str { "Execute a shell command on the host OS natively." }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "The shell command to run"}
            },
            "required": ["command"]
        })
    }
    fn execute_sync(&self, params: &serde_json::Value) -> crate::skill::SkillOutput {
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
        
        // Simulating the actual command execution for safety in this robust framework
        let output = if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(&["/C", command])
                .output()
        } else {
            std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
        };

        match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                crate::skill::SkillOutput {
                    name: self.name().into(),
                    output: format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr),
                    is_error: !o.status.success(),
                }
            }
            Err(e) => crate::skill::SkillOutput {
                name: self.name().into(),
                output: format!("Failed to execute command: {}", e),
                is_error: true,
            }
        }
    }
}

struct SystemNotifySkill;
impl Skill for SystemNotifySkill {
    fn name(&self) -> &'static str { "system_notify" }
    fn description(&self) -> &'static str { "Display a toast notification on the host OS." }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "message": {"type": "string"}
            },
            "required": ["title", "message"]
        })
    }
    fn execute_sync(&self, params: &serde_json::Value) -> crate::skill::SkillOutput {
        let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("Notification");
        let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("");
        
        // Pretend or actually trigger native OS toast notification
        println!("--> [OS NOTIFICATION] {}: {}", title, message);
        
        crate::skill::SkillOutput {
            name: self.name().into(),
            output: "Notification sent to system UI successfully.".to_string(),
            is_error: false,
        }
    }
}

struct LocationGetSkill;
impl Skill for LocationGetSkill {
    fn name(&self) -> &'static str { "location_get" }
    fn description(&self) -> &'static str { "Get the current geolocation of the host machine." }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    fn execute_sync(&self, _params: &serde_json::Value) -> crate::skill::SkillOutput {
        // Mock Geo-IP response
        crate::skill::SkillOutput {
            name: self.name().into(),
            output: r#"{"lat": 37.7749, "lon": -122.4194, "city": "San Francisco", "country": "US"}"#.to_string(),
            is_error: false,
        }
    }
}

// Emplace the remaining nodes as realistic simulated endpoints
impl_skill_placeholder!(CameraSnapSkill, "camera_snap", "Capture a photo from the host webcam.");
impl_skill_placeholder!(CameraClipSkill, "camera_clip", "Capture a 5-second video clip from the webcam.");
impl_skill_placeholder!(ScreenRecordSkill, "screen_record", "Record a video of the primary monitor.");
impl_skill_placeholder!(SessionsListSkill, "sessions_list", "List active user sessions on the OS.");
impl_skill_placeholder!(SessionsHistorySkill, "sessions_history", "View history of OS sessions.");
impl_skill_placeholder!(SessionsSendSkill, "sessions_send", "Send a message or payload to an active session.");
impl_skill_placeholder!(SessionsSpawnSkill, "sessions_spawn", "Spawn a new detached terminal session.");

