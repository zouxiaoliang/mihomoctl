use tui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{List, ListItem, Paragraph, Widget},
};

use crate::{
    interactive::{EndlessSelf, Noop, SortMethod},
    spans_window_owned, tagged_footer, Wrap,
    ui::{
        components::{
            Footer, FooterItem, FooterWidget, MovableListItem, MovableListManage, MovableListState,
        },
        utils::{get_block, get_focused_block, get_text_style},
    },
};

#[derive(Clone, Debug)]
pub struct MovableList<'a, T, S = Noop>
where
    T: MovableListItem<'a>,
    S: Default,
{
    pub(super) title: String,
    pub(super) state: &'a MovableListState<'a, T, S>,
}

impl<'a, T, S> MovableList<'a, T, S>
where
    S: SortMethod<T> + EndlessSelf + Default + ToString,
    T: MovableListItem<'a>,
    MovableListState<'a, T, S>: MovableListManage,
{
    pub fn new<TITLE: Into<String>>(title: TITLE, state: &'a MovableListState<'a, T, S>) -> Self {
        Self {
            state,
            title: title.into(),
        }
    }

    fn render_footer(&self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let mut footer = Footer::default();
        let pos = self.state.current_pos();

        let sort_str = self.state.sort.to_string();

        footer.push_right(FooterItem::span(Span::styled(
            format!(" Ln {}, Col {} ", pos.y, pos.x),
            Style::default()
                .fg(if pos.hold { Color::Green } else { Color::Blue })
                .add_modifier(Modifier::REVERSED),
        )));

        if pos.hold {
            let style = Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::REVERSED);

            footer.push_left(FooterItem::span(Span::styled(" FREE ", style)));
            footer.push_left(FooterItem::span(Span::styled(" [^] ▲ ▼ ◀ ▶ Move ", style)));
            if self.state.can_pause() {
                if self.state.is_paused() {
                    let paused_style = Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::REVERSED);
                    footer.push_left(FooterItem::span(Span::styled(" PAUSED ", paused_style)));
                    footer.push_left(FooterItem::span(Span::styled(" p Resume ", style)));
                } else {
                    footer.push_left(FooterItem::span(Span::styled(" p Pause ", style)));
                }
                footer.push_left(FooterItem::span(Span::styled(" c Clear ", style)));
            }
            if !sort_str.is_empty() {
                footer.push_left(tagged_footer("Sort", style, sort_str).into());
            }
            if self.state.can_search() {
                if let Some(query) = self.state.search_query() {
                    let highlight = style.add_modifier(Modifier::BOLD);
                    footer.push_left(FooterItem::span(Span::styled(" SEARCH ", highlight)));
                    footer.push_left(FooterItem::span(Span::raw(query.to_owned())).wrapped());
                    footer.push_left(FooterItem::span(Span::styled(" Esc Cancel ", style)));
                } else {
                    footer.push_left(FooterItem::span(Span::styled(" / Search ", style)));
                }
            }
        } else {
            let style = Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::REVERSED);

            footer.push_left(FooterItem::span(Span::styled(" NORMAL ", style)));
            if self.state.can_pause() {
                if self.state.is_paused() {
                    let paused_style = Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::REVERSED);
                    footer.push_left(FooterItem::span(Span::styled(" PAUSED ", paused_style)));
                    footer.push_left(FooterItem::span(Span::styled(" p Resume ", style)));
                } else {
                    footer.push_left(FooterItem::span(Span::styled(" p Pause ", style)));
                }
                footer.push_left(FooterItem::span(Span::styled(" c Clear ", style)));
            }
            footer.push_left(FooterItem::span(Span::styled(
                " SPACE / [^] ▲ ▼ ◀ ▶ Move ",
                style,
            )));
            if !sort_str.is_empty() {
                footer.push_left(tagged_footer("Sort", style, sort_str).into());
            }
            if self.state.can_search() {
                if let Some(query) = self.state.search_query() {
                    let highlight = style.add_modifier(Modifier::BOLD);
                    footer.push_left(FooterItem::span(Span::styled(" SEARCH ", highlight)));
                    footer.push_left(FooterItem::span(Span::raw(query.to_owned())).wrapped());
                    footer.push_left(FooterItem::span(Span::styled(" Esc Cancel ", style)));
                } else {
                    footer.push_left(FooterItem::span(Span::styled(" / Search ", style)));
                }
            }
        }

        let widget = FooterWidget::new(&footer);
        widget.render(area, buf);
    }
}

