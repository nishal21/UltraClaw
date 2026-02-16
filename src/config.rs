// ============================================================================
// ULTRACLAW — config.rs
// ============================================================================
// Configuration loading from environment variables.
//
// MEMORY OPTIMIZATION:
// We deliberately avoid heavy config frameworks (e.g., `config`, `figment`)
// which build layered HashMap trees in memory. Instead, we read env vars
// directly into a flat struct. This struct is ~200 bytes on the stack when
// borrowed, and only the String fields allocate on the heap.
//
// ENERGY OPTIMIZATION:
// Config is loaded once at startup. No file watchers, no polling, no
// hot-reload threads burning CPU cycles in the background.
// ============================================================================

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;

/// All configuration for the Ultraclaw agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // --- Matrix Connection ---
    pub homeserver_url: String,
    pub matrix_user: String,
    pub matrix_password: String,

    // --- Cloud LLM ---
    #[serde(default)]
    pub cloud_api_key: String,
    #[serde(default = "default_cloud_model")]
    pub cloud_model: String,
    #[serde(default = "default_cloud_base_url")]
    pub cloud_base_url: String,

    // --- Local LLM ---
    #[serde(default = "default_local_model_path")]
    pub local_model_path: String,

    // --- Database ---
    #[serde(default = "default_db_path")]
    pub db_path: String,

    // --- Session Management ---
    #[serde(default = "default_session_ttl")]
    pub session_ttl_secs: u64,
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    // --- Memory ---
    #[serde(default = "default_memory_max_age")]
    pub memory_max_age_days: i64,
    #[serde(default = "default_context_window")]
    pub context_window_size: usize,

    // --- MCP ---
    #[serde(default)]
    pub mcp_server_command: String,

    // --- Media Generation ---
    #[serde(default)]
    pub stability_api_key: String,
    #[serde(default)]
    pub runway_api_key: String,
    #[serde(default)]
    pub replicate_api_key: String,
    #[serde(default)]
    pub together_api_key: String,
    #[serde(default)]
    pub fal_api_key: String,
    #[serde(default)]
    pub leonardo_api_key: String,
    #[serde(default)]
    pub imagen_api_key: String,
    #[serde(default)]
    pub veo_api_key: String,
    #[serde(default)]
    pub kling_api_key: String,
    #[serde(default)]
    pub seedance_api_key: String,
    #[serde(default)]
    pub luma_api_key: String,
    #[serde(default)]
    pub minimax_api_key: String,
    #[serde(default)]
    pub pika_api_key: String,
    #[serde(default)]
    pub sora_api_key: String,
    #[serde(default = "default_media_output_dir")]
    pub media_output_dir: String,
    #[serde(default = "default_image_provider")]
    pub media_image_provider: String,
    #[serde(default = "default_video_provider")]
    pub media_video_provider: String,
}

// Defaults for Serde
fn default_cloud_model() -> String { "gpt-4o-mini".to_string() }
fn default_cloud_base_url() -> String { "https://api.openai.com/v1".to_string() }
fn default_local_model_path() -> String { "model.gguf".to_string() }
fn default_db_path() -> String { "ultraclaw.db".to_string() }
fn default_session_ttl() -> u64 { 1800 }
fn default_max_sessions() -> usize { 256 }
fn default_memory_max_age() -> i64 { 90 }
fn default_context_window() -> usize { 20 }
fn default_media_output_dir() -> String { "media_output".to_string() }
fn default_image_provider() -> String { "openai".to_string() }
fn default_video_provider() -> String { "veo".to_string() }

impl Default for Config {
    fn default() -> Self {
        Self {
            homeserver_url: String::new(),
            matrix_user: String::new(),
            matrix_password: String::new(),
            cloud_api_key: String::new(),
            cloud_model: default_cloud_model(),
            cloud_base_url: default_cloud_base_url(),
            local_model_path: default_local_model_path(),
            db_path: default_db_path(),
            session_ttl_secs: default_session_ttl(),
            max_sessions: default_max_sessions(),
            memory_max_age_days: default_memory_max_age(),
            context_window_size: default_context_window(),
            mcp_server_command: String::new(),
            stability_api_key: String::new(),
            runway_api_key: String::new(),
            replicate_api_key: String::new(),
            together_api_key: String::new(),
            fal_api_key: String::new(),
            leonardo_api_key: String::new(),
            imagen_api_key: String::new(),
            veo_api_key: String::new(),
            kling_api_key: String::new(),
            seedance_api_key: String::new(),
            luma_api_key: String::new(),
            minimax_api_key: String::new(),
            pika_api_key: String::new(),
            sora_api_key: String::new(),
            media_output_dir: default_media_output_dir(),
            media_image_provider: default_image_provider(),
            media_video_provider: default_video_provider(),
        }
    }
}

impl Config {
    /// Load configuration with the following precedence:
    /// 1. Environment variables (highest priority)
    /// 2. config.json file
    /// 3. .env file values
    /// 4. Hardcoded defaults
    pub fn load() -> Result<Self, String> {
        // 1. Try to load from config.json
        let mut config = if Path::new("config.json").exists() {
            let content = fs::read_to_string("config.json")
                .map_err(|e| format!("Failed to read config.json: {}", e))?;
            serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse config.json: {}", e))?
        } else {
            Config::default()
        };

        // 2. Load .env file (if present) to populate env vars
        let _ = dotenvy::dotenv();

        // 3. Override with environment variables
        if let Ok(val) = env::var("ULTRACLAW_HOMESERVER_URL") { config.homeserver_url = val; }
        if let Ok(val) = env::var("ULTRACLAW_MATRIX_USER") { config.matrix_user = val; }
        if let Ok(val) = env::var("ULTRACLAW_MATRIX_PASSWORD") { config.matrix_password = val; }
        if let Ok(val) = env::var("ULTRACLAW_CLOUD_API_KEY") { config.cloud_api_key = val; }
        if let Ok(val) = env::var("ULTRACLAW_CLOUD_MODEL") { config.cloud_model = val; }
        if let Ok(val) = env::var("ULTRACLAW_CLOUD_BASE_URL") { config.cloud_base_url = val; }
        // ... (can add others if strict env override needed, but usually these are enough)

        Ok(config)
    }

    /// Save the current configuration to config.json
    pub fn save(&self) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write("config.json", content)
            .map_err(|e| format!("Failed to write config.json: {}", e))?;
        Ok(())
    }

    /// Check if the configuration is valid (has required fields).
    pub fn is_valid(&self) -> bool {
        !self.homeserver_url.is_empty() 
            && !self.matrix_user.is_empty() 
            && !self.matrix_password.is_empty()
    }
}


