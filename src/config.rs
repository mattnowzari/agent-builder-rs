#[derive(Debug, Clone)]
pub struct Config {
    pub kibana_url: Option<String>,
    pub api_key: Option<String>,
    pub space: Option<String>,
    pub agent_id: String,
    /// Allow self-signed certs / hostname mismatches (dev / local Kibana).
    pub insecure_tls: bool,
    /// Path to a theme YAML file. `None` means use defaults.
    pub theme_path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            kibana_url: None,
            api_key: None,
            space: None,
            agent_id: "elastic-ai-agent".to_string(),
            insecure_tls: false,
            theme_path: None,
        }
    }
}

impl Config {
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.kibana_url.as_deref().unwrap_or("").is_empty() {
            missing.push("KIBANA_URL (or ES_HOST)");
        }
        if self.api_key.as_deref().unwrap_or("").is_empty() {
            missing.push("API_KEY (or ES_API_KEY)");
        }
        missing
    }

    pub fn is_ready(&self) -> bool {
        self.missing().is_empty()
    }
}

pub fn load_from_env() -> Config {
    let _ = dotenvy::dotenv();

    let mut cfg = Config {
        kibana_url: env_first_nonempty(&[
            "KIBANA_URL",
            "ES_HOST",
            "ELASTICSEARCH_HOST",
            "ELASTIC_HOST",
        ]),
        api_key: env_first_nonempty(&["API_KEY", "ES_API_KEY"]),
        space: env_first_nonempty(&["KIBANA_SPACE", "SPACE"]),
        insecure_tls: env_bool(&["KIBANA_INSECURE_TLS", "INSECURE_TLS"], false),
        ..Config::default()
    };

    if let Ok(agent_id) = std::env::var("AGENT_ID") {
        let agent_id = agent_id.trim().to_string();
        if !agent_id.is_empty() {
            cfg.agent_id = agent_id;
        }
    }

    cfg.theme_path = env_first_nonempty(&["THEME", "TUI_THEME"]);

    cfg
}

fn env_first_nonempty(keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn env_bool(keys: &[&str], default: bool) -> bool {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            let v = v.trim().to_ascii_lowercase();
            if v.is_empty() {
                continue;
            }
            return matches!(v.as_str(), "1" | "true" | "yes" | "y" | "on");
        }
    }
    default
}
