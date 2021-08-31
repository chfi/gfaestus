pub struct SlotList<T> {
    // display: Box<for<'a> FnMut(&'a egui::Ui, T) -> egui::Response>
    display: Box<dyn Fn(&mut egui::Ui, &T) -> egui::Response>,

    offset: usize,
    slot_count: usize,

    indices: Vec<usize>,
}

impl<T> SlotList<T> {
    pub fn new<F>(slot_count: usize, display: F) -> Self
    where
        F: Fn(&mut egui::Ui, &T) -> egui::Response + 'static,
    {
        Self {
            display: Box::new(display),

            offset: 0,
            slot_count,

            indices: Vec::new(),
        }
    }

    pub fn display(&self, ui: &mut egui::Ui, value: &T) -> egui::Response {
        let function = &self.display;
        function(ui, value)
    }

    pub fn ui_list(
        &self,
        ui: &mut egui::Ui,
        values: &[T],
    ) -> Vec<egui::Response> {
        let res: Vec<egui::Response> = (0..self.slot_count)
            .filter_map(|ix| {
                let val = if self.indices.is_empty() {
                    values.get(self.offset + ix)
                } else {
                    let ix = self.indices.get(self.offset + ix)?;
                    values.get(*ix)
                }?;

                Some((&self.display)(ui, val))
            })
            .collect();

        res
    }
}

/// Creates a popup that, unlike the built-in egui one, doesn't
/// disappear when the user clicks inside the popup
pub fn popup_below_widget(
    ui: &egui::Ui,
    popup_id: egui::Id,
    widget_response: &egui::Response,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    if ui.memory().is_popup_open(popup_id) {
        let parent_clip_rect = ui.clip_rect();

        let popup_response = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(widget_response.rect.left_bottom())
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(parent_clip_rect); // for when the combo-box is in a scroll area.
                let frame = egui::Frame::popup(ui.style());
                let frame_margin = frame.margin;
                frame.show(ui, |ui| {
                    ui.with_layout(
                        egui::Layout::top_down_justified(egui::Align::LEFT),
                        |ui| {
                            ui.set_width(
                                widget_response.rect.width()
                                    - 2.0 * frame_margin.x,
                            );
                            add_contents(ui)
                        },
                    );
                });
            });

        let popup_response = popup_response.response;

        if ui.input().key_pressed(egui::Key::Escape)
            || (popup_response.clicked_elsewhere()
                && widget_response.clicked_elsewhere())
        {
            ui.memory().close_popup();
        }
    }
}
