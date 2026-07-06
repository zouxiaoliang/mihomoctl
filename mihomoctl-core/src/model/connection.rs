use serde::{Deserialize, Serialize};

use crate::model::{RuleType, TimeType};

fn null_connections_default<'de, D>(deserializer: D) -> Result<Vec<Connection>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<Connection>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(rename = "type")]
    pub connection_type: String,

    #[serde(rename = "sourceIP")]
    pub source_ip: String,
    pub source_port: String,

    #[serde(rename = "destinationIP")]
    pub destination_ip: String,
    pub destination_port: String,
    pub host: String,
    pub network: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub upload: u64,
    pub download: u64,
    pub metadata: Metadata,
    pub rule: RuleType,
    pub rule_payload: String,
    pub start: TimeType,
    pub chains: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct Connections {
    #[serde(default, deserialize_with = "null_connections_default")]
    pub connections: Vec<Connection>,
    pub download_total: u64,
    pub upload_total: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnectionWithSpeed {
    pub connection: Connection,
    pub upload: Option<u64>,
    pub download: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnectionsWithSpeed {
    pub connections: Vec<ConnectionWithSpeed>,
    pub download_total: u64,
    pub upload_total: u64,
}

#[cfg(feature = "deserialize")]
mod deserialize {
    use chrono::Utc;

    use crate::model::{Connection, ConnectionWithSpeed, Connections, ConnectionsWithSpeed};

    impl Connection {
        pub fn up_speed(&self) -> Option<u64> {
            let elapsed = (Utc::now() - self.start).num_seconds();
            if elapsed <= 0 {
                None
            } else {
                Some(self.upload / elapsed as u64)
            }
        }

        pub fn down_speed(&self) -> Option<u64> {
            let elapsed = (Utc::now() - self.start).num_seconds();
            if elapsed <= 0 {
                None
            } else {
                Some(self.download / elapsed as u64)
            }
        }
    }

    impl From<Connections> for ConnectionsWithSpeed {
        fn from(val: Connections) -> Self {
            Self {
                connections: val.connections.into_iter().map(Into::into).collect(),
                download_total: val.download_total,
                upload_total: val.upload_total,
            }
        }
    }

    impl From<ConnectionsWithSpeed> for Connections {
        fn from(val: ConnectionsWithSpeed) -> Self {
            Self {
                connections: val.connections.into_iter().map(Into::into).collect(),
                download_total: val.download_total,
                upload_total: val.upload_total,
            }
        }
    }

    impl From<Connection> for ConnectionWithSpeed {
        fn from(val: Connection) -> Self {
            let elapsed = (Utc::now() - val.start).num_seconds();
            if elapsed <= 0 {
                Self {
                    connection: val,
                    upload: None,
                    download: None,
                }
            } else {
                Self {
                    download: Some(val.download / elapsed as u64),
                    upload: Some(val.upload / elapsed as u64),
                    connection: val,
                }
            }
        }
    }

    impl From<ConnectionWithSpeed> for Connection {
        fn from(val: ConnectionWithSpeed) -> Self {
            val.connection
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Connections;

    #[test]
    fn connections_accepts_null_list_as_empty_list() {
        let connections: Connections = serde_json::from_str(
            r#"{
                "downloadTotal": 6062698133,
                "uploadTotal": 412273263,
                "connections": null,
                "memory": 37298176
            }"#,
        )
        .unwrap();

        assert!(connections.connections.is_empty());
        assert_eq!(connections.download_total, 6062698133);
        assert_eq!(connections.upload_total, 412273263);
    }
}
