// ============================================================================
// ULTRACLAW — main.rs
// ============================================================================
// Entrypoint for the Ultraclaw autonomous AI agent.
//
// This file orchestrates the boot sequence:
// 1. Initialize structured logging (tracing)
// 2. Load configuration from environment variables
// 3. Open the SQLite databases (conversation context + long-term memory)
// 4. Build the Soul (agent personality + directives)
// 5. Register built-in Skills (file read, command exec, dir list)
// 6. Optionally connect to MCP servers for external tools
// 7. Create the SessionManager for multi-turn conversation tracking
// 8. Initialize the FailoverEngine (Cloud → Local LLM)
// 9. Log into Matrix
// 10. Spawn a background task for session expiration
// 11. Enter the Matrix event loop (runs forever)
//
// TOTAL MEMORY BUDGET AT STARTUP (before any conversations):
// ┌────────────────────────────┬────────────┐
// │ Component                  │ RAM Usage   │
// ├────────────────────────────┼────────────┤
// │ Tokio runtime + threads    │ ~2 MB       │
// │ Matrix SDK client          │ ~1-2 MB     │
// │ SQLite conv DB (page cache)│ ~2 MB       │
// │ SQLite memory DB           │ ~2 MB       │
// │ Soul (persona + directives)│ ~1 KB       │
// │ SkillRegistry              │ ~500 B      │
// │ SessionManager (empty)     │ ~100 B      │
// │ Reqwest HTTP client        │ ~200 B      │
// │ Config struct              │ ~500 B      │
// │ Binary code (.text segment)│ ~5-10 MB    │
// │ Stack space (main + tasks) │ ~1 MB       │
// ├────────────────────────────┼────────────┤
// │ TOTAL (idle, no model)     │ ~13-18 MB   │
// │ + Local model (mmap RSS)   │ ~500 MB-1GB │
// └────────────────────────────┴────────────┘
//
// Note: The local model's mmap'd pages are demand-paged. The ~500MB-1GB
// is only resident during active local inference. At idle, the OS can
// reclaim these pages, bringing total RSS back to ~13-18 MB.
//
// ENERGY BUDGET:
// - Idle (no messages): ~0.1W (epoll_wait, sleeping)
// - Processing a message (cloud): ~0.5W for ~2 seconds
// - Processing a message (local): ~5-15W for ~5-30 seconds (CPU inference)
// - For context: a Raspberry Pi 4 draws ~3W at idle, ~6W under full CPU load
// ============================================================================

// Module declarations — each file becomes a module in the crate.
// The compiler only includes code that is actually used, so dead modules
// don't contribute to binary size (with LTO enabled).
mod config;
mod db;
mod formatter;
mod inference;
mod matrix;
mod mcp;
mod media;
mod media_skill;
mod memory;
mod onboarding; // New module
mod session;
mod skill;
mod soul;
mod tools;

