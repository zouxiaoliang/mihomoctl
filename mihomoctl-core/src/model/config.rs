use serde::{Deserialize, Serialize};

use super::{Level, Mode};

fn null_vec_default<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    pub port: u64,
    pub socks_port: u64,
    pub redir_port: u64,
    pub tproxy_port: u64,
    pub mixed_port: u64,
    pub allow_lan: bool,
    pub ipv6: bool,
    pub mode: Mode,
    pub log_level: Level,
    pub bind_address: String,
    #[serde(default, deserialize_with = "null_vec_default")]
    pub authentication: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn config_accepts_null_authentication_as_empty_list() {
        let config: Config = serde_json::from_str(
            r#"{
                "port": 7890,
                "socks-port": 7891,
                "redir-port": 7892,
                "tproxy-port": 7895,
                "mixed-port": 7893,
                "allow-lan": true,
                "ipv6": false,
                "mode": "rule",
                "log-level": "info",
                "bind-address": "*",
                "authentication": null
            }"#,
        )
        .unwrap();

        assert!(config.authentication.is_empty());
    }
}
