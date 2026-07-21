use std::{cmp::Ordering, collections::HashMap, fmt::Debug, marker::PhantomData};

use mihomoctl_core::model::Proxies;
use crossterm::event::KeyCode;
use tui::{
    style::{Color, Modifier, Style},
    text::Span,
};

use crate::{
    components::{Footer, FooterItem, MovableListManage, ProxyGroup, ProxyItem},
    interactive::{EndlessSelf, ProxySort, Sortable},
    ui::{help_footer, tagged_footer, Action, Coord, ListEvent, Wrap},
};

// - [X] Right & Enter can be used to apply selection
// - [X] Esc for exist expand mode
// - [X] T for test latency of current group
// - [X] S for switch between sorting strategies
// - [X] / for searching
#[derive(Clone, Debug, PartialEq)]
pub struct ProxyTree<'a> {
    pub(super) groups: Vec<ProxyGroup<'a>>,
    pub(super) expanded: bool,
    pub(super) cursor: usize,
    pub(super) testing: bool,
    search_query: Option<String>,
    visible_group_indices: Option<Vec<usize>>,
    visible_member_indices: HashMap<usize, Vec<usize>>,
    pub(super) footer: Footer<'a>,
    sort_method: ProxySort,
    /// First visible group (in visible order) of the last rendered collapsed
    /// window. Persisted across renders so the cursor walks the page and the
    /// view only scrolls once a group would fall off the bottom edge.
    pub(super) window_start: ScrollAnchor,
    /// Same idea for the member list of the expanded group: the first visible
    /// member row, so node navigation only scrolls once the focused node would
    /// leave the viewport. Reset when a group is (re-)expanded.
    pub(super) member_window_start: ScrollAnchor,
}

/// Render-time scroll anchor with interior mutability: widgets render from
/// `&self`, and the states are shared across threads, so it must be `Sync`.
#[derive(Debug, Default)]
pub(super) struct ScrollAnchor(std::sync::atomic::AtomicUsize);

