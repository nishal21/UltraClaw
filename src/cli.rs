use crate::connector::Connector;
use crate::db::{ChatMessage, ConversationDb};
use crate::inference::InferenceEngine;
use crate::mcp::McpClient;
use crate::memory::MemoryStore;
use crate::session::SessionManager;
use crate::skill::SkillRegistry;
use crate::soul::Soul;
use crate::tools;
use crate::config::Config;

use async_trait::async_trait;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

pub struct CliConnector;

impl CliConnector {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Connector for CliConnector {
    fn name(&self) -> &str {
        "CLI"
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
        println!("╔══════════════════════════════════════════════════╗");
        println!("║             ULTRACLAW CLI MODE                   ║");
        println!("║      Talk to the agent directly (No Matrix)      ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!("Type 'exit' or 'quit' to stop.\n");

        let room_id = "cli_user"; // Fixed ID for CLI session
        let platform = crate::formatter::Platform::Unknown; 

        // Pre-create session
        {
            let mut sessions = sessions.lock().await;
            sessions.get_or_create(room_id, platform);
        }

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            print!("> ");
            if let Err(e) = stdout.flush() {
                 error!("Failed to flush stdout: {}", e);
                 break;
            }

            let mut input = String::new();
            if stdin.read_line(&mut input).is_err() {
                break;
            }

            let input = input.trim();
            if input.is_empty() {
                continue;
            }

            if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                break;
            }

            // --- PROCESS INPUT ---
            
            // 1. Store user message
            {
                let db = db.lock().await;
                if let Err(e) = db.append_message(room_id, "user", input) {
                    error!("Failed to store user message: {}", e);
                }
            }

            // 2. Load Context
            let context = {
                let db = db.lock().await;
                db.get_context(room_id, config.context_window_size).unwrap_or_default()
            };

            // 3. Recall Memory
            let memory_context = {
                let mem = memory.lock().await;
                mem.summarize_for_context(room_id, 200).unwrap_or(None)
            };
            
            // 4. Build System Prompt
            let session_context = {
                 let sessions = sessions.lock().await;
                 sessions.get_session_context(room_id)
            };

            let system_msg = soul.build_system_message(
                Some("User is strictly strictly strictly strictly using a Command Line Interface (CLI). Format answers as plain text."),
                session_context.as_deref(),
                memory_context.as_deref(),
            );

            let mut messages = Vec::new();
            messages.push(ChatMessage { role: "system".to_string(), content: system_msg });
            messages.extend(context);

            // 5. Infer
            info!("Thinking...");
            let tool_schema = skills.to_tool_schema();
            let response = match engine.infer(messages.clone(), Some(tool_schema), soul.temperature, soul.max_tokens).await {
                Ok(resp) => resp,
                Err(e) => {
                    println!("Error: {}", e);
                    continue;
                }
            };

            // 6. Handle Tools
            let tool_calls = tools::parse_tool_calls(&response);
            let final_response = if !tool_calls.is_empty() {
                println!("Executing {} tool(s)...", tool_calls.len());
                let tool_output = tools::execute_tool_calls(&tool_calls, &skills, mcp.as_deref()).await;
                
                // Re-infer
                messages.push(ChatMessage { role: "assistant".to_string(), content: response.clone() });
                messages.push(ChatMessage { role: "system".to_string(), content: format!("Tool execution results:\n{}", tool_output) });
                
                match engine.infer(messages.clone(), None, soul.temperature, soul.max_tokens).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        println!("Error during re-inference: {}", e);
                        response
                    }
                }
            } else {
                response
            };

            // 7. Store and Print Response
            {
                 let db = db.lock().await;
                 if let Err(e) = db.append_message(room_id, "assistant", &final_response) {
                     error!("Failed to store assistant response: {}", e);
                 }
            }

            println!("\n{}\n", final_response);
        }
        
        Ok(())
    }
}

