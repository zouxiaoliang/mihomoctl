use std::{fmt::Debug, marker::PhantomData};

use mihomoctl_core::model::ProxyType;
use tui::{
    style::{Color, Modifier, Style},
    text::{Span, Spans},
};

use crate::ui::{
    components::{Consts, ProxyItem},
    utils::get_text_style,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProxyGroup<'a> {
    pub(super) name: String,
    pub(super) proxy_type: ProxyType,
    pub(super) members: Vec<ProxyItem>,
    pub(super) current: Option<usize>,
    pub(super) cursor: usize,
    pub(super) _life: PhantomData<&'a ()>,
}

pub enum ProxyGroupFocusStatus {
    None,
    Focused,
    Expanded,
}

impl<'a> ProxyGroup<'a> {
    pub fn proxy_type(&self) -> ProxyType {
        self.proxy_type
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn members(&self) -> &Vec<ProxyItem> {
        &self.members
    }

    pub fn current_member_name(&self) -> Option<&str> {
        self.members.get(self.cursor).map(|member| member.name())
    }

    pub fn get_summary_widget(&'a self) -> impl Iterator<Item = Span<'static>> + 'a {
        self.members.iter().map(Self::member_status_span)
    }

    pub fn get_widget(&'a self, width: usize, status: ProxyGroupFocusStatus) -> Vec<Spans<'a>> {
        self.get_filtered_widget(width, status, None, 0)
    }

    /// Number of terminal rows this group occupies when rendered collapsed (the
    /// non-expanded view): one header row plus the member badges wrapped into
    /// rows. Kept in sync with the chunking in [`get_filtered_widget`] so the
    /// tree can decide when scrolling is actually necessary.
    pub fn collapsed_height(&self, width: usize, visible_member_indices: Option<&[usize]>) -> usize {
        let count = visible_member_indices
            .map(<[usize]>::len)
            .unwrap_or_else(|| self.members.len());
        let per_row = width
            .saturating_sub(Consts::FOCUSED_INDICATOR_SPAN.width() + 2)
            .saturating_div(2)
            .max(1);
        1 + count.div_ceil(per_row)
    }

    pub fn get_filtered_widget(
        &'a self,
        width: usize,
        status: ProxyGroupFocusStatus,
        visible_member_indices: Option<&[usize]>,
        member_skip: usize,
    ) -> Vec<Spans<'a>> {
        let member_indices = visible_member_indices
            .map(<[usize]>::to_vec)
            .unwrap_or_else(|| (0..self.members.len()).collect::<Vec<_>>());
        let delimiter = Span::raw(" ");
        let prefix = if matches!(status, ProxyGroupFocusStatus::Focused) {
            Consts::FOCUSED_INDICATOR_SPAN
        } else {
            Consts::UNFOCUSED_INDICATOR_SPAN
        };
        let name = Span::styled(
            &self.name,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

        let proxy_type = Span::styled(self.proxy_type.to_string(), Consts::PROXY_TYPE_STYLE);

        let count = member_indices.len();
        let visible_cursor = member_indices
            .iter()
            .position(|index| *index == self.cursor)
            .unwrap_or_default();
        let proxy_count = Span::styled(
            if matches!(status, ProxyGroupFocusStatus::Expanded) {
                format!("{}/{}", visible_cursor + 1, count)
            } else {
                count.to_string()
            },
            Style::default().fg(Color::Green),
        );

        let mut ret = Vec::with_capacity(if matches!(status, ProxyGroupFocusStatus::Expanded) {
            member_indices.len() + 1
        } else {
            2
        });

        ret.push(Spans::from(vec![
            prefix.clone(),
            name,
            delimiter.clone(),
            proxy_type,
            delimiter,
            proxy_count,
        ]));

        if matches!(status, ProxyGroupFocusStatus::Expanded) {
            let skipped = member_skip;
            let text_style = get_text_style();
            let is_current = |index: usize| self.current.map(|x| x == index).unwrap_or(false);
            let is_pointed = |index: usize| self.cursor == index;

            let lines = member_indices
                .iter()
                .skip(skipped)
                .map(|member_index| {
                    let x = &self.members[*member_index];
                    let prefix = if self.cursor == *member_index {
                    Consts::EXPANDED_FOCUSED_INDICATOR_SPAN
                } else {
                    Consts::EXPANDED_INDICATOR_SPAN
                };
                let name = Span::styled(
                    &x.name,
                    if is_current(*member_index) {
                        Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::BOLD)
                    } else if is_pointed(*member_index) {
                        text_style.fg(Color::LightBlue)
                    } else {
                        text_style
                    },
                );
                let proxy_type = Span::styled(x.proxy_type.to_string(), Consts::PROXY_TYPE_STYLE);

                let delay_span = x
                    .history
                    .as_ref()
                    .map(|x| {
                        if x.delay > 0 {
                            let style = Self::get_delay_style(x.delay);
                            Span::styled(x.delay.to_string(), style)
                        } else {
                            Span::styled(Consts::NO_LATENCY_SIGN, Consts::NO_LATENCY_STYLE)
                        }
                    })
                    .unwrap_or_else(|| {
                        if !x.proxy_type.is_normal() {
                            Span::raw("")
                        } else {
                            Span::styled(Consts::NO_LATENCY_SIGN, Consts::NO_LATENCY_STYLE)
                        }
                    });
                vec![
                    prefix,
                    Consts::DELIMITER_SPAN.clone(),
                    name,
                    Consts::DELIMITER_SPAN.clone(),
                    proxy_type,
                    Consts::DELIMITER_SPAN.clone(),
                    delay_span,
                ]
                .into()
                });
            ret.extend(lines);
        } else {
            ret.extend(
                member_indices
                    .iter()
                    .map(|index| &self.members[*index])
                    .map(Self::member_status_span)
                    .collect::<Vec<_>>()
                    .chunks(
                        width
                            .saturating_sub(Consts::FOCUSED_INDICATOR_SPAN.width() + 2)
                            .saturating_div(2),
                    )
                    .map(|x| {
                        std::iter::once(Consts::UNFOCUSED_INDICATOR_SPAN)
                            .chain(x.to_owned())
                            .collect::<Vec<_>>()
                            .into()
                    }),
            )
        }

        ret
    }

    fn get_delay_style(delay: u64) -> Style {
        match delay {
            0 => Consts::NO_LATENCY_STYLE,
            1..=200 => Consts::LOW_LATENCY_STYLE,
            201..=400 => Consts::MID_LATENCY_STYLE,
            401.. => Consts::HIGH_LATENCY_STYLE,
        }
    }

    fn get_delay_span(delay: u64) -> Span<'static> {
        match delay {
            0 => Consts::NO_LATENCY_SPAN,
            1..=200 => Consts::LOW_LATENCY_SPAN,
            201..=400 => Consts::MID_LATENCY_SPAN,
            401.. => Consts::HIGH_LATENCY_SPAN,
        }
    }

    fn member_status_span(member: &ProxyItem) -> Span<'static> {
        if let Some(ref history) = member.history {
            Self::get_delay_span(history.delay)
        } else if member.proxy_type.is_normal() {
            Consts::NO_LATENCY_SPAN
        } else {
            Consts::NOT_PROXY_SPAN
        }
    }
}

impl<'a> Default for ProxyGroup<'a> {
    fn default() -> Self {
        Self {
            members: vec![],
            current: None,
            proxy_type: ProxyType::Selector,
            name: String::new(),
            cursor: 0,
            _life: PhantomData,
        }
    }
}
