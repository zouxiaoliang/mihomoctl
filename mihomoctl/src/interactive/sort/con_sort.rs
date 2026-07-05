use mihomoctl_core::model::ConnectionWithSpeed;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;

use crate::{EndlessSelf, OrderBy, SortMethod, SortOrder};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    SmartDefault,
    strum::EnumString,
    strum::Display,
    strum::EnumVariantNames,
    strum::EnumIter,
)]
#[strum(ascii_case_insensitive)]
#[serde(rename_all = "lowercase")]
enum ConSortBy {
    Host,
    Down,
    Up,
    DownSpeed,
    UpSpeed,
    Chains,
    Rule,
    #[default]
    Time,
    Src,
    Dest,
    Type,
}

impl SortMethod<ConnectionWithSpeed> for ConSortBy {
    fn sort_fn(&self, a: &ConnectionWithSpeed, b: &ConnectionWithSpeed) -> std::cmp::Ordering {
        let a_conn = &a.connection;
        let b_conn = &b.connection;
        let a_meta = &a_conn.metadata;
        let b_meta = &b_conn.metadata;

        match self {
            Self::Host => a_meta
                .host
                .cmp(&b_meta.host)
                .then_with(|| a_meta.destination_port.cmp(&b_meta.destination_port)),
            Self::Down => a_conn.download.cmp(&b_conn.download),
            Self::Up => a_conn.upload.cmp(&b_conn.upload),
            Self::DownSpeed => a.download.cmp(&b.download),
            Self::UpSpeed => a.upload.cmp(&b.upload),
            Self::Chains => a_conn.chains.cmp(&b_conn.chains),
            Self::Rule => a_conn
                .rule
                .cmp(&b_conn.rule)
                .then_with(|| a_conn.rule_payload.cmp(&b_conn.rule_payload)),
            Self::Time => a_conn.start.cmp(&b_conn.start),
            Self::Src => a_meta
                .source_ip
                .cmp(&b_meta.source_ip)
                .then_with(|| a_meta.source_port.cmp(&b_meta.source_port)),
            Self::Dest => a_meta
                .destination_ip
                .cmp(&b_meta.destination_ip)
                .then_with(|| a_meta.host.cmp(&b_meta.host))
                .then_with(|| a_meta.destination_port.cmp(&b_meta.destination_port)),
            Self::Type => a_meta
                .connection_type
                .cmp(&b_meta.connection_type)
                .then_with(|| a_meta.network.cmp(&b_meta.network)),
        }
    }
}

impl EndlessSelf for ConSortBy {
    fn next_self(&mut self) {
        *self = match self {
            Self::Host => Self::Down,
            Self::Down => Self::Up,
            Self::Up => Self::DownSpeed,
            Self::DownSpeed => Self::UpSpeed,
            Self::UpSpeed => Self::Chains,
            Self::Chains => Self::Rule,
            Self::Rule => Self::Time,
            Self::Time => Self::Src,
            Self::Src => Self::Dest,
            Self::Dest => Self::Type,
            Self::Type => Self::Host,
        }
    }

