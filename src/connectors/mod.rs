// ============================================================================
// ULTRACLAW — Connectors Module
// ============================================================================
// Exports platform-specific connector implementations.

#[cfg(feature = "discord")]
pub mod discord;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "webhook")]
pub mod webhook;
pub mod massive_channels;

pub mod mobile;
pub mod firmware;

pub mod android_client;
pub mod swabble_mac;
pub mod phone_control;

#[cfg(feature = "slack")] pub mod slack;
#[cfg(feature = "whatsapp")] pub mod whatsapp;
#[cfg(feature = "teams")] pub mod teams;
#[cfg(feature = "mattermost")] pub mod mattermost;
#[cfg(feature = "googlechat")] pub mod googlechat;
#[cfg(feature = "imessage")] pub mod imessage;
#[cfg(feature = "signal")] pub mod signal;
#[cfg(feature = "wechat")] pub mod wechat;
#[cfg(feature = "dingtalk")] pub mod dingtalk;
#[cfg(feature = "feishu")] pub mod feishu;
#[cfg(feature = "wecom")] pub mod wecom;
#[cfg(feature = "qq")] pub mod qq;
#[cfg(feature = "line")] pub mod line;
#[cfg(feature = "twitch")] pub mod twitch;
#[cfg(feature = "nostr")] pub mod nostr;
#[cfg(feature = "irc")] pub mod irc;
#[cfg(feature = "nextcloud")] pub mod nextcloud;
#[cfg(feature = "synology")] pub mod synology;