impl<'a, T, S> Widget for MovableList<'a, T, S>
where
    S: SortMethod<T> + EndlessSelf + Default + ToString,
    T: MovableListItem<'a>,
    MovableListState<'a, T, S>: MovableListManage,
{
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let num = self.state.visible_len();

        let offset = self.state.offset;

        let block = if offset.hold {
            get_focused_block(&self.title)
        } else {
            get_block(&self.title)
        };
        let pad = self.state.padding;
        let inner = block.inner(area);
        let inner = if pad == 0 {
            inner
        } else {
            Rect {
                x: inner.x + pad,
                y: inner.y,
                width: inner.width.saturating_sub(pad * 2),
                height: inner.height,
            }
        };

        let header = self.state.header.clone();
        let list_inner = if header.is_some() {
            Rect {
                y: inner.y.saturating_add(1),
                height: inner.height.saturating_sub(1),
                ..inner
            }
        } else {
            inner
        };

        let height = list_inner.height as usize;

        // The cursor position in visible order
        let cursor = if offset.y + 1 > num {
            num.saturating_sub(1)
        } else {
            offset.y
        };

        // Scroll window with hysteresis: the cursor walks the page and the
        // window only follows once the cursor crosses its edges.
        let mut window_start = self.state.window_start.get();
        if cursor < window_start {
            window_start = cursor;
        } else if height > 0 && cursor >= window_start + height {
            window_start = cursor + 1 - height;
        }
        // Keep the window within bounds when the list shrinks
        window_start = window_start.min(num.saturating_sub(height.max(1)));
        self.state.window_start.set(window_start);

        let y_offset = window_start;

        let x_offset = offset.x;

        let index_width = num.to_string().len();
        let index_style = Style::default().fg(Color::DarkGray);

        let x_range = x_offset
            ..(x_offset
                .saturating_add(inner.width as usize)
                .saturating_sub(index_width));
        let with_index = self.state.with_index;
        let rev_index = self.state.reverse_index;

        // Get that portion of items
        let items = if num != 0 {
            let visible_indices = if self.state.reverse_items {
                (0..num).rev().collect::<Vec<_>>()
            } else {
                (0..num).collect::<Vec<_>>()
            };

            visible_indices
                .into_iter()
                .skip(y_offset)
                .take(height)
                .enumerate()
                .filter_map(|(i, visible_index)| {
                    let item_index = self.state.visible_item_index(visible_index)?;
                    let x = self.state.items.get(item_index)?;
                    let content = x.to_spans();
                    let x_width = content.width();
                    let content = spans_window_owned(content, &x_range);

                    let mut spans = if x_width != 0 && content.width() == 0 {
                        Span::raw("◀").into()
                    } else {
                        content
                    };

                    if with_index {
                        let cur_index = if rev_index {
                            num - i - y_offset
                        } else {
                            i + y_offset + 1
                        };
                        spans.0.insert(
                            0,
                            Span::styled(
                                format!("{:>width$} ", cur_index, width = index_width),
                                index_style,
                            ),
                        );
                    };
                    if i + y_offset == cursor {
                        for span in spans.0.iter_mut() {
                            span.style = span.style.add_modifier(Modifier::REVERSED);
                        }
                    }
                    Some(ListItem::new(spans))
                })
                .collect::<Vec<_>>()
        } else {
            vec![ListItem::new(Span::raw(
                self.state
                    .placeholder
                    .to_owned()
                    .unwrap_or_else(|| "Nothing's here yet".into()),
            ))]
        };

        block.render(area, buf);
        if let Some(header) = header {
            Paragraph::new(header).style(get_text_style()).render(
                Rect {
                    height: 1,
                    ..inner
                },
                buf,
            );
        }
        List::new(items)
            .style(get_text_style())
            .render(list_inner, buf);

        self.render_footer(area, buf);
    }
}

// #[test]
// fn test_movable_list() {
//     let items = &["Test1", "测试1", "[ABCD] 🇺🇲 测试 符号
// 106"].into_iter().map(|x| x.);     assert_eq!()
// }

#[cfg(test)]
mod tests {
    use tui::{buffer::Buffer, layout::Rect, text::{Span, Spans}, widgets::Widget};

    use super::*;