    fn prev_self(&mut self) {
        *self = match self {
            Self::Host => Self::Type,
            Self::Down => Self::Host,
            Self::Up => Self::Down,
            Self::DownSpeed => Self::Up,
            Self::UpSpeed => Self::DownSpeed,
            Self::Chains => Self::UpSpeed,
            Self::Rule => Self::Chains,
            Self::Time => Self::Rule,
            Self::Src => Self::Time,
            Self::Dest => Self::Src,
            Self::Type => Self::Dest,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub struct ConSort {
    by: ConSortBy,
    order: SortOrder,
}

impl ConSort {
    const VARIANTS: [Self; 22] = {
        use ConSortBy::*;
        use SortOrder::*;

        [
            Self::new(Time, Descendant),
            Self::new(Host, Ascendant),
            Self::new(Host, Descendant),
            Self::new(Down, Descendant),
            Self::new(Down, Ascendant),
            Self::new(Up, Descendant),
            Self::new(Up, Ascendant),
            Self::new(DownSpeed, Descendant),
            Self::new(DownSpeed, Ascendant),
            Self::new(UpSpeed, Descendant),
            Self::new(UpSpeed, Ascendant),
            Self::new(Chains, Ascendant),
            Self::new(Chains, Descendant),
            Self::new(Rule, Ascendant),
            Self::new(Rule, Descendant),
            Self::new(Src, Ascendant),
            Self::new(Src, Descendant),
            Self::new(Dest, Ascendant),
            Self::new(Dest, Descendant),
            Self::new(Type, Ascendant),
            Self::new(Type, Descendant),
            Self::new(Time, Ascendant),
        ]
    };

    #[inline]
    const fn new(by: ConSortBy, order: SortOrder) -> Self {
        Self { by, order }
    }

    #[inline]
    fn variants() -> &'static [Self] {
        &Self::VARIANTS
    }
}

impl EndlessSelf for ConSort {
    fn next_self(&mut self) {
        let variants = Self::variants();
        let index = variants
            .iter()
            .position(|variant| variant == self)
            .unwrap_or_default();
        *self = variants[(index + 1) % variants.len()];
    }

    fn prev_self(&mut self) {
        let variants = Self::variants();
        let index = variants
            .iter()
            .position(|variant| variant == self)
            .unwrap_or_default();
        *self = variants[(index + variants.len() - 1) % variants.len()];
    }
}

impl SortMethod<ConnectionWithSpeed> for ConSort {
    fn sort_fn(&self, a: &ConnectionWithSpeed, b: &ConnectionWithSpeed) -> std::cmp::Ordering {
        self.by.sort_fn(a, b).order_by(self.order)
    }
}

impl ToString for ConSort {
    fn to_string(&self) -> String {
        format!(
            "{} {}",
            self.by,
            match self.order {
                SortOrder::Ascendant => "▲",
                SortOrder::Descendant => "▼",
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use mihomoctl_core::model::{Connection, ConnectionWithSpeed, Metadata, RuleType};

    use super::*;

    fn connection(host: &str, download: u64, upload_speed: Option<u64>) -> ConnectionWithSpeed {
        ConnectionWithSpeed {
            download: Some(download / 10),
            upload: upload_speed,
            connection: Connection {
                id: host.to_owned(),
                upload: upload_speed.unwrap_or_default() * 10,
                download,
                metadata: Metadata {
                    connection_type: "tcp".to_owned(),
                    source_ip: "127.0.0.1".to_owned(),
                    source_port: "50000".to_owned(),
                    destination_ip: "1.1.1.1".to_owned(),
                    destination_port: "443".to_owned(),
                    host: host.to_owned(),
                    network: "tcp".to_owned(),
                },
                rule: RuleType::Match,
                rule_payload: String::new(),
                start: Utc::now(),
                chains: vec!["DIRECT".to_owned()],
            },
        }
    }

    #[test]
    fn connection_sort_orders_by_fields_and_cycles() {
        let alpha = connection("alpha.test", 100, Some(30));
        let beta = connection("beta.test", 200, Some(10));

        let by_host = ConSort::new(ConSortBy::Host, SortOrder::Ascendant);
        assert_eq!(
            by_host.sort_fn(&beta, &alpha),
            std::cmp::Ordering::Greater
        );

        let by_up_speed = ConSort::new(ConSortBy::UpSpeed, SortOrder::Descendant);
        assert_eq!(
            by_up_speed.sort_fn(&alpha, &beta),
            std::cmp::Ordering::Less
        );

        let mut sort = ConSort::default();
        assert_eq!(sort.to_string(), "Time ▼");
        sort.next_self();
        assert_eq!(sort.to_string(), "Host ▲");
        sort.prev_self();
        assert_eq!(sort.to_string(), "Time ▼");
    }
}
