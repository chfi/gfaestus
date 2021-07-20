use std::fs::{
    DirBuilder, DirEntry, File, FileType, Metadata, OpenOptions, Permissions,
    ReadDir,
};
use std::path::{Path, PathBuf};

use std::ffi::{OsStr, OsString};
use std::str::FromStr;
use std::sync::Arc;

use crossbeam::atomic::AtomicCell;

use anyhow::Result;

pub struct FilePicker {
    id: egui::Id,

    pwd: PathBuf,
    current_dir: PathBuf,
    current_dir_text: String,

    highlighted_dir: Option<PathBuf>,
    selected_path: Option<PathBuf>,

    dir_list: Vec<DirEntry>,
    history: Vec<PathBuf>,
}

impl FilePicker {
    pub fn new<P: AsRef<Path>>(id: egui::Id, pwd: P) -> Self {
        let pwd = pwd.as_ref().to_owned();
        let current_dir = pwd.clone();
        let current_dir_text = current_dir.as_os_str().to_str().unwrap();
        let current_dir_text = current_dir_text.to_owned();

        Self {
            id,

            pwd,
            current_dir,
            current_dir_text,

            highlighted_dir: None,
            selected_path: None,

            dir_list: Vec::new(),
            history: Vec::new(),
        }
    }

    fn reset(&mut self) {
        self.current_dir.clone_from(&self.pwd);
        self.selected_path = None;
        self.dir_list.clear();
        self.history.clear();
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
        let current_dir_text = self.current_dir.as_os_str().to_str().unwrap();
        self.current_dir_text = current_dir_text.to_owned();

        self.selected_path = None;
        self.dir_list.clear();

        let dirs = std::fs::read_dir(new_dir)?;

        for dir in dirs {
            let entry = dir?;
            self.dir_list.push(entry);
        }

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

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        open: &mut bool,
        // path_dst: Arc<AtomicCell<PathBuf>>,
    ) -> Option<egui::Response> {
        egui::Window::new("File picker")
            .id(self.id)
            .collapsible(false)
            .open(open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.current_dir_text);
                    if ui.button("Goto").clicked() {
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

                egui::ScrollArea::auto_sized().show(ui, |mut ui| {
                    egui::Grid::new("file_list").striped(true).show(
                        &mut ui,
                        |ui| {
                            let mut goto_dir: Option<PathBuf> = None;

                            let mut choose_path: Option<PathBuf> = None;

                            for dir in self.dir_list.iter() {
                                let dir_path = dir.path();
                                if let Some(name) = dir.file_name().to_str() {
                                    let checked = if let Some(sel_name) =
                                        &self.highlighted_dir
                                    {
                                        sel_name == &dir_path
                                    } else {
                                        false
                                    };
                                    let row =
                                        ui.selectable_label(checked, name);

                                    if row.clicked() {
                                        self.highlighted_dir =
                                            Some(dir_path.clone());
                                    }

                                    if row.double_clicked() {
                                        if dir_path.is_dir() {
                                            goto_dir = Some(dir_path);
                                        } else if dir_path.is_file() {
                                            choose_path = Some(dir_path);
                                        }
                                    }

                                    ui.end_row();
                                }
                            }

                            if let Some(dir) = goto_dir {
                                self.goto_dir(&dir, true).unwrap();
                            }

                            if let Some(path) = choose_path {
                                self.selected_path = Some(path);
                            }
                        },
                    );
                })
            })
    }
}
