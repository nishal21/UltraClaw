pub struct AuthManager {
    users: std::collections::HashMap<String, String>, // username -> hashed_pw
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthManager {
    pub fn new() -> Self {
        let mut manager = Self {
            users: std::collections::HashMap::new(),
        };
        // Default admin
        manager.users.insert("admin".into(), "admin_hash_placeholder".into());
        manager
    }

    pub fn login(&self, user: &str, _pass: &str) -> Result<String, String> {
        if self.users.contains_key(user) {
            Ok(format!("session_token_for_{}", user))
        } else {
            Err("Invalid user".into())
        }
    }

    pub fn validate_token(&self, token: &str) -> bool {
        token.starts_with("session_token_for_")
    }
}

pub struct QuotaManager {
    daily_limit: u32,
    current_usage: u32,
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self::new()
    }
}

impl QuotaManager {
    pub fn new() -> Self {
        Self {
            daily_limit: 1000,
            current_usage: 0,
        }
    }

    pub fn consume(&mut self, tokens: u32) -> Result<(), String> {
        if self.current_usage + tokens > self.daily_limit {
            Err("Quota exceeded".into())
        } else {
            self.current_usage += tokens;
            Ok(())
        }
    }
}
