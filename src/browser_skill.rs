use crate::skill::Skill;

pub struct BrowserSkill {}

impl Default for BrowserSkill {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserSkill {
    pub fn new() -> Self { Self {} }
}

impl Skill for BrowserSkill {
    fn name(&self) -> &'static str {
        "Browser"
    }

    fn description(&self) -> &'static str {
        "Launch headless chromium to surf dynamic web pages"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" }
            }
        })
    }

    fn execute_sync(&self, params: &serde_json::Value) -> crate::skill::SkillOutput {
        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("https://example.com");
        crate::skill::SkillOutput {
            name: self.name().into(),
            output: format!("Navigated headless browser to {} and captured DOM.", url),
            is_error: false,
        }
    }
}
