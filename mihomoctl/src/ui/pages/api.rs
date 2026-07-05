use tui::widgets::Widget;

use crate::{
    components::MovableList,
    define_widget,
    ui::api::ApiMenu,
};

define_widget!(ApiPage);

impl<'a> Widget for ApiPage<'a> {
    fn render(self, area: tui::layout::Rect, buf: &mut tui::buffer::Buffer) {
        let Some(api_state) = self.state.active_api_state() else {
            return;
        };
        if self.state.title() == "APIs" {
            MovableList::new(self.state.title(), api_state).render(area, buf);
        } else {
            ApiMenu::new(self.state.title(), api_state).render(area, buf);
        }
    }
}
