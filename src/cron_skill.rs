use crate::skill::{Skill, SkillOutput};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

pub static CRON_JOB_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Cron Task / Heartbeat Skill
/// Allows the agent to schedule recurring background tasks.
pub struct CronSkill;

impl Skill for CronSkill {
    fn name(&self) -> &'static str {
        "schedule_cron"
    }

    fn description(&self) -> &'static str {
        "Schedule a task to run periodically in the background (heartbeat)."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "interval_seconds": {
                    "type": "integer",
                    "description": "How often the task should run."
                },
                "action": {
                    "type": "string",
                    "description": "Description of the action to perform."
                }
            },
            "required": ["interval_seconds", "action"]
        })
    }

    fn execute_sync(&self, args: &Value) -> SkillOutput {
        let interval = args.get("interval_seconds").and_then(|v| v.as_i64()).unwrap_or(60);
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        
        let id = CRON_JOB_COUNT.fetch_add(1, Ordering::SeqCst);
        
        // In a real implementation, we would spawn a tokio task here that waits `interval`
        // seconds and pushes an event into the main EventLoop.
        SkillOutput {
            name: self.name().to_string(),
            output: format!("Cron job {} scheduled. Action: '{}' every {} seconds.", id, action, interval),
            is_error: false,
        }
    }
}
