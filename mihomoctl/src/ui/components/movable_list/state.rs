use std::{
    borrow::Cow,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use crossterm::event::KeyCode;
use match_any::match_any;
use paste::paste;
use smart_default::SmartDefault;
use tui::text::Spans;

use crate::{
    interactive::{EndlessSelf, SortMethod, Sortable},
    ui::{
        api::ApiListState,
        components::{MovableListItem, ProxyTree},
        utils::Coord,
    },
    Action, ConListState, DebugListState, ListEvent, LogListState, RuleListState,
};

macro_rules! impl_setter {
    ($prop:ident, $ty:ty) => {
        paste! {
            pub fn [<set_ $prop>](&mut self, $prop: $ty) -> &mut Self {
                self.$prop = $prop;
                self
            }
        }
    };
    ($prop:ident, $val:expr) => {
        pub fn $prop(&mut self) -> &mut Self {
            self.$prop = $val;
            self
        }
    };
    ($fn_name:ident, $prop:ident, $val:expr) => {
        pub fn $fn_name(&mut self) -> &mut Self {
            self.$prop = $val;
            self
        }
    };
}

impl<'a, T, S> MovableListState<'a, T, S>
where
    T: MovableListItem<'a>,
    S: SortMethod<T> + EndlessSelf + Default,
{
    impl_setter!(with_index, true);

    impl_setter!(without_index, with_index, false);

    impl_setter!(asc_index, reverse_index, false);

    impl_setter!(dsc_index, reverse_index, true);

    impl_setter!(normal_order, reverse_items, false);

    impl_setter!(reverse_order, reverse_items, true);

    impl_setter!(pausable, pause_enabled, true);

    impl_setter!(items, Vec<T>);

    impl_setter!(padding, u16);

    pub fn new(items: Vec<T>) -> Self
    where
        T: MovableListItem<'a>,
    {
        Self {
            items,
            ..Default::default()
        }
    }

    pub fn new_with_sort(mut items: Vec<T>, sort: S) -> Self
    where
        T: MovableListItem<'a>,
    {
        items.sort_by(|a, b| sort.sort_fn(a, b));

        Self {
            items,
            sort,
            ..Default::default()
        }
    }

    pub fn placeholder<P: Into<Cow<'a, str>>>(&mut self, content: P) -> &mut Self {
        self.placeholder = Some(content.into());
        self
    }

    pub fn header(&mut self, content: Spans<'a>) -> &mut Self {
        self.header = Some(content);
        self
    }

    pub fn sorted_merge(&mut self, other: Vec<T>) {
        self.items = other;
        self.sort();
        self.apply_search();
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
        if self.offset.hold {
            self.offset.y += 1;
        }
        self.apply_search();
    }

    /// Push a new item while retaining at most `cap` items, discarding the
    /// oldest ones when the limit is exceeded. Used for unbounded streaming
    /// lists (e.g. logs) to prevent memory from growing indefinitely.
    pub fn push_capped(&mut self, item: T, cap: usize) {
        self.items.push(item);
        if self.offset.hold {
            self.offset.y += 1;
        }
        if cap > 0 && self.items.len() > cap {
            let overflow = self.items.len() - cap;
            self.items.drain(..overflow);
            // `offset.y` counts rows from the newest end, so trimming the
            // oldest items leaves it valid; clamp only as a safety net.
            self.offset.y = self.offset.y.min(self.items.len().saturating_sub(1));
        }
        self.apply_search();
    }

    /// Remove every item and reset the scroll/search view to a clean state.
    /// Used by the "clear" action on streaming pages (Logs, Conns).
    pub fn clear(&mut self) {
        self.items.clear();
        self.offset = Coord::default();
        self.window_start.set(0);
        self.apply_search();
    }

    pub fn sort_label(&self) -> String
    where
        S: ToString,
    {
        self.sort.to_string()
    }
}

// TODO: Use lazy updated footer
#[derive(Debug, Clone, PartialEq, Eq, SmartDefault)]
pub struct MovableListState<'a, T: MovableListItem<'a>, S: Default> {
    pub(super) offset: Coord,
    pub(super) items: Vec<T>,
    pub(super) header: Option<Spans<'a>>,
    pub(super) placeholder: Option<Cow<'a, str>>,
    #[default = 1]
    pub(super) padding: u16,
    pub(super) sort: S,
    pub(super) with_index: bool,
    pub(super) reverse_index: bool,
    #[default = true]
    pub(super) reverse_items: bool,
    pause_enabled: bool,
    paused: bool,
    search_query: Option<String>,
    filtered_indices: Option<Vec<usize>>,
    /// First visible row of the last rendered window. Updated during render
    /// so the cursor walks down the page and only scrolls at the edges.
    pub(super) window_start: WindowStart,
}

/// Render-time scroll anchor. Interior mutability is needed because widgets
/// render from `&self`, and it has to be `Sync` as the states are shared
/// across threads.
#[derive(Debug, Default)]
pub(super) struct WindowStart(std::sync::atomic::AtomicUsize);

