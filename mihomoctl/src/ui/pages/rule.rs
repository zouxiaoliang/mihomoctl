use std::net::IpAddr;

use mihomoctl_core::model::{Rule, RuleType, Rules};
use tui::{
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::Widget,
};

use crate::{
    components::{MovableList, MovableListItem, MovableListState},
    define_widget,
    interactive::RuleSort,
    AsColor,
};

define_widget!(RulePage);

impl<'a> Widget for RulePage<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        MovableList::new("Rules", &self.state.rule_state).render(area, buf);
    }
}

impl AsColor for RuleType {
    fn as_color(&self) -> tui::style::Color {
        match self {
            RuleType::Domain => Color::Green,
            RuleType::DomainSuffix => Color::Green,
            RuleType::DomainKeyword => Color::Green,
            RuleType::GeoIP => Color::Yellow,
            RuleType::IPCIDR => Color::Yellow,
            RuleType::SrcIPCIDR => Color::Yellow,
            RuleType::SrcPort => Color::Yellow,
            RuleType::DstPort => Color::Yellow,
            RuleType::Process => Color::Yellow,
            RuleType::Match => Color::Blue,
            RuleType::Direct => Color::Blue,
            RuleType::Reject => Color::Red,
            RuleType::Unknown => Color::DarkGray,
        }
    }
}

impl<'a> From<Rules> for MovableListState<'a, Rule, RuleSort> {
    fn from(val: Rules) -> Self {
        let mut state = Self::new_with_sort(val.rules, RuleSort::default());
        state.header(Spans::from(Span::styled(
            format!("{:16} {:37} {}", "TYPE", "PAYLOAD", "PROXY"),
            Style::default().fg(Color::DarkGray),
        )));
        state
    }
}

impl<'a> MovableListItem<'a> for Rule {
    fn to_spans(&self) -> Spans<'a> {
        let type_color = self.rule_type.as_color();
        let name_color = if self.proxy == "DIRECT" || self.proxy == "REJECT" {
            Color::DarkGray
        } else {
            Color::Yellow
        };
        let gray = Style::default().fg(Color::DarkGray);
        let r_type: &'static str = self.rule_type.into();
        let payload = if self.payload.is_empty() {
            "*"
        } else {
            &self.payload
        }
        .to_owned();
        let dash: String = "─".repeat(35_usize.saturating_sub(payload.len()) + 2) + " ";
        vec![
            Span::styled(format!("{:16}", r_type), Style::default().fg(type_color)),
            Span::styled(payload + " ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(dash, gray),
            Span::styled(self.proxy.to_owned(), Style::default().fg(name_color)),
        ]
        .into()
    }

    /// Rules match on their payload (the IP/CIDR or domain in the middle column)
    /// rather than the whole rendered line, so a query never hits the rule type
    /// or the target proxy. For IP-CIDR rules an address query additionally
    /// matches by network containment (e.g. `192.168.1.5` matches `192.168.0.0/16`),
    /// not just by characters.
    fn matches_query(&self, query: &str) -> bool {
        if self.payload.to_lowercase().contains(query) {
            return true;
        }

        if matches!(self.rule_type, RuleType::IPCIDR | RuleType::SrcIPCIDR) {
            if let Ok(addr) = query.parse::<IpAddr>() {
                return ip_in_cidr(addr, &self.payload);
            }
        }

        false
    }
}

/// Whether `addr` falls inside the `cidr` network (`a.b.c.d/n`, IPv4 or IPv6).
/// A missing prefix is treated as a full-length host match. Returns `false` for
/// unparseable networks or a mismatched address family.
fn ip_in_cidr(addr: IpAddr, cidr: &str) -> bool {
    let (network, prefix) = match cidr.split_once('/') {
        Some((network, prefix)) => (network.trim(), prefix.trim().parse::<u8>().ok()),
        None => (cidr.trim(), None),
    };

    match (addr, network.parse::<IpAddr>()) {
        (IpAddr::V4(addr), Ok(IpAddr::V4(network))) => {
            let prefix = prefix.unwrap_or(32);
            if prefix > 32 {
                return false;
            }
            let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
            (u32::from(addr) & mask) == (u32::from(network) & mask)
        }
        (IpAddr::V6(addr), Ok(IpAddr::V6(network))) => {
            let prefix = prefix.unwrap_or(128);
            if prefix > 128 {
                return false;
            }
            let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix) };
            (u128::from(addr) & mask) == (u128::from(network) & mask)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use mihomoctl_core::model::{Rule, RuleType};

    use crate::components::MovableListItem;

    fn rule(rule_type: RuleType, payload: &str) -> Rule {
        Rule {
            rule_type,
            payload: payload.to_owned(),
            proxy: "PROXY".to_owned(),
        }
    }

    #[test]
    fn ip_query_matches_containing_cidr_rule() {
        let rule = rule(RuleType::IPCIDR, "192.168.0.0/16");
        assert!(rule.matches_query("192.168.1.5"));
        assert!(!rule.matches_query("192.169.1.5"));
    }

    #[test]
    fn ip_query_matches_src_ipcidr_and_ipv6() {
        assert!(rule(RuleType::SrcIPCIDR, "10.0.0.0/8").matches_query("10.20.30.40"));
        assert!(rule(RuleType::IPCIDR, "2001:db8::/32").matches_query("2001:db8::1"));
        assert!(!rule(RuleType::IPCIDR, "2001:db8::/32").matches_query("2001:db9::1"));
    }

    #[test]
    fn substring_match_still_works_for_domains() {
        let rule = rule(RuleType::DomainSuffix, "google.com");
        assert!(rule.matches_query("google"));
        assert!(!rule.matches_query("192.168.1.1"));
    }

    #[test]
    fn containment_does_not_apply_to_non_ip_rule_types() {
        // A GeoIP payload is not a CIDR, so an address query must not match.
        assert!(!rule(RuleType::GeoIP, "CN").matches_query("1.2.3.4"));
    }
}