use crate::config::Config;
use crate::db::ConversationDb;
use crate::inference::{CloudEngine, FailoverEngine, LocalEngine};
// use crate::matrix::MatrixClient; // Unused import removed
use crate::media::{MediaEngine, MediaProvider};
use crate::media_skill::{GenerateImageSkill, GenerateVideoSkill};
use crate::memory::MemoryStore;
use crate::mcp::McpClient;
use crate::session::SessionManager;
use crate::skill::SkillRegistry;
use crate::soul::Soul;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// The main entry point for Ultraclaw.
///
/// Uses `#[tokio::main]` which expands to a multi-threaded async runtime.
/// The runtime spawns worker threads equal to the number of CPU cores.
/// On a single-core device, it uses 1 worker thread (no overhead).
///
/// `current_thread` flavor could save ~500KB of RAM by using cooperative
/// scheduling on a single thread. We use `multi_thread` for robustness:
/// if a local inference blocks a thread, other rooms can still be served.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ========================================================================
    // STEP 1: Initialize structured logging
    // ========================================================================
    // The env filter allows runtime control: RUST_LOG=ultraclaw=debug
    // Default: info level (minimal output, minimal I/O energy).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("ultraclaw=info")),
        )
        .compact() // Compact format saves terminal I/O bandwidth
        .init();

    info!("╔══════════════════════════════════════════════════╗");
    info!("║          ULTRACLAW — AI Agent v0.1.0             ║");
    info!("║  Hyper-Optimized Multi-Platform Inference Engine  ║");
    info!("╚══════════════════════════════════════════════════╝");

    // ========================================================================
    // STEP 2: Load configuration
    // ========================================================================
    // Check for --init flag or missing config
    let args: Vec<String> = std::env::args().collect();
    let force_init = args.contains(&"--init".to_string());
    
    // Attempt to load config; if it fails or init requested, run wizard
    let config = if force_init || Config::load().is_err() || !Config::load().unwrap().is_valid() {
        if !force_init {
             info!("Configuration missing or invalid. Starting onboarding wizard...");
        }
        if let Err(e) = onboarding::run_wizard() {
             error!("Wizard failed: {}", e);
             return Ok(());
        }
        // Reload after wizard
        Config::load().expect("Failed to load config after wizard")
    } else {
        Config::load().unwrap()
    };
    
    info!(
        homeserver = %config.homeserver_url,
        model = %config.cloud_model,
        local_model = %config.local_model_path,
        db_path = %config.db_path,
        session_ttl = config.session_ttl_secs,
        max_sessions = config.max_sessions,
        "Configuration loaded"
    );
    let config = Arc::new(config);

    // ========================================================================
    // STEP 3: Open databases
    // ========================================================================
    // Conversation context DB — short-term, per-room message history.
    // SQLite with WAL mode, 2MB page cache.
    let conv_db = ConversationDb::open(&config.db_path)
        .expect("Failed to open conversation database");
    info!(path = %config.db_path, "Conversation database opened");
    let conv_db = Arc::new(Mutex::new(conv_db));

    // Long-term memory DB — persistent facts, preferences, instructions.
    // Stored alongside the conversation DB (same file for simplicity,
    // or a separate file for isolation — configurable).
    let memory_db_path = format!("{}.memory", config.db_path);
    let memory_store = MemoryStore::open(&memory_db_path)
        .expect("Failed to open memory database");
    info!(path = %memory_db_path, "Memory database opened");

    // Prune old memories on startup (garbage collection)
    match memory_store.prune(config.memory_max_age_days, 0.2) {
        Ok(n) if n > 0 => info!(pruned = n, "Pruned old memories"),
        _ => {}
    }
    let memory_store = Arc::new(Mutex::new(memory_store));

    // ========================================================================
    // STEP 4: Build the Soul
    // ========================================================================
    // The Soul defines the agent's personality and behavioral directives.
    // Default Ultraclaw persona uses static strings (zero heap allocation).
    let soul = Soul::default_soul();
    info!(
        name = %soul.name,
        directives = soul.directives.len(),
        temperature = soul.temperature,
        "Soul initialized"
    );
    let soul = Arc::new(soul);

    // ========================================================================
    // STEP 5: Register Skills
    // ========================================================================
    // Built-in skills: read_file, list_directory, run_command.
    // These are the agent's "hands" — they execute side-effects.
    let mut skills = SkillRegistry::new();
    info!("Skill registry initialized with built-in skills");

    // ========================================================================
    // STEP 5b: Initialize Media Engine + Register Media Skills
    // ========================================================================
    // Build the API key map from config. Only providers with non-empty keys
    // are added — the MediaEngine auto-selects the best available provider.
    let mut media_keys: HashMap<MediaProvider, String> = HashMap::new();
    let key_pairs = [
        (MediaProvider::OpenAI, config.cloud_api_key.as_str()),
        (MediaProvider::Stability, config.stability_api_key.as_str()),
        (MediaProvider::Runway, config.runway_api_key.as_str()),
        (MediaProvider::Replicate, config.replicate_api_key.as_str()),
        (MediaProvider::Together, config.together_api_key.as_str()),
        (MediaProvider::Fal, config.fal_api_key.as_str()),
        (MediaProvider::Leonardo, config.leonardo_api_key.as_str()),
        (MediaProvider::Imagen, config.imagen_api_key.as_str()),
        (MediaProvider::Veo, config.veo_api_key.as_str()),
        (MediaProvider::Kling, config.kling_api_key.as_str()),
        (MediaProvider::Seedance, config.seedance_api_key.as_str()),
        (MediaProvider::Luma, config.luma_api_key.as_str()),
        (MediaProvider::Minimax, config.minimax_api_key.as_str()),
        (MediaProvider::Pika, config.pika_api_key.as_str()),
        (MediaProvider::Sora, config.sora_api_key.as_str()),
    ];
    for (provider, key) in &key_pairs {
        if !key.is_empty() {
            media_keys.insert(*provider, key.to_string());
        }
    }

    let has_media = !media_keys.is_empty();
    let media_engine = Arc::new(Mutex::new(MediaEngine::new(
        media_keys,
        std::path::PathBuf::from(&config.media_output_dir),
        MediaProvider::from_str_loose(&config.media_image_provider),
        MediaProvider::from_str_loose(&config.media_video_provider),
    )));

    if has_media {
        // Register media skills with the MediaEngine
        skills.register(Box::new(GenerateImageSkill::new(media_engine.clone())));
        skills.register(Box::new(GenerateVideoSkill::new(media_engine.clone())));
        info!("Media skills registered (generate_image, generate_video)");
    } else {
        info!("No media API keys configured — media skills disabled");
    }

    let skills = Arc::new(skills);

    // ========================================================================
    // STEP 6: Connect to MCP servers (optional)
    // ========================================================================
    // If configured, spawn an MCP server process and connect via stdio pipes.
    let mcp_client: Option<Arc<McpClient>> = if !config.mcp_server_command.is_empty() {
        info!(
            command = %config.mcp_server_command,
            "Connecting to MCP server..."
        );
        match McpClient::connect(&config.mcp_server_command, &[]).await {
            Ok(client) => {
                // List available tools from the MCP server
                match client.list_tools().await {
                    Ok(tools) => {
                        info!(
                            tool_count = tools.len(),
                            "MCP server connected, tools discovered"
                        );
                        for tool in &tools {
                            info!(tool = %tool.name, "  MCP tool available");
                        }
                    }
                    Err(e) => warn!("Failed to list MCP tools: {}", e),
                }
                Some(Arc::new(client))
            }
            Err(e) => {
                error!(error = %e, "Failed to connect to MCP server (continuing without MCP)");
                None
            }
        }
    } else {
        info!("MCP not configured (ULTRACLAW_MCP_SERVER_COMMAND is empty)");
        None
    };

    // ========================================================================
    // STEP 7: Create Session Manager
    // ========================================================================
    // Tracks multi-turn conversation sessions per room.
    // Max 256 sessions × ~140 bytes each = ~35KB max RAM usage.
    let sessions = SessionManager::new(config.session_ttl_secs, config.max_sessions);
    info!(
        ttl_secs = config.session_ttl_secs,
        max = config.max_sessions,
        "Session manager initialized"
    );
    let sessions = Arc::new(Mutex::new(sessions));

    // ========================================================================
    // STEP 8: Initialize Inference Engines
    // ========================================================================
    // Cloud engine: reqwest HTTP client → OpenAI-compatible API
    let cloud = CloudEngine::new(
        &config.cloud_api_key,
        &config.cloud_model,
        &config.cloud_base_url,
    );
    info!(
        model = %config.cloud_model,
        base_url = %config.cloud_base_url,
        "Cloud inference engine initialized"
    );

    // Local engine: llama.cpp with mmap'd GGUF model
    // Note: The model is NOT loaded yet. It will be memory-mapped on first
    // local inference call (lazy loading to avoid wasting RAM if cloud works).
    let local = LocalEngine::new(&config.local_model_path);
    info!(
        model_path = %config.local_model_path,
        "Local inference engine initialized (model will be mmap'd on first use)"
    );

    // Failover engine: tries Cloud first, falls back to Local on failure
    let engine: Arc<dyn crate::inference::InferenceEngine> =
        Arc::new(FailoverEngine::new(cloud, local));
    info!("Failover engine ready: Cloud → Local");

    // ========================================================================
    // STEP 9: Log into Matrix
    // ========================================================================
    let client = matrix::login(&config)
        .await
        .expect("Matrix login failed — check ULTRACLAW_HOMESERVER_URL, _MATRIX_USER, _MATRIX_PASSWORD");

    // ========================================================================
    // STEP 10: Spawn background maintenance tasks
    // ========================================================================
    // Session expiration sweep — runs every 60 seconds.
    // Uses a tokio interval (timer wheel entry, zero-cost when not firing).
    {
        let sessions = sessions.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let mut sessions = sessions.lock().await;
                let expired = sessions.expire_idle();
                if expired > 0 {
                    info!(expired = expired, "Expired idle sessions");
                }
            }
        });
    }

    // ========================================================================
    // STEP 11: Enter the Matrix event loop (runs forever)
    // ========================================================================
    info!("Entering Matrix event loop — Ultraclaw is now live!");
    info!("Listening for messages across all bridged platforms...");

    matrix::run_event_loop(
        client,
        engine,
        conv_db,
        memory_store,
        sessions,
        soul,
        skills,
        mcp_client,
        config,
    )
    .await;

    Ok(())
}