impl ScrollAnchor {
    pub(super) fn get(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(super) fn set(&self, value: usize) {
        self.0.store(value, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Clone for ScrollAnchor {
    fn clone(&self) -> Self {
        Self(std::sync::atomic::AtomicUsize::new(self.get()))
    }
}

impl PartialEq for ScrollAnchor {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Eq for ScrollAnchor {}

impl<'a> Default for ProxyTree<'a> {
    fn default() -> Self {
        let mut ret = Self {
            groups: Default::default(),
            expanded: Default::default(),
            cursor: Default::default(),
            footer: Default::default(),
            testing: Default::default(),
            search_query: None,
            visible_group_indices: None,
            visible_member_indices: Default::default(),
            sort_method: Default::default(),
            window_start: Default::default(),
            member_window_start: Default::default(),
        };
        ret.update_footer();
        ret
    }
}

impl<'a> ProxyTree<'a> {
    #[inline]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    #[inline]
    pub fn current_group(&self) -> &ProxyGroup<'a> {
        &self.groups[self.cursor]
    }

    #[inline]
    pub fn is_testing(&self) -> bool {
        self.testing
    }

    /// Names of all testable (normal-typed) proxies across every group,
    /// deduplicated while keeping their first-seen order.
    pub fn unique_normal_members(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.groups
            .iter()
            .flat_map(|group| group.members())
            .filter(|member| member.proxy_type().is_normal())
            .filter(|member| seen.insert(member.name().to_owned()))
            .map(|member| member.name().to_owned())
            .collect()
    }

    #[inline]
    pub fn is_searching(&self) -> bool {
        self.search_query.is_some()
    }

    pub fn begin_search(&mut self) -> &mut Self {
        self.search_query = Some(String::new());
        self.visible_group_indices = None;
        self.visible_member_indices.clear();
        self.update_footer()
    }

    pub fn cancel_search(&mut self) -> &mut Self {
        self.search_query = None;
        self.visible_group_indices = None;
        self.visible_member_indices.clear();
        self.update_footer()
    }

    pub fn input_search_char(&mut self, ch: char) -> &mut Self {
        if let Some(query) = self.search_query.as_mut() {
            query.push(ch);
            self.apply_search();
        }
        self.update_footer()
    }

    pub fn backspace_search(&mut self) -> &mut Self {
        if let Some(query) = self.search_query.as_mut() {
            query.pop();
            self.apply_search();
        }
        self.update_footer()
    }

    #[inline]
    pub fn start_testing(&mut self) -> &mut Self {
        self.testing = true;
        self.update_footer()
    }

    #[inline]
    pub fn end_testing(&mut self) -> &mut Self {
        self.testing = false;
        self.update_footer()
    }

    pub fn sort_groups_with_frequency(&mut self, freq: &HashMap<String, usize>) -> &mut Self {
        self.groups
            .sort_by(|a, b| match (freq.get(&a.name), freq.get(&b.name)) {
                (Some(a_freq), Some(b_freq)) => b_freq.cmp(a_freq),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => a.name.cmp(&b.name),
            });
        self.apply_search();
        self
    }

    pub fn update_footer(&mut self) -> &mut Self {
        let mut footer = Footer::default();
        let current_group = match self.groups.get(self.cursor) {
            Some(grp) => grp,
            _ => return self,
        };

        if !self.expanded {
            let group_name = current_group.name.clone();
            let style = Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::REVERSED);

            let highlight = style.add_modifier(Modifier::BOLD);
            let sort = tagged_footer("Sort", style, self.sort_method);

            let mut left = vec![
                FooterItem::span(Span::styled(" FREE ", style)),
                FooterItem::span(Span::styled(" SPACE to expand ", style)),
                FooterItem::span(Span::styled(" ENTER choose node ", style)),
                if self.testing {
                    FooterItem::span(Span::styled(" Testing ", highlight.fg(Color::Green)))
                } else {
                    FooterItem::spans(help_footer("Test", style, highlight)).wrapped()
                },
                FooterItem::spans(sort),
            ];

            if !self.testing {
                left.insert(
                    3,
                    FooterItem::span(Span::styled(" Shift-T Test All ", style)),
                );
            }

            if let Some(query) = &self.search_query {
                left.push(FooterItem::span(Span::styled(" SEARCH ", highlight)));
                left.push(FooterItem::span(Span::raw(query.to_owned())).wrapped());
                left.push(FooterItem::span(Span::styled(" Esc Cancel ", style)));
            } else {
                left.push(FooterItem::span(Span::styled(" / Search ", style)));
            }

            footer.append_left(&mut left);

            let name = FooterItem::span(Span::styled(group_name, style)).wrapped();
            footer.push_right(name);

            if let Some(now) = current_group.current {
                footer.push_right(
                    FooterItem::span(Span::raw(current_group.members[now].name.to_owned()))
                        .wrapped(),
                );
            }
        } else {
            let style = Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::REVERSED);
            let highlight = style.add_modifier(Modifier::BOLD);

            footer.push_left(FooterItem::span(Span::styled(" [^] ▲ ▼ Move ", style)));

            if current_group.proxy_type.is_selector() {
                footer.push_left(FooterItem::span(Span::styled(
                    " ENTER switch node ",
                    style,
                )));
            }

            footer.push_left(if self.testing {
                FooterItem::span(Span::styled(" Testing ", highlight.fg(Color::Blue)))
            } else {
                FooterItem::spans(help_footer("Test", style, highlight)).wrapped()
            });

            if !self.testing {
                footer.push_left(FooterItem::span(Span::styled(" Shift-T Test All ", style)));
            }

            footer.push_left(tagged_footer("Sort", style, self.sort_method).into());

            if let Some(query) = &self.search_query {
                footer.push_left(FooterItem::span(Span::styled(" SEARCH ", highlight)));
                footer.push_left(FooterItem::span(Span::raw(query.to_owned())).wrapped());
                footer.push_left(FooterItem::span(Span::styled(" Esc Cancel ", style)));
            } else {
                footer.push_left(FooterItem::span(Span::styled(" / Search ", style)));
            }

            if let Some(ref now) = current_group.members[current_group.cursor].now {
                footer.push_right(FooterItem::span(Span::raw(now.to_owned())).wrapped());
            }
        }
        self.footer = footer;
        self
    }

    fn apply_search(&mut self) -> &mut Self {
        let Some(query) = self.search_query.as_ref() else {
            self.visible_group_indices = None;
            self.visible_member_indices.clear();
            return self;
        };
        if query.is_empty() {
            self.visible_group_indices = None;
            self.visible_member_indices.clear();
            return self;
        }

        let query = query.to_lowercase();
        let mut visible_group_indices = Vec::new();
        let mut visible_member_indices = HashMap::new();
        let mut first_match = None;

        for (group_index, group) in self.groups.iter().enumerate() {
            if group.name.to_lowercase().contains(&query) {
                visible_group_indices.push(group_index);
                first_match.get_or_insert((group_index, None));
                continue;
            }

            let members = group
                .members
                .iter()
                .enumerate()
                .filter_map(|(member_index, member)| {
                    member
                        .name
                        .to_lowercase()
                        .contains(&query)
                        .then_some(member_index)
                })
                .collect::<Vec<_>>();

            if !members.is_empty() {
                let first_member = members[0];
                visible_group_indices.push(group_index);
                visible_member_indices.insert(group_index, members);
                first_match.get_or_insert((group_index, Some(first_member)));
            }
        }

        if let Some((group_index, member_index)) = first_match {
            self.cursor = group_index;
            if let Some(member_index) = member_index {
                self.groups[group_index].cursor = member_index;
                self.expanded = true;
            } else {
                self.expanded = false;
            }
        } else {
            self.cursor = 0;
            self.expanded = false;
        }

        self.visible_group_indices = Some(visible_group_indices);
        self.visible_member_indices = visible_member_indices;
        self
    }

    pub(super) fn visible_group_indices(&self) -> Vec<usize> {
        self.visible_group_indices
            .clone()
            .unwrap_or_else(|| (0..self.groups.len()).collect())
    }

    pub(super) fn visible_member_indices(&self, group_index: usize) -> Option<&[usize]> {
        self.visible_member_indices
            .get(&group_index)
            .map(Vec::as_slice)
    }

    pub(super) fn visible_cursor_pos(&self) -> usize {
        self.visible_group_indices
            .as_ref()
            .and_then(|indices| indices.iter().position(|index| *index == self.cursor))
            .unwrap_or(self.cursor)
    }

    fn visible_group_count(&self) -> usize {
        self.visible_group_indices
            .as_ref()
            .map_or_else(|| self.groups.len(), Vec::len)
    }

    pub fn replace_with(&mut self, mut new_tree: ProxyTree<'a>) -> &mut Self {
        // let map = HashMap::<_, _, RandomState>::from_iter(self.groups.iter().map(|x|
        // (&x.name, x)));
        let old_groups = &self.groups;
        let current_group = self.groups.get(self.cursor);
        for (index, new_group) in new_tree.groups.iter_mut().enumerate() {
            if let Some(true) = current_group.map(|x| x.name == new_group.name) {
                new_tree.cursor = index;
            }
            if let Some(old_group) = old_groups.iter().find(|group| group.name == new_group.name) {
                new_group.cursor = old_group
                    .members
                    .get(old_group.cursor)
                    .and_then(|old_member| {
                        new_group
                            .members
                            .iter()
                            .position(|new_member| new_member.name == old_member.name)
                    })
                    .or(new_group.current)
                    .unwrap_or_default()
            }
        }
        self.groups = new_tree.groups;
        let method = self.sort_method;
        self.sort_with(&method);
        self.apply_search();
        self.update_footer()
    }
}

/// Walk the `now` chain of a (possibly nested) group until a proxy with a
/// latency history is found, guarding against selection cycles.
fn resolve_history(
    proxies: &Proxies,
    name: &str,
) -> Option<mihomoctl_core::model::History> {
    let mut name = name;
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(name) {
            return None;
        }
        let proxy = proxies.get(name)?;
        if let Some(history) = proxy.latest_history() {
            return Some(history.clone());
        }
        name = proxy.now.as_deref()?;
    }
}

impl<'a> From<Proxies> for ProxyTree<'a> {
    fn from(val: Proxies) -> Self {
        let mut ret = Self {
            groups: Vec::with_capacity(val.len()),
            ..Default::default()
        };
        for (name, group) in val.groups() {
            let all = group
                .all
                .as_ref()
                .expect("ProxyGroup should have member vec");
            let mut members = Vec::with_capacity(all.len());
            for x in all.iter() {
                let mut member: ProxyItem = (
                    x.as_str(),
                    val.get(x)
                        .to_owned()
                        .expect("Group member should be in all proxies"),
                )
                    .into();
                // Groups usually have no latency history of their own; show
                // the latency of the node they currently resolve to instead.
                if member.history.is_none() {
                    member.history = resolve_history(&val, x);
                }
                members.push(member);
            }

            // if group.now.is_some then it must be in all proxies
            // So use map & expect instead of Option#and_then
            let current = group.now.as_ref().map(|name| {
                members
                    .iter()
                    .position(|item: &ProxyItem| &item.name == name)
                    .expect("Group member should be in all proxies")
            });

            ret.groups.push(ProxyGroup {
                _life: PhantomData,
                name: name.to_owned(),
                proxy_type: group.proxy_type,
                cursor: current.unwrap_or_default(),
                current,
                members,
            })
        }

        ret
    }
}

impl<'a> MovableListManage for ProxyTree<'a> {
    fn sort(&mut self) -> &mut Self {
        let method = self.sort_method;
        self.sort_with(&method);
        self.apply_search();
        self
    }

