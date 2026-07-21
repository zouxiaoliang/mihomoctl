use tui::widgets::{Paragraph, Widget};

use crate::{
    components::{FooterWidget, ProxyGroupFocusStatus, ProxyTree},
    get_block, get_focused_block,
};

#[derive(Clone, Debug)]
pub struct ProxyTreeWidget<'a> {
    state: &'a ProxyTree<'a>,
}

impl<'a> ProxyTreeWidget<'a> {
    pub fn new(state: &'a ProxyTree<'a>) -> Self {
        Self { state }
    }

    /// Index (in visible order) of the first group to render in the collapsed
    /// view. The window is held steady and only scrolls once the focused group
    /// would no longer fit at the bottom of the viewport, so navigating within a
    /// screenful never scrolls. The chosen start is persisted on the tree so the
    /// hysteresis carries across renders.
    fn collapsed_window_start(
        &self,
        visible: &[usize],
        cursor: usize,
        width: usize,
        viewport: usize,
    ) -> usize {
        let heights = visible
            .iter()
            .map(|&group_index| {
                self.state.groups[group_index]
                    .collapsed_height(width, self.state.visible_member_indices(group_index))
            })
            .collect::<Vec<_>>();

        // Clamp the remembered anchor to the cursor: pulling the window up when
        // the cursor moved above it, and staying put otherwise.
        let mut start = self.state.window_start.get().min(cursor);

        // Advance the window down only as far as needed for the focused group to
        // fit within the viewport.
        while start < cursor
            && heights
                .get(start..=cursor)
                .map(|window| window.iter().sum::<usize>())
                .unwrap_or(0)
                > viewport
        {
            start += 1;
        }

        self.state.window_start.set(start);
        start
    }

    /// Index of the first member row to render for the expanded group. Like the
    /// collapsed window it holds steady and only scrolls once the focused node
    /// would leave the `viewport` (member rows fit on screen), so navigating
    /// nodes within a screenful never scrolls. Persisted across renders.
    fn member_window_start(&self, viewport: usize) -> usize {
        let group = &self.state.groups[self.state.cursor()];
        let cursor = self
            .state
            .visible_member_indices(self.state.cursor())
            .and_then(|indices| indices.iter().position(|index| *index == group.cursor))
            .unwrap_or(group.cursor);

        let mut start = self.state.member_window_start.get();
        if cursor < start {
            start = cursor;
        } else if viewport > 0 && cursor >= start + viewport {
            start = cursor + 1 - viewport;
        }

        self.state.member_window_start.set(start);
        start
    }
}

impl<'a> Widget for ProxyTreeWidget<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let block = if self.state.expanded {
            get_focused_block("Proxies")
        } else {
            get_block("Proxies")
        };
        let inner = block.inner(area);
        block.render(area, buf);

        let cursor = self.state.visible_cursor_pos();
        let visible = self.state.visible_group_indices();

        let (skip, member_skip) = if self.state.expanded {
            // Expanded view shows a single group's members, anchored at the top.
            // The header takes one row, the rest is available for member rows.
            let member_viewport = (inner.height as usize).saturating_sub(1);
            (cursor, self.member_window_start(member_viewport))
        } else {
            let skip = self.collapsed_window_start(
                &visible,
                cursor,
                area.width as usize,
                inner.height as usize,
            );
            (skip, 0)
        };

        let text = visible
            .into_iter()
            .skip(skip)
            .enumerate()
            .map(|(i, group_index)| {
                let group = &self.state.groups[group_index];
                let status = match (self.state.expanded, cursor == i + skip) {
                    (true, true) => ProxyGroupFocusStatus::Expanded,
                    (false, true) => ProxyGroupFocusStatus::Focused,
                    _ => ProxyGroupFocusStatus::None,
                };
                let member_skip = if matches!(status, ProxyGroupFocusStatus::Expanded) {
                    member_skip
                } else {
                    0
                };
                group.get_filtered_widget(
                    area.width as usize,
                    status,
                    self.state.visible_member_indices(group_index),
                    member_skip,
                )
            })
            .reduce(|mut a, b| {
                a.extend(b);
                a
            })
            .unwrap_or_default()
            .into_iter()
            .take(inner.height as usize)
            .collect::<Vec<_>>();

        Paragraph::new(text).render(inner, buf);
        FooterWidget::new(&self.state.footer).render(area, buf);
    }
}
