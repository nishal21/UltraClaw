#![allow(dead_code, unused_variables, unused_imports, unreachable_code, unexpected_cfgs)]

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
// 9. Initialize available standard connectors (CLI, Discord, Webhook, etc.)
// 10. Spawn a background task for session expiration
// 11. Enter the event loop (runs forever)
//
// TOTAL MEMORY BUDGET AT STARTUP (before any conversations):
// ┌────────────────────────────┬────────────┐
// │ Component                  │ RAM Usage   │
// ├────────────────────────────┼────────────┤
// │ Tokio runtime + threads    │ ~2 MB       │
// │ CLI / Webhook Connectors   │ ~1-2 MB     │
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
// mod matrix; // Moved to connectors/matrix.rs
mod mcp;
mod media;
mod media_skill;
mod memory;
mod onboarding; // New module
mod sandbox_skill;
mod search_skill;
mod session;
mod skill;
mod soul;
mod swarm_skill;
mod cron_skill;
mod tools;
mod cli;
mod connector;
mod connectors;
mod voice_skill;
mod browser_skill;
mod smarthome_skill;
mod system_nodes;
mod auth;
mod gateway;
mod memory_vector;
mod quota;
mod rag_sop;
mod robot_skill;
mod security_landlock;
mod skill_manager;
mod wasm_plugin;
mod git_resolver;
mod openclaw_skills;
mod tailscale_funnel;
mod group_context;
mod live_canvas;

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
    // STEP 1.5: Security Kernel Sandbox (Landlock)
    // ========================================================================
    let mut landlock = crate::security_landlock::LandlockSecurity::new();
    if let Err(e) = landlock.enforce() {
        warn!("Linux Landlock sandboxing not available or failed: {}", e);
    } else {
        info!("Linux Landlock strict kernel sandboxing engaged.");
    }

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

    for node_skill in crate::system_nodes::SystemNodesModule::register_all() {
        skills.register(node_skill);
    }
    info!("System Node tools (camera, screen_record, etc.) registered");

    let skills = Arc::new(skills);

    // ========================================================================
    // STEP 5.5: Initialize Level 2 Conflict Resolver & OpenClaw Registry
    // ========================================================================
    let git_resolver = crate::git_resolver::SemanticGitResolver::new();
    info!("NanoClaw Level 2 Semantic Git Conflict Resolution Engine active.");

    let openclaw_registry = crate::openclaw_skills::OpenClawSkillRegistry::new();
    info!("OpenClaw Hyper-Skill Module loaded with {} custom extensions.", openclaw_registry.list_extensions().len());

    let _tailscale = crate::tailscale_funnel::TailscaleFunnel::new();
    let _group_ctx = crate::group_context::GroupContextManager::new(std::path::PathBuf::from("/tmp/ultraclaw_groups"));
    let _canvas = crate::live_canvas::LiveCanvasProtocol::new();
    
    let massive_channels_init = crate::connectors::massive_channels::MassiveChannelsInit::new();
    massive_channels_init.initialize_all();

    let api_gateway = crate::gateway::ApiGateway::new(3030);
    api_gateway.start();

    info!("Tailscale Funnel, Group Context, Live Canvas, API Gateway (port 3030), and Massive Channels initialized natively.");

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
    // Attempt auto-discovery of local models if path is empty or default
    let local_path = if config.local_model_path.is_empty() || config.local_model_path == "models/llama3-8b.gguf" {
        // Search common model directories for any .gguf files
        let common_dirs = vec!["models", ".", "../models", "/models", "C:/models"];
        let mut found_path = config.local_model_path.clone();
        for dir in common_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "gguf") {
                        found_path = path.to_string_lossy().to_string();
                        tracing::info!("Auto-discovered local model at: {}", found_path);
                        break;
                    }
                }
            }
            if found_path != config.local_model_path && !found_path.is_empty() {
                break;
            }
        }
        found_path
    } else {
        config.local_model_path.clone()
    };

    let local = LocalEngine::new(&local_path);
    info!(
        model_path = %local_path,
        "Local inference engine initialized (model will be mmap'd on first use)"
    );

    // Failover engine: tries Cloud first, falls back to Local on failure
    let engine: Arc<dyn crate::inference::InferenceEngine> =
        Arc::new(FailoverEngine::new(cloud, local));
    info!("Failover engine ready: Cloud → Local");

    // ========================================================================
    // STEP 9: Initialize Connectors
    // ========================================================================
    // Ultraclaw supports running multiple connectors simultaneously (Matrix, CLI, etc.)
    // We determine which ones to run based on args and config.
    use crate::connector::Connector;

    let mut connectors: Vec<Box<dyn Connector>> = Vec::new();

    // Check for CLI mode flag argument
    if args.contains(&"--cli".to_string()) {
        info!("CLI mode enabled via flag");
        connectors.push(Box::new(cli::CliConnector::new()));
    } else {
    // No default connector inserted here because Matrix was removed by user request.
    }

    // --- Discord Connector ---
    #[cfg(feature = "discord")]
    {
        if config.discord_token.is_some() {
            info!("Discord connector enabled via config");
            connectors.push(Box::new(connectors::discord::DiscordConnector::new()));
        }
    }

    // --- Telegram Connector ---
    #[cfg(feature = "telegram")]
    {
        if config.telegram_token.is_some() {
            info!("Telegram connector enabled via config");
            connectors.push(Box::new(connectors::telegram::TelegramConnector::new()));
        }
    }

    // --- Webhook Connector ---
    #[cfg(feature = "webhook")]
    {
        info!("Webhook connector enabled");
        connectors.push(Box::new(connectors::webhook::WebhookConnector::new()));
    }

    // Default fallback: If NO connectors are enabled, prompt user or enable Matrix?
    // For now, if list is empty, enable Matrix as default unless --cli was passed?
    // Logic refinement:
    if connectors.is_empty() && !args.contains(&"--cli".to_string()) {
         info!("No connectors enabled. Defaulting to CLI.");
         connectors.push(Box::new(cli::CliConnector::new()));
    }

    if connectors.is_empty() {
        warn!("No connectors enabled! Exiting.");
        return Ok(());
    }

    // ========================================================================
    // STEP 10: Spawn background maintenance tasks
    // ========================================================================
    // Session expiration sweep — runs every 60 seconds.
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
    // STEP 11: Run Connectors
    // ========================================================================
    info!("Starting {} connector(s)...", connectors.len());

    // Spawn a task for each connector
    let mut handles = Vec::new();

    for connector in connectors {
        let engine = engine.clone();
        let db = conv_db.clone();
        let memory = memory_store.clone();
        let sessions = sessions.clone();
        let soul = soul.clone();
        let skills = skills.clone();
        let mcp = mcp_client.clone();
        let config = config.clone();

        let name = connector.name().to_string();
        info!(connector = %name, "Launching connector");

        handles.push(tokio::spawn(async move {
            if let Err(e) = connector.run(engine, db, memory, sessions, soul, skills, mcp, config).await {
                error!(connector = %name, error = %e, "Connector failed");
            }
        }));
    }

    // Wait for all connectors (they usually run forever)
    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}
