// ============================================================================
// ULTRACLAW — Telegram Connector
// ============================================================================
// Integration with Telegram Bot API via `teloxide`.
//
// MEMORY OPTIMIZATION:
// - Uses `rustls` backend via `teloxide` features.
// - Long-polling mode (no webhook server needed).

use crate::config::Config;
use crate::connector::Connector;
use crate::db::{ChatMessage, ConversationDb};
use crate::formatter::Platform;
use crate::inference::InferenceEngine;
use crate::mcp::McpClient;
use crate::memory::MemoryStore;
use crate::session::SessionManager;
use crate::skill::SkillRegistry;
use crate::soul::Soul;
use crate::tools;

use anyhow::Context as _;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use teloxide::prelude::*;

pub struct TelegramConnector;

impl TelegramConnector {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Connector for TelegramConnector {
    fn name(&self) -> &str {
        "Telegram"
    }

    async fn run(
        &self,
        engine: Arc<dyn InferenceEngine>,
        db: Arc<Mutex<ConversationDb>>,
        memory: Arc<Mutex<MemoryStore>>,
        sessions: Arc<Mutex<SessionManager>>,
        soul: Arc<Soul>,
        skills: Arc<SkillRegistry>,
        mcp: Option<Arc<McpClient>>,
        config: Arc<Config>,
    ) -> anyhow::Result<()> {
        let token = config
            .telegram_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Telegram token not configured"))?;

        let bot = Bot::new(token);
        
        info!("Telegram Connector starting...");

        // Verify connection
        let me = bot.get_me().await.context("Failed to connect to Telegram")?;
        info!("Connected as @{}", me.user.username.unwrap_or_default());

        // Clone Arcs for the closure
        let engine = engine.clone();
        let db = db.clone();
        let memory = memory.clone();
        let sessions = sessions.clone();
        let soul = soul.clone();
        let skills = skills.clone();
        let mcp = mcp.clone();
        let config = config.clone();

        // Start REPL (Read-Eval-Print Loop) for Telegram
        // This blocks the async task until the bot stops.
        teloxide::repl(bot, move |bot: Bot, msg: Message| {
            let engine = engine.clone();
            let db = db.clone();
            let memory = memory.clone();
            let sessions = sessions.clone();
            let soul = soul.clone();
            let skills = skills.clone();
            let mcp = mcp.clone();
            let config = config.clone();

            async move {
                // Ignore non-text messages for now
                let content = match msg.text() {
                    Some(text) => text.to_string(),
                    None => return Ok(()),
                };
                
                let user_id = msg.chat.id.to_string(); // Chat ID is enough for context
                let username = msg.chat.username().map(|u| u.to_string()).unwrap_or_else(|| "Unknown".to_string());
                let platform = Platform::Telegram;
                
                info!(user = %username, chat_id = %user_id, "Received Telegram message");

                // --- BRAIN LOOP START ---
                
                // 1. Ensure Session Exists
                {
                    let mut sessions = sessions.lock().await;
                    sessions.get_or_create(&user_id, platform);
                }

                // 2. Store User Message
                {
                    let db = db.lock().await;
                    if let Err(e) = db.append_message(&user_id, "user", &content) {
                        error!("Failed to store user message: {}", e);
                    }
                }

                // 3. Load Context
                let context = {
                    let db = db.lock().await;
                    db.get_context(&user_id, config.context_window_size).unwrap_or_default()
                };

                // 4. Recall Memory
                let memory_context = {
                    let mem = memory.lock().await;
                    mem.summarize_for_context(&user_id, 200).unwrap_or(None)
                };

                // 5. Build System Prompt
                let session_context = {
                    let sessions = sessions.lock().await;
                    sessions.get_session_context(&user_id)
                };

                let system_msg = soul.build_system_message(
                    Some("User is on Telegram. Keep formatting simple (bold, italic, code)."),
                    session_context.as_deref(),
                    memory_context.as_deref(),
                );

                let mut messages = Vec::new();
                messages.push(ChatMessage { role: "system".to_string(), content: system_msg });
                messages.extend(context);

               // 6. Inference (Think)
                let tool_schema = skills.to_tool_schema();
                // Send "typing..." action
                let _ = bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing).await;

                let response = match engine.infer(messages.clone(), Some(tool_schema), soul.temperature, soul.max_tokens).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!("Inference error: {}", e);
                        bot.send_message(msg.chat.id, "Checking my connection circuits...").await?;
                        return Ok(());
                    }
                };

                // 7. Tool Execution (Act)
                let tool_calls = tools::parse_tool_calls(&response);
                let final_response = if !tool_calls.is_empty() {
                    info!("Executing {} tool(s)...", tool_calls.len());
                    let _ = bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing).await;

                    let tool_output = tools::execute_tool_calls(&tool_calls, &skills, mcp.as_deref()).await;
                    
                    messages.push(ChatMessage { role: "assistant".to_string(), content: response.clone() });
                    messages.push(ChatMessage { role: "system".to_string(), content: format!("Tool execution results:\n{}", tool_output) });
                    
                    match engine.infer(messages.clone(), None, soul.temperature, soul.max_tokens).await {
                        Ok(resp) => resp,
                        Err(e) => {
                             error!("Re-inference error: {}", e);
                             format!("Tool error: {}", e)
                        }
                    }
                } else {
                    response
                };

                // 8. Store and Send Response
                {
                     let db = db.lock().await;
                     if let Err(e) = db.append_message(&user_id, "assistant", &final_response) {
                         error!("Failed to store assistant response: {}", e);
                     }
                }
                
                // Format for Telegram (strip markdown if needed or use parse_mode)
                // Ultraclaw's formatter.rs handles basic markdown stripping/adaptation.
                // But teloxide supports MarkdownV2.
                // We should let formatter.rs do the heavy lifting to avoid errors.
                // Or try to send as MarkdownV2 and fallback to text on error.
                // For safety and MVP, just send text.
                
                // Standard send
                if let Err(e) = bot.send_message(msg.chat.id, final_response).await {
                    error!("Error sending response to Telegram: {:?}", e);
                }

                Ok(())
            }
        })
        .await;

        Ok(())
    }
}
