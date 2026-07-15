use tui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{List, ListItem, Widget},
};

use crate::{get_block, ConfigState};

#[derive(Clone, Debug)]
pub struct ConfigPage<'a> {
    state: &'a ConfigState,
}

impl<'a> ConfigPage<'a> {
    pub fn new(state: &'a ConfigState) -> Self {
        Self { state }
    }
}

enum ConfigListItem<'a> {
    Title(&'a str),
    Item { label: &'a str, content: String },
    Separator,
    Empty,
}

impl<'a> ConfigListItem<'a> {
    pub fn title(title: &'a str) -> impl Iterator<Item = ConfigListItem<'a>> {
        [
            ConfigListItem::Empty,
            ConfigListItem::Title(title),
            ConfigListItem::Separator,
        ]
        .into_iter()
    }

    pub fn into_list_item(self, width: u16) -> ListItem<'a> {
        match self {
            ConfigListItem::Title(title) => ListItem::new(title).style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            ConfigListItem::Item { label, content } => {
                let text = if label == "Server" {
                    format_wrapped_value(label, &content, width as usize)
                } else {
                    format!(
                        "{:<15}{:>right$}",
                        label,
                        content,
                        right = (width - 15) as usize
                    )
                };
                ListItem::new(text).style(Style::default().fg(Color::White))
            }
            ConfigListItem::Separator => {
                ListItem::new(format!("{:-<width$}", "", width = width as usize))
            }
            ConfigListItem::Empty => {
                ListItem::new(format!("{:width$}", "", width = width as usize))
            }
        }
    }
}

fn format_wrapped_value(label: &str, value: &str, width: usize) -> String {
    let prefix = format!("{label} ");
    let indent = prefix.chars().count();
    let width = width.max(indent + 1);
    let mut line_len = indent;
    let mut output = prefix;

    for ch in value.chars() {
        if line_len >= width {
            output.push('\n');
            output.extend(std::iter::repeat(' ').take(indent));
            line_len = indent;
        }
        output.push(ch);
        line_len += 1;
    }

    output
}

impl<'a> Widget for ConfigPage<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let width = area.width.saturating_sub(4).max(10);
        let block = get_block("Config");
        let list = ConfigListItem::title("Clash")
            .chain(self.state.clash_list().map(|x| ConfigListItem::Item {
                label: x.0,
                content: x.1,
            }))
            .chain(ConfigListItem::title("Mihomoctl"))
            .chain(self.state.mihomoctl_list().map(|x| ConfigListItem::Item {
                label: x.0,
                content: x.1,
            }))
            .map(|x| x.into_list_item(width))
            .collect::<Vec<_>>();
        let inner = block.inner(area);
        let inner = Rect {
            x: inner.x + 1,
            width: inner.width - 1,
            ..inner
        };
        block.render(area, buf);
        List::new(list).render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::format_wrapped_value;

    #[test]
    fn server_url_uses_the_full_config_panel_width() {
        assert_eq!(
            format_wrapped_value("Server", "http://192.168.8.1:9090", 31),
            "Server http://192.168.8.1:9090"
        );
    }

    #[test]
    fn long_server_url_wraps_instead_of_being_truncated() {
        let rendered = format_wrapped_value("Server", "http://example.com:9090/controller", 20);

        assert_eq!(rendered.replace('\n', "").replace(' ', ""), "Serverhttp://example.com:9090/controller");
        assert!(rendered.contains('\n'));
    }
}