    fn row(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| buf.get(x, y).symbol.as_str())
            .collect::<String>()
    }

    fn reversed_row(buf: &Buffer, y: u16, width: u16) -> bool {
        (0..width).any(|x| {
            let cell = buf.get(x, y);
            !cell.symbol.trim().is_empty()
                && cell.modifier.contains(tui::style::Modifier::REVERSED)
        })
    }

    #[test]
    fn cursor_walks_down_the_page_before_the_window_scrolls() {
        let mut state: MovableListState<'_, String, Noop> =
            MovableListState::new((0..10).map(|i| format!("item-{i}")).collect());
        state.normal_order();

        // 6 rows total - 2 border rows = 4 visible list rows
        let area = Rect::new(0, 0, 30, 6);

        // Cursor within the first page: window stays at the top
        state.offset.y = 2;
        let mut buf = Buffer::empty(area);
        MovableList::new("List", &state).render(area, &mut buf);
        assert!(row(&buf, 1, 30).contains("item-0"));
        assert!(reversed_row(&buf, 3, 30)); // item-2 highlighted in place

        // Cursor beyond the page: window follows, cursor rides the bottom
        state.offset.y = 5;
        let mut buf = Buffer::empty(area);
        MovableList::new("List", &state).render(area, &mut buf);
        assert!(row(&buf, 1, 30).contains("item-2"));
        assert!(row(&buf, 4, 30).contains("item-5"));
        assert!(reversed_row(&buf, 4, 30)); // bottom row highlighted

        // Moving back up: window holds until the cursor hits its top edge
        state.offset.y = 3;
        let mut buf = Buffer::empty(area);
        MovableList::new("List", &state).render(area, &mut buf);
        assert!(row(&buf, 1, 30).contains("item-2"));
        assert!(reversed_row(&buf, 2, 30)); // item-3 highlighted in place

        state.offset.y = 1;
        let mut buf = Buffer::empty(area);
        MovableList::new("List", &state).render(area, &mut buf);
        assert!(row(&buf, 1, 30).contains("item-1"));
        assert!(reversed_row(&buf, 1, 30)); // window scrolled up to the cursor
    }

    #[test]
    fn movable_list_renders_fixed_header_above_items() {
        let mut state: MovableListState<'_, String, Noop> =
            MovableListState::new(vec!["first row".to_owned()]);
        state.header(Spans::from(Span::raw("HEADER")));

        let area = Rect::new(0, 0, 30, 6);
        let mut buf = Buffer::empty(area);
        MovableList::new("List", &state).render(area, &mut buf);

        assert!(row(&buf, 1, 30).contains("HEADER"));
        assert!(row(&buf, 2, 30).contains("first row"));
    }

    #[test]
    fn movable_list_search_renders_only_matching_items() {
        let mut state: MovableListState<'_, String, Noop> = MovableListState::new(vec![
            "old needle".to_owned(),
            "middle row".to_owned(),
            "new needle".to_owned(),
        ]);

        state.begin_search();
        for ch in "needle".chars() {
            state.input_search_char(ch);
        }

        let area = Rect::new(0, 0, 30, 6);
        let mut buf = Buffer::empty(area);
        MovableList::new("List", &state).render(area, &mut buf);

        let rendered = (0..area.height)
            .map(|y| row(&buf, y, area.width))
            .collect::<String>();

        assert!(rendered.contains("old needle"));
        assert!(rendered.contains("new needle"));
        assert!(!rendered.contains("middle row"));
    }

    #[test]
    fn movable_list_footer_renders_pause_state() {
        let mut state: MovableListState<'_, String, Noop> =
            MovableListState::new(vec!["live row".to_owned()]);
        state.pausable();

        let area = Rect::new(0, 0, 80, 6);
        let mut buf = Buffer::empty(area);
        MovableList::new("List", &state).render(area, &mut buf);
        let rendered = (0..area.height)
            .map(|y| row(&buf, y, area.width))
            .collect::<String>();
        assert!(rendered.contains("p Pause"));

        state.toggle_paused();
        let mut buf = Buffer::empty(area);
        MovableList::new("List", &state).render(area, &mut buf);
        let rendered = (0..area.height)
            .map(|y| row(&buf, y, area.width))
            .collect::<String>();
        assert!(rendered.contains("PAUSED"));
        assert!(rendered.contains("p Resume"));
    }

    fn footer_text(state: &MovableListState<'_, String, Noop>) -> String {
        let area = Rect::new(0, 0, 80, 6);
        let mut buf = Buffer::empty(area);
        MovableList::new("List", state).render(area, &mut buf);
        (0..area.height)
            .map(|y| row(&buf, y, area.width))
            .collect::<String>()
    }

    #[test]
    fn searchable_list_hints_the_search_shortcut() {
        let mut state: MovableListState<'_, String, Noop> =
            MovableListState::new(vec!["row".to_owned()]);
        state.searchable();
        assert!(footer_text(&state).contains("/ Search"));

        // Once a search is active the footer swaps the hint for the query and a
        // cancel shortcut.
        state.begin_search();
        let rendered = footer_text(&state);
        assert!(rendered.contains("SEARCH"));
        assert!(rendered.contains("Esc Cancel"));
    }

    #[test]
    fn non_searchable_list_omits_the_search_hint() {
        let state: MovableListState<'_, String, Noop> =
            MovableListState::new(vec!["row".to_owned()]);
        assert!(!footer_text(&state).contains("/ Search"));
    }
}
