use tui::{
    style::Style,
    text::{Span, Spans},
    widgets::Widget,
};

use crate::{
    mihomoctl::model::Log,
    components::{MovableList, MovableListItem},
    define_widget, AsColor,
};

impl<'a> MovableListItem<'a> for Log {
    fn to_spans(&self) -> Spans<'a> {
        let color = self.log_type.clone().as_color();
        let payload = format_log_payload(&self.payload);
        Spans::from(vec![
            Span::styled(
                format!("{:<5}", self.log_type.to_string().to_uppercase()),
                Style::default().fg(color),
            ),
            Span::raw(" "),
            Span::raw(payload),
        ])
    }
}

define_widget!(LogPage);

impl<'a> Widget for LogPage<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let list = MovableList::new("Logs", &self.state.log_state);
        list.render(area, buf);
    }
}

fn format_log_payload(payload: &str) -> String {
    let Ok(value) = mihomoctl_core::serde_json::from_str::<mihomoctl_core::serde_json::Value>(payload)
    else {
        return payload.to_owned();
    };
    let Some(object) = value.as_object() else {
        return payload.to_owned();
    };

    let mut parts = Vec::new();
    if let Some(message) = object.get("message").and_then(|value| value.as_str()) {
        parts.push(message.to_owned());
    }

    if let Some(fields) = object.get("fields").and_then(|value| value.as_object()) {
        for (key, value) in fields {
            let value = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            parts.push(format!("{key}={value}"));
        }
    }

    if parts.is_empty() {
        payload.to_owned()
    } else {
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use mihomoctl_core::model::{Level, Log};

    use super::*;

    fn content(log: &Log) -> String {
        log.to_spans()
            .0
            .into_iter()
            .map(|span| span.content.into_owned())
            .collect::<String>()
    }

    #[test]
    fn structured_log_payload_is_rendered_readably() {
        let log = Log {
            log_type: Level::Info,
            payload: r#"{"time":"2026-07-03T01:02:03Z","level":"info","message":"proxy connected","fields":{"proxy":"DIRECT","rule":"MATCH"}}"#.to_owned(),
        };

        let rendered = content(&log);
        assert!(rendered.contains("proxy connected"));
        assert!(rendered.contains("proxy=DIRECT"));
        assert!(rendered.contains("rule=MATCH"));
        assert!(!rendered.contains(r#""fields""#));
    }
}
