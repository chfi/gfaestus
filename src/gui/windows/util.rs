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
