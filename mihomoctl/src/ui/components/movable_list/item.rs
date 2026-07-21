use std::borrow::Cow;

use tui::text::{Span, Spans};

pub trait MovableListItem<'a> {
    fn to_spans(&self) -> Spans<'a>;

    /// Text used when the list is filtered by a search query. Defaults to the
    /// full rendered line; items with a meaningful sub-field (e.g. a rule's
    /// payload) can override this to scope matching to that field only.
    fn search_text(&self) -> String {
        self.to_spans()
            .0
            .into_iter()
            .map(|span| span.content.into_owned())
            .collect()
    }

    /// Whether this item matches an active (already lowercased) search query.
    /// Defaults to a case-insensitive substring test over [`search_text`]; items
    /// can override to add semantic matching (e.g. an IP-CIDR rule matching a
    /// queried address by network containment rather than by characters).
    ///
    /// [`search_text`]: MovableListItem::search_text
    fn matches_query(&self, query: &str) -> bool {
        self.search_text().to_lowercase().contains(query)
    }
}

impl<'a> MovableListItem<'a> for Spans<'a> {
    fn to_spans(&self) -> Spans<'a> {
        self.to_owned()
    }
}

impl<'a> MovableListItem<'a> for String {
    fn to_spans(&self) -> Spans<'a> {
        Spans(vec![Span::raw(self.to_owned())])
    }
}

impl<'a> MovableListItem<'a> for Cow<'a, str> {
    fn to_spans(&self) -> Spans<'a> {
        Spans(vec![Span::raw(self.clone())])
    }
}

pub trait MovableListItemExt<'a>: MovableListItem<'a> {
    fn width(&self) -> usize {
        self.to_spans().width()
    }
}

impl<'a, T> MovableListItemExt<'a> for T where T: MovableListItem<'a> {}