impl WindowStart {
    pub(super) fn get(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(super) fn set(&self, value: usize) {
        self.0.store(value, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Clone for WindowStart {
    fn clone(&self) -> Self {
        Self(std::sync::atomic::AtomicUsize::new(self.get()))
    }
}

impl PartialEq for WindowStart {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Eq for WindowStart {}

impl<'a, T, S> MovableListState<'a, T, S>
where
    T: MovableListItem<'a>,
    S: Default,
{
    #[inline]
    pub fn is_searching(&self) -> bool {
        self.search_query.is_some()
    }

    #[inline]
    pub fn search_query(&self) -> Option<&str> {
        self.search_query.as_deref()
    }

    pub fn begin_search(&mut self) -> &mut Self {
        self.search_query = Some(String::new());
        self.filtered_indices = None;
        self
    }

    pub fn cancel_search(&mut self) -> &mut Self {
        if let Some(item_index) = self.current_visible_item_index() {
            self.offset.y = self.items.len().saturating_sub(item_index + 1);
        }
        self.search_query = None;
        self.filtered_indices = None;
        self
    }

    #[inline]
    pub fn can_pause(&self) -> bool {
        self.pause_enabled
    }

    #[inline]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn toggle_paused(&mut self) -> &mut Self {
        if self.pause_enabled {
            self.paused = !self.paused;
        }
        self
    }

    pub fn input_search_char(&mut self, ch: char) -> &mut Self {
        if let Some(query) = self.search_query.as_mut() {
            query.push(ch);
            self.apply_search();
        }
        self
    }

    pub fn backspace_search(&mut self) -> &mut Self {
        if let Some(query) = self.search_query.as_mut() {
            query.pop();
            self.apply_search();
        }
        self
    }

    fn apply_search(&mut self) -> &mut Self {
        let Some(query) = self.search_query.as_ref() else {
            self.filtered_indices = None;
            return self;
        };
        if query.is_empty() {
            self.filtered_indices = None;
            return self;
        }

        let query = query.to_lowercase();
        let filtered_indices = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                item_search_text(item)
                    .to_lowercase()
                    .contains(&query)
                    .then_some(index)
            })
            .collect::<Vec<_>>();

        self.filtered_indices = Some(filtered_indices);
        self.offset.x = 0;
        self.offset.y = 0;
        self.offset.hold = true;
        self
    }

    pub(super) fn visible_len(&self) -> usize {
        self.filtered_indices
            .as_ref()
            .map_or_else(|| self.items.len(), Vec::len)
    }

    pub(super) fn visible_item_index(&self, visible_index: usize) -> Option<usize> {
        self.filtered_indices
            .as_ref()
            .map_or(Some(visible_index), |indices| indices.get(visible_index).copied())
    }

    pub(crate) fn current_item_index(&self) -> Option<usize> {
        let offset = self.offset.y;
        let visible_index = if self.reverse_items {
            self.visible_len().checked_sub(offset.saturating_add(1))?
        } else {
            (offset < self.visible_len()).then_some(offset)?
        };

        self.filtered_indices
            .as_ref()
            .map_or(Some(visible_index), |indices| indices.get(visible_index).copied())
    }

    fn current_visible_item_index(&self) -> Option<usize> {
        self.filtered_indices
            .as_ref()
            .and_then(|_| self.current_item_index())
    }
}

fn item_search_text<'a, T>(item: &T) -> String
where
    T: MovableListItem<'a>,
{
    item.to_spans()
        .0
        .into_iter()
        .map(|span| span.content.into_owned())
        .collect()
}

impl<'a, T, S> Deref for MovableListState<'a, T, S>
where
    T: MovableListItem<'a>,
    S: Default,
{
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl<'a, T, S> DerefMut for MovableListState<'a, T, S>
where
    T: MovableListItem<'a>,
    S: Default,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
    }
}

impl<'a, T, S> Extend<T> for MovableListState<'a, T, S>
where
    T: MovableListItem<'a>,
    S: Default,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.items.extend(iter);
        self.apply_search();
    }
}

impl<'a, T, S> Sortable<'a, S> for MovableListState<'a, T, S>
where
    T: MovableListItem<'a>,
    S: SortMethod<T> + Default,
{
    type Item<'b> = T;

    fn sort_with(&mut self, method: &S) {
        self.items.sort_by(|a, b| method.sort_fn(a, b))
    }
}

pub trait MovableListManage {
    fn sort(&mut self) -> &mut Self;

    fn next_sort(&mut self) -> &mut Self;

    fn prev_sort(&mut self) -> &mut Self;

    fn current_pos(&self) -> Coord;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool;
    fn toggle(&mut self) -> &mut Self;

    fn end(&mut self) -> &mut Self;

    fn hold(&mut self) -> &mut Self;

    fn handle(&mut self, event: ListEvent) -> Option<Action>;
    fn offset(&self) -> &Coord;
}