    fn next_sort(&mut self) -> &mut Self {
        self.sort_method.next_self();
        let method = self.sort_method;
        self.sort_with(&method);
        self.apply_search();
        self.update_footer()
    }

    fn prev_sort(&mut self) -> &mut Self {
        self.sort_method.prev_self();
        let method = self.sort_method;
        self.sort_with(&method);
        self.apply_search();
        self.update_footer()
    }

    fn current_pos(&self) -> Coord {
        Default::default()
    }

    #[inline]
    fn toggle(&mut self) -> &mut Self {
        self.expanded = !self.expanded;
        self.update_footer()
    }

    #[inline]
    fn end(&mut self) -> &mut Self {
        self.expanded = false;
        self.update_footer()
    }

    #[inline]
    fn len(&self) -> usize {
        self.visible_group_count()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.visible_group_count() == 0
    }

    fn hold(&mut self) -> &mut Self {
        self.expanded = true;
        self
    }

    fn handle(&mut self, event: ListEvent) -> Option<Action> {
        if self.expanded {
            let step = if event.fast { 3 } else { 1 };
            let visible_members = self.visible_member_indices.get(&self.cursor).cloned();
            let group = &mut self.groups[self.cursor];
            match event.code {
                KeyCode::Up => {
                    if let Some(visible_members) = &visible_members {
                        let pos = visible_members
                            .iter()
                            .position(|index| *index == group.cursor)
                            .unwrap_or_default();
                        group.cursor = visible_members[pos.saturating_sub(step)];
                    } else if group.cursor > 0 {
                        group.cursor = group.cursor.saturating_sub(step);
                    }
                }
                KeyCode::Down => {
                    if let Some(visible_members) = &visible_members {
                        let pos = visible_members
                            .iter()
                            .position(|index| *index == group.cursor)
                            .unwrap_or_default();
                        let next = (pos + step).min(visible_members.len().saturating_sub(1));
                        group.cursor = visible_members[next];
                    } else {
                        let left = group.members.len().saturating_sub(group.cursor + 1);
                        group.cursor += left.min(step);
                    }
                }
                KeyCode::Right | KeyCode::Enter => {
                    if group.proxy_type.is_selector() {
                        let current = group.members[group.cursor].name.to_owned();
                        return Some(Action::ApplySelection {
                            group: group.name.to_owned(),
                            proxy: current,
                        });
                    }
                }
                _ => {}
            }
        } else {
            let visible_group_indices = self.visible_group_indices();
            let visible_cursor = visible_group_indices
                .iter()
                .position(|index| *index == self.cursor)
                .unwrap_or_default();
            match event.code {
                KeyCode::Up => {
                    if visible_cursor > 0 {
                        self.cursor = visible_group_indices[visible_cursor.saturating_sub(1)];
                    }
                }
                KeyCode::Down => {
                    if visible_cursor + 1 < visible_group_indices.len() {
                        self.cursor = visible_group_indices[visible_cursor + 1];
                    }
                }
                KeyCode::Enter => {
                    self.expanded = true;
                    // Start the member view at the top; the render pass will
                    // scroll down to the selected node only if it overflows.
                    self.member_window_start.set(0);
                }
                _ => {}
            }
        }
        self.update_footer();
        None
    }

