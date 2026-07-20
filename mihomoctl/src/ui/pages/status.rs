use bytesize::ByteSize;
use tui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Paragraph, Widget},
};

use crate::ui::{
    components::{MovableListManage, Traffics},
    define_widget, get_block, get_text_style,
};

use super::config::ConfigPage;

define_widget!(StatusPage);

impl<'a> Widget for StatusPage<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let (info_area, config_area, version_area, traffic_area) = status_areas(area);

        let last_traffic = self
            .state
            .traffics
            .iter()
            .last()
            .map(|x| x.to_owned())
            .unwrap_or_default();

        let (up_avg, down_avg) = match self.state.start_time {
            time if time.elapsed().as_secs() == 0 => ("?".to_string(), "?".to_string()),
            time => {
                let elapsed = time.elapsed().as_secs();
                // Use the cumulative total (not `traffics`, which is trimmed) so
                // the average stays correct over long sessions.
                let (up_all, down_all) = self.state.traffic_total;

                (
                    ByteSize(up_all / elapsed).to_string_as(true) + "/s",
                    ByteSize(down_all / elapsed).to_string_as(true),
                )
            }
        };

        let con_num = self.state.con_state.len().to_string();
        let (total_up, total_down) = self.state.con_size;
        let clash_ver = self
            .state
            .version
            .to_owned()
            .map_or_else(|| "?".to_owned(), |v| v.version.to_string());

        let info = [
            ("⇉ Connections", con_num.as_str()),
            ("◎ Memory", &format_memory(self.state.memory.as_ref())),
            (
                "▲ Upload",
                &(ByteSize(last_traffic.up).to_string_as(true) + "/s"),
            ),
            (
                "▼ Download",
                &(ByteSize(last_traffic.down).to_string_as(true) + "/s"),
            ),
            ("▲ Avg.", &up_avg),
            ("▼ Avg.", &down_avg),
            (
                "▲ Max",
                &(ByteSize(self.state.max_traffic.up).to_string_as(true) + "/s"),
            ),
            (
                "▼ Max",
                &(ByteSize(self.state.max_traffic.down).to_string_as(true) + "/s"),
            ),
            ("▲ Total", &ByteSize(total_up).to_string_as(true)),
            ("▼ Total", &ByteSize(total_down).to_string_as(true)),
        ];

        let info_str = info
            .into_iter()
            .map(|(title, content)| format!(" {:<13}{:>18} ", title, content))
            .fold(String::with_capacity(340), |mut a, b| {
                a.push_str(&b);
                a.push('\n');
                a
            });

        Paragraph::new(info_str)
            .block(get_block("Info"))
            .style(get_text_style())
            .render(info_area, buf);

        ConfigPage::new(&self.state.config_state).render(config_area, buf);

        let versions = format!(
            " {:<13}{:>18} \n {:<13}{:>18} ",
            "Clash Ver.",
            clash_ver,
            "Mihomoctl Ver.",
            env!("CARGO_PKG_VERSION")
        );
        Paragraph::new(versions)
            .block(get_block("Version"))
            .style(get_text_style())
            .render(version_area, buf);

        let traffic = Traffics::new(self.state);
        traffic.render(traffic_area, buf)
    }
}

fn status_areas(area: Rect) -> (Rect, Rect, Rect, Rect) {
    let columns = Layout::default()
        .constraints([Constraint::Length(35), Constraint::Min(0)])
        .direction(Direction::Horizontal)
        .split(area);
    let sections = Layout::default()
        .constraints([
            Constraint::Length(12),
            Constraint::Min(0),
            Constraint::Length(4),
        ])
        .direction(Direction::Vertical)
        .split(columns[0]);

    (sections[0], sections[1], sections[2], columns[1])
}

fn format_memory(memory: Option<&mihomoctl_core::serde_json::Value>) -> String {
    let Some(memory) = memory else {
        return "?".to_owned();
    };

    let inuse = memory
        .get("inuse")
        .or_else(|| memory.get("inUse"))
        .and_then(|value| value.as_u64());

    let oslimit = memory
        .get("oslimit")
        .or_else(|| memory.get("osLimit"))
        .and_then(|value| value.as_u64());

    match (inuse, oslimit) {
        (Some(inuse), Some(oslimit)) if oslimit > 0 => {
            format!(
                "{} / {}",
                ByteSize(inuse).to_string_as(true),
                ByteSize(oslimit).to_string_as(true)
            )
        }
        (Some(inuse), _) => ByteSize(inuse).to_string_as(true),
        _ => memory.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use mihomoctl_core::serde_json::json;
    use tui::layout::Rect;

    use super::{format_memory, status_areas};

    #[test]
    fn memory_values_are_already_reported_in_bytes() {
        let memory = json!({
            "inuse": 20 * 1024 * 1024,
            "oslimit": 256 * 1024 * 1024
        });

        assert_eq!(format_memory(Some(&memory)), "20.0 MiB / 256.0 MiB");
    }

    #[test]
    fn zero_os_limit_is_not_displayed_as_a_real_limit() {
        let memory = json!({ "inUse": 20 * 1024 * 1024, "osLimit": 0 });

        assert_eq!(format_memory(Some(&memory)), "20.0 MiB");
    }

    #[test]
    fn status_sidebar_is_split_into_three_panels() {
        let (info, config, versions, traffic) = status_areas(Rect::new(0, 0, 120, 30));

        assert_eq!(info, Rect::new(0, 0, 35, 12));
        assert_eq!(config, Rect::new(0, 12, 35, 14));
        assert_eq!(versions, Rect::new(0, 26, 35, 4));
        assert_eq!(traffic, Rect::new(35, 0, 85, 30));
    }
}
