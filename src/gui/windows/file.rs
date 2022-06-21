use std::collections::HashSet;
use std::path::{Path, PathBuf};

use std::str::FromStr;

use anyhow::Result;

use crate::reactor::{ModalError, ModalSuccess};

#[derive(Debug, Clone)]
pub struct FilePicker {
    id: egui::Id,

    pub pwd: PathBuf,
    pub current_dir: PathBuf,
    pub current_dir_text: String,

    pub highlighted_dir: Option<PathBuf>,
    pub selected_path: Option<PathBuf>,

    pub dir_list: Vec<PathBuf>,
    pub history: Vec<PathBuf>,

    pub extensions: HashSet<String>,
}

impl FilePicker {
    pub fn new<P: AsRef<Path>>(id: egui::Id, pwd: P) -> Result<Self> {
        let pwd = pwd.as_ref().to_owned();
        let current_dir = pwd.clone();
        let current_dir_text = current_dir.as_os_str().to_str().unwrap();
        let current_dir_text = current_dir_text.to_owned();

        let mut result = Self {
            id,

            pwd,
            current_dir,
            current_dir_text,

            highlighted_dir: None,
            selected_path: None,

            dir_list: Vec::new(),
            history: Vec::new(),

            extensions: HashSet::default(),
        };

        result.load_current_dir()?;

        Ok(result)
    }

    pub fn set_visible_extensions(
        &mut self,
        extensions: &[&str],
    ) -> Result<()> {
        let extensions = extensions.iter().map(|s| s.to_string()).collect();
        self.extensions = extensions;
        self.load_current_dir()?;
        Ok(())
    }

    pub fn selected_path(&self) -> Option<&Path> {
        let path = self.selected_path.as_ref()?;
        Some(path.as_ref())
    }

    pub fn reset_selection(&mut self) {
        self.selected_path = None;
        self.highlighted_dir = None;
    }

    pub fn reset(&mut self) {
        self.current_dir.clone_from(&self.pwd);
        self.selected_path = None;
        self.dir_list.clear();
        self.history.clear();
    }

    fn load_current_dir(&mut self) -> Result<()> {
        let current_dir_text = self.current_dir.as_os_str().to_str().unwrap();
        self.current_dir_text = current_dir_text.to_owned();

        self.selected_path = None;
        self.dir_list.clear();

        let dirs = std::fs::read_dir(&self.current_dir)?;

        for dir in dirs {
            let entry = dir?;
            let path = entry.path();

            if self.extensions.is_empty()
                || self.extensions.iter().any(|ext| {
                    if path.is_dir() {
                        return true;
                    }

                    if let Some(file_ext) =
                        path.extension().and_then(|f_ext| f_ext.to_str())
                    {
                        file_ext == ext
                    } else {
                        false
                    }
                })
            {
                self.dir_list.push(path);
            }
        }

        self.dir_list.sort();

        Ok(())
    }

    pub fn goto_dir<P: AsRef<Path>>(
        &mut self,
        new_dir: P,
        add_history: bool,
    ) -> Result<()> {
        let new_dir = new_dir.as_ref();

        if add_history {
            self.history.push(self.current_dir.clone());
        }

        self.current_dir = new_dir.to_owned();
        self.load_current_dir()?;

        Ok(())
    }

    fn goto_prev(&mut self) -> Result<()> {
        if let Some(new_dir) = self.history.pop() {
            self.goto_dir(new_dir, false)?;
        }
        Ok(())
    }

    fn go_up(&mut self) -> Result<()> {
        if let Some(parent) = self.current_dir.parent().map(|p| p.to_owned()) {
            self.goto_dir(parent, true)?;
        }
        Ok(())
    }

    fn goto_path_in_text_box(&mut self) -> Result<()> {
        let path = PathBuf::from_str(&self.current_dir_text)?;

        if path.exists() && path.is_dir() {
            self.goto_dir(path, true)?;
        }

        Ok(())
    }

    pub fn ui_impl(
        &mut self,
        ui: &mut egui::Ui,
        force_accept: bool,
    ) -> std::result::Result<ModalSuccess, ModalError> {
        let max_height = ui.input().screen_rect.height() - 100.0;
        /*
        ui.set_max_height(max_height);
        */

        ui.horizontal(|ui| {
            let text_box = ui.text_edit_singleline(&mut self.current_dir_text);

            if ui.button("Goto").clicked()
                || (text_box.lost_focus()
                        // || (text_box.has_focus()
                            && ui.input().key_pressed(egui::Key::Enter))
            {
                self.goto_path_in_text_box().unwrap();
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Prev").clicked() {
                self.goto_prev().unwrap();
            }

            if ui.button("Up").clicked() {
                self.go_up().unwrap();
            }
        });

        let mut goto_dir: Option<PathBuf> = None;

        let mut choose_path: Option<PathBuf> = None;

        egui::ScrollArea::from_max_height(max_height - 100.0).show(
            ui,
            |mut ui| {
                egui::Grid::new("file_list").striped(true).show(
                    &mut ui,
                    |ui| {
                        for dir_path in self.dir_list.iter() {
                            if let Some(name) =
                                dir_path.file_name().and_then(|n| n.to_str())
                            {
                                let checked = if let Some(sel_name) =
                                    &self.highlighted_dir
                                {
                                    sel_name == dir_path
                                } else {
                                    false
                                };
                                let row = ui.selectable_label(checked, name);

                                if row.clicked() {
                                    self.highlighted_dir =
                                        Some(dir_path.clone());
                                }

                                if row.double_clicked() {
                                    if dir_path.is_dir() {
                                        goto_dir = Some(dir_path.to_owned());
                                    } else if dir_path.is_file() {
                                        choose_path = Some(dir_path.to_owned());
                                    }
                                }

                                ui.end_row();
                            }
                        }
                    },
                );
            },
        );

        if force_accept {
            if let Some(dir_path) = self.highlighted_dir.as_ref() {
                if dir_path.is_dir() {
                    goto_dir = Some(dir_path.to_owned());
                } else if dir_path.is_file() {
                    choose_path = Some(dir_path.to_owned());
                }
            }
        }

        if let Some(dir) = goto_dir {
            self.goto_dir(&dir, true).unwrap();
            ui.scroll_to_cursor(egui::Align::TOP);
        }

        if let Some(path) = choose_path {
            self.selected_path = Some(path);
            return Ok(ModalSuccess::Success);
        }

        Err(ModalError::Continue)
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        egui::Window::new("File picker")
            .id(self.id)
            .collapsible(false)
            .open(open)
            .show(ctx, |ui| {
                let max_height = ui.input().screen_rect.height() - 100.0;

                ui.set_max_height(max_height);

                let _ = self.ui_impl(ui, false);

                if ui.button("Ok").clicked() {
                    self.selected_path = self.highlighted_dir.clone();
                }
            })
    }
}