    fn offset(&self) -> &crate::Coord {
        &Coord {
            x: 0,
            y: 0,
            hold: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use mihomoctl_core::model::{History, Proxy, ProxyType};
    use tui::{buffer::Buffer, layout::Rect, widgets::Widget};

    use crate::components::{Consts, ProxyTreeWidget};

    use super::*;

    fn item(name: &str) -> ProxyItem {
        item_with_type(name, ProxyType::Vmess, Some(10))
    }

    fn item_with_type(name: &str, proxy_type: ProxyType, delay: Option<u64>) -> ProxyItem {
        ProxyItem {
            name: name.to_owned(),
            proxy_type,
            history: delay.map(|delay| History {
                time: Default::default(),
                delay,
            }),
            udp: None,
            now: None,
        }
    }

    fn group(name: &str, members: Vec<ProxyItem>) -> ProxyGroup<'static> {
        ProxyGroup {
            name: name.to_owned(),
            proxy_type: ProxyType::Selector,
            members,
            current: None,
            cursor: 0,
            _life: PhantomData,
        }
    }

    fn proxy(proxy_type: ProxyType, delays: &[u64], now: Option<&str>) -> Proxy {
        Proxy {
            proxy_type,
            history: delays
                .iter()
                .map(|delay| History {
                    time: Default::default(),
                    delay: *delay,
                })
                .collect(),
            udp: None,
            all: None,
            now: now.map(ToOwned::to_owned),
        }
    }

    fn proxies(entries: Vec<(&str, Proxy)>) -> Proxies {
        Proxies {
            proxies: entries
                .into_iter()
                .map(|(name, proxy)| (name.to_owned(), proxy))
                .collect(),
        }
    }

    #[test]
    fn unique_normal_members_dedupes_and_skips_groups() {
        let tree = ProxyTree {
            groups: vec![
                group(
                    "A",
                    vec![
                        item("x"),
                        item("y"),
                        item_with_type("Inner", ProxyType::Selector, None),
                    ],
                ),
                group("B", vec![item("y"), item("z")]),
            ],
            ..ProxyTree::default()
        };

        assert_eq!(tree.unique_normal_members(), vec!["x", "y", "z"]);
    }

    #[test]
    fn proxy_item_shows_the_latest_latency_test() {
        let mut all = proxies(vec![(
            "node",
            proxy(ProxyType::Vmess, &[500, 42], None),
        )]);
        all.proxies.insert(
            "Group".to_owned(),
            Proxy {
                all: Some(vec!["node".to_owned()]),
                ..proxy(ProxyType::Selector, &[], Some("node"))
            },
        );

        let tree: ProxyTree = all.into();
        assert_eq!(tree.groups[0].members[0].delay(), Some(42));
    }

    #[test]
    fn nested_group_member_resolves_latency_through_now_chain() {
        let mut all = proxies(vec![
            ("node", proxy(ProxyType::Vmess, &[123], None)),
            ("untested", proxy(ProxyType::Vmess, &[], None)),
        ]);
        all.proxies.insert(
            "Inner".to_owned(),
            Proxy {
                all: Some(vec!["node".to_owned(), "untested".to_owned()]),
                ..proxy(ProxyType::Selector, &[], Some("node"))
            },
        );
        all.proxies.insert(
            "Outer".to_owned(),
            Proxy {
                all: Some(vec!["Inner".to_owned(), "untested".to_owned()]),
                ..proxy(ProxyType::Selector, &[], Some("Inner"))
            },
        );

        let tree: ProxyTree = all.into();
        let outer = tree
            .groups
            .iter()
            .find(|group| group.name == "Outer")
            .unwrap();
        let inner_member = outer
            .members
            .iter()
            .find(|member| member.name() == "Inner")
            .unwrap();
        // "Inner" has no history of its own: latency comes from "node"
        assert_eq!(inner_member.delay(), Some(123));
        // Untested normal nodes still show no latency
        let untested = outer
            .members
            .iter()
            .find(|member| member.name() == "untested")
            .unwrap();
        assert_eq!(untested.delay(), None);
    }

    #[test]
    fn latency_resolution_survives_selection_cycles() {
        let mut all = proxies(vec![]);
        all.proxies.insert(
            "A".to_owned(),
            Proxy {
                all: Some(vec!["B".to_owned()]),
                ..proxy(ProxyType::Selector, &[], Some("B"))
            },
        );
        all.proxies.insert(
            "B".to_owned(),
            Proxy {
                all: Some(vec!["A".to_owned()]),
                ..proxy(ProxyType::Selector, &[], Some("A"))
            },
        );

        let tree: ProxyTree = all.into();
        for group in &tree.groups {
            assert_eq!(group.members[0].delay(), None);
        }
    }

    #[test]
    fn proxy_tree_search_focuses_matching_member_and_can_cancel() {
        let mut tree = ProxyTree {
            groups: vec![
                group("GlobalMedia", vec![item("home-pass"), item("racknerd")]),
                group("Fallback", vec![item("trojan-racknerd")]),
            ],
            ..ProxyTree::default()
        };

        tree.begin_search();
        for ch in "rack".chars() {
            tree.input_search_char(ch);
        }

        assert!(tree.is_searching());
        assert!(tree.expanded);
        assert_eq!(tree.cursor, 0);
        assert_eq!(tree.groups[0].cursor, 1);
        assert_eq!(tree.visible_group_indices(), vec![0, 1]);
        assert_eq!(tree.visible_member_indices(0), Some(&[1][..]));
        assert_eq!(tree.visible_member_indices(1), Some(&[0][..]));

        tree.cancel_search();
        assert!(!tree.is_searching());
    }

    #[test]
    fn proxy_tree_footer_hints_the_search_shortcut() {
        let footer_text = |tree: &ProxyTree| {
            tree.footer
                .items()
                .flat_map(|item| item.to_spans().0)
                .map(|span| span.content.into_owned())
                .collect::<String>()
        };

        let mut tree = ProxyTree {
            groups: vec![group("GlobalMedia", vec![item("racknerd")])],
            ..ProxyTree::default()
        };
        tree.update_footer();
        assert!(footer_text(&tree).contains("/ Search"));

        tree.begin_search();
        let rendered = footer_text(&tree);
        assert!(rendered.contains("SEARCH"));
        assert!(rendered.contains("Esc Cancel"));
    }

    #[test]
    fn collapsed_proxy_tree_scrolls_only_when_focus_would_overflow() {
        let mut tree = ProxyTree {
            groups: (0..6)
                .map(|i| group(&format!("G{i}"), vec![item("m")]))
                .collect(),
            ..ProxyTree::default()
        };
        // Inner height is 6 rows (8 minus the block border), and each collapsed
        // group is 2 rows, so exactly three groups fit on screen.
        let area = Rect::new(0, 0, 60, 8);
        let render = |tree: &ProxyTree| {
            let mut buf = Buffer::empty(area);
            ProxyTreeWidget::new(tree).render(area, &mut buf);
        };

        // Focus inside the first screenful: nothing scrolls.
        tree.cursor = 2;
        render(&tree);
        assert_eq!(tree.window_start.get(), 0);

        // Focus one past the bottom edge: the window advances by a single group.
        tree.cursor = 3;
        render(&tree);
        assert_eq!(tree.window_start.get(), 1);

        // Moving back up but still within the window does not scroll further.
        tree.cursor = 1;
        render(&tree);
        assert_eq!(tree.window_start.get(), 1);

        // Moving above the window pulls it back to the top.
        tree.cursor = 0;
        render(&tree);
        assert_eq!(tree.window_start.get(), 0);
    }

    #[test]
    fn expanded_proxy_tree_scrolls_members_only_when_focus_would_overflow() {
        let members = (0..10).map(|i| item(&format!("m{i}"))).collect::<Vec<_>>();
        let mut tree = ProxyTree {
            groups: vec![group("G", members)],
            expanded: true,
            ..ProxyTree::default()
        };
        // Inner height 6 (8 minus border), header takes one row, so five member
        // rows fit on screen.
        let area = Rect::new(0, 0, 60, 8);
        let render = |tree: &ProxyTree| {
            let mut buf = Buffer::empty(area);
            ProxyTreeWidget::new(tree).render(area, &mut buf);
        };

        // Focused node within the first screenful: no scroll.
        tree.groups[0].cursor = 4;
        render(&tree);
        assert_eq!(tree.member_window_start.get(), 0);

        // One past the bottom edge: the member window advances by one row.
        tree.groups[0].cursor = 5;
        render(&tree);
        assert_eq!(tree.member_window_start.get(), 1);

        // Moving up but still inside the window does not scroll further.
        tree.groups[0].cursor = 2;
        render(&tree);
        assert_eq!(tree.member_window_start.get(), 1);

        // Above the window pulls it back up.
        tree.groups[0].cursor = 0;
        render(&tree);
        assert_eq!(tree.member_window_start.get(), 0);
    }

    #[test]
    fn proxy_tree_search_renders_only_matching_members() {
        let mut tree = ProxyTree {
            groups: vec![
                group("GlobalMedia", vec![item("home-pass"), item("racknerd")]),
                group("Fallback", vec![item("trojan-racknerd")]),
            ],
            ..ProxyTree::default()
        };

        tree.begin_search();
        for ch in "rack".chars() {
            tree.input_search_char(ch);
        }

        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        ProxyTreeWidget::new(&tree).render(area, &mut buf);
        let mut rendered = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                rendered.push_str(buf.get(x, y).symbol.as_str());
            }
        }