impl<'a, T, S> MovableListManage for MovableListState<'a, T, S>
where
    T: MovableListItem<'a>,
    S: SortMethod<T> + EndlessSelf + Default,
{
    fn sort(&mut self) -> &mut Self {
        let sort = &self.sort;
        self.items.sort_with(sort);
        self.apply_search();
        self
    }

    fn next_sort(&mut self) -> &mut Self {
        self.sort.next_self();
        let sort = &self.sort;
        self.items.sort_with(sort);
        self.apply_search();
        self
    }

    fn prev_sort(&mut self) -> &mut Self {
        self.sort.prev_self();
        let sort = &self.sort;
        self.items.sort_with(sort);
        self.apply_search();
        self
    }

    fn current_pos(&self) -> Coord {
        let x = self.offset.x;
        let y = self.visible_len().saturating_sub(self.offset.y);
        Coord {
            x,
            y,
            hold: self.offset.hold,
        }
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn toggle(&mut self) -> &mut Self {
        self.offset.toggle();
        self
    }

    fn end(&mut self) -> &mut Self {
        self.offset.end();
        self
    }

    fn hold(&mut self) -> &mut Self {
        self.offset.hold();
        self
    }

    fn handle(&mut self, event: ListEvent) -> Option<Action> {
        let len = self.visible_len().saturating_sub(1);
        let offset = &mut self.offset;

        if !offset.hold {
            offset.hold = true;
        }

        match (event.fast, event.code) {
            (true, KeyCode::Left) => offset.x = offset.x.saturating_sub(7),
            (true, KeyCode::Right) => offset.x = offset.x.saturating_add(7),
            (true, KeyCode::Up) => offset.y = offset.y.saturating_sub(5),
            (true, KeyCode::Down) => offset.y = offset.y.saturating_add(5).min(len),
            (false, KeyCode::Left) => offset.x = offset.x.saturating_sub(1),
            (false, KeyCode::Right) => offset.x = offset.x.saturating_add(1),
            (false, KeyCode::Up) => offset.y = offset.y.saturating_sub(1),
            (false, KeyCode::Down) => offset.y = offset.y.saturating_add(1).min(len),
            _ => {}
        }
        None
    }

    fn offset(&self) -> &Coord {
        &self.offset
    }
}

pub enum MovableListManager<'a, 'own> {
    Log(&'own mut LogListState<'a>),
    Connection(&'own mut ConListState<'a>),
    Rule(&'own mut RuleListState<'a>),
    Event(&'own mut DebugListState<'a>),
    Proxy(&'own mut ProxyTree<'a>),
    Api(&'own mut ApiListState<'a>),
}

impl<'a, 'own> MovableListManage for MovableListManager<'a, 'own> {
    fn sort(&mut self) -> &mut Self {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.sort();
            }
        );
        self
    }

    fn next_sort(&mut self) -> &mut Self {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.next_sort();
            }
        );
        self
    }

    fn prev_sort(&mut self) -> &mut Self {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.prev_sort();
            }
        );
        self
    }

    fn current_pos(&self) -> Coord {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.current_pos()
            }
        )
    }

    fn len(&self) -> usize {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.len()
            }
        )
    }

    fn is_empty(&self) -> bool {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.is_empty()
            }
        )
    }

    fn toggle(&mut self) -> &mut Self {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.toggle();
            }
        );
        self
    }

    fn end(&mut self) -> &mut Self {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.end();
            }
        );
        self
    }

    fn hold(&mut self) -> &mut Self {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.hold();
            }
        );
        self
    }

    fn handle(&mut self, event: ListEvent) -> Option<Action> {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.handle(event)
            }
        )
    }

    fn offset(&self) -> &Coord {
        match_any!(
            self,
            Self::Log(inner) |
            Self::Event(inner) |
            Self::Rule(inner) |
            Self::Connection(inner) |
            Self::Proxy(inner) |
            Self::Api(inner) => {
                inner.offset()
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::interactive::Noop;

    use super::MovableListState;

    #[test]
    fn list_search_filters_to_matching_items() {
        let mut state: MovableListState<'_, String, Noop> = MovableListState::new(vec![
            "old needle".to_owned(),
            "middle row".to_owned(),
            "new needle".to_owned(),
        ]);

        state.begin_search();
        for ch in "needle".chars() {
            state.input_search_char(ch);
        }

        assert!(state.is_searching());
        assert_eq!(state.search_query(), Some("needle"));
        assert_eq!(state.visible_len(), 2);
        assert_eq!(state.offset.y, 0);
    }

    #[test]
    fn list_search_restores_full_position_on_cancel() {
        let mut state: MovableListState<'_, String, Noop> = MovableListState::new(vec![
            "alpha".to_owned(),
            "needle row".to_owned(),
            "omega".to_owned(),
        ]);

        state.begin_search();
        for ch in "needle".chars() {
            state.input_search_char(ch);
        }

        assert_eq!(state.visible_len(), 1);
        assert_eq!(state.offset.y, 0);

        state.cancel_search();

        assert!(!state.is_searching());
        assert_eq!(state.search_query(), None);
        assert_eq!(state.visible_len(), 3);
        assert_eq!(state.offset.y, 1);
    }
}