        assert!(rendered.contains("racknerd"));
        assert!(rendered.contains("Fallback"));
        assert!(!rendered.contains("home-pass"));
    }

    #[test]
    fn collapsed_proxy_tree_renders_one_selection_arrow() {
        let tree = ProxyTree {
            groups: vec![group("GlobalMedia", vec![item("home-pass"), item("racknerd")])],
            ..ProxyTree::default()
        };

        let area = Rect::new(0, 0, 60, 6);
        let mut buf = Buffer::empty(area);
        ProxyTreeWidget::new(&tree).render(area, &mut buf);
        let mut rendered = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                rendered.push_str(buf.get(x, y).symbol.as_str());
            }
        }

        assert_eq!(rendered.matches('>').count(), 1);
    }

    #[test]
    fn collapsed_proxy_tree_uses_history_for_group_member_status() {
        let tree = ProxyTree {
            groups: vec![group(
                "GLOBAL",
                vec![item_with_type("Fallback", ProxyType::URLTest, Some(42))],
            )],
            ..ProxyTree::default()
        };

        let area = Rect::new(0, 0, 60, 6);
        let mut buf = Buffer::empty(area);
        ProxyTreeWidget::new(&tree).render(area, &mut buf);
        let mut rendered = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                rendered.push_str(buf.get(x, y).symbol.as_str());
            }
        }

        assert!(rendered.contains(Consts::PROXY_LATENCY_SIGN.trim()));
        assert!(!rendered.contains(Consts::NOT_PROXY_SIGN.trim()));
    }

    #[test]
    fn enter_switches_to_the_selected_node_in_a_selector_group() {
        let mut tree = ProxyTree {
            groups: vec![group("GLOBAL", vec![item("node-a"), item("node-b")])],
            expanded: true,
            ..ProxyTree::default()
        };
        tree.groups[0].cursor = 1;

        let action = tree.handle(ListEvent {
            fast: false,
            code: KeyCode::Enter,
        });

        assert_eq!(
            action,
            Some(Action::ApplySelection {
                group: "GLOBAL".to_owned(),
                proxy: "node-b".to_owned(),
            })
        );
    }
}
