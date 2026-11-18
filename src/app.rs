use crate::{image_source, storage, wallpaper};
use eframe::egui::{self, Color32, ColorImage, RichText, TextureHandle};
use image::ImageReader;
use std::io::Cursor;
use std::sync::mpsc::{self, Receiver};
use std::thread;

const PREVIEW_MAX_WIDTH: u32 = 1_400;
const PREVIEW_MAX_HEIGHT: u32 = 820;

#[derive(Debug)]
enum BackgroundResult {
    Fetched(Result<image_source::DownloadedImage, String>),
    Applied(Result<(), String>),
}

pub struct WallpaperApp {
    paths: storage::AppPaths,
    source: image_source::Source,
    preview: Option<TextureHandle>,
    image: Option<image_source::DownloadedImage>,
    status: String,
    receiver: Option<Receiver<BackgroundResult>>,
    busy: bool,
    last_error: bool,
}

impl WallpaperApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&cc.egui_ctx);
        let paths = storage::AppPaths::discover().unwrap_or_else(|error| {
            eprintln!("Could not determine app data directory: {error:#}");
            storage::AppPaths::for_test(std::env::temp_dir().join("daily-wallpaper"))
        });
        let mut app = Self {
            paths,
            source: image_source::Source::Bing,
            preview: None,
            image: None,
            status: "Pick a source and bring a fresh view to your desktop.".into(),
            receiver: None,
            busy: false,
            last_error: false,
        };
        if let Ok(bytes) = std::fs::read(app.paths.current_image()) {
            if let Ok(preview) = decode_preview(&bytes) {
                app.preview = Some(cc.egui_ctx.load_texture(
                    "wallpaper-preview",
                    preview,
                    egui::TextureOptions::LINEAR,
                ));
                app.image = Some(image_source::DownloadedImage {
                    bytes,
                    title: "Last applied wallpaper".into(),
                    credit: "Local app cache".into(),
                    source: app.source,
                });
                app.status = "Showing the last wallpaper from your local cache.".into();
            }
        }
        app
    }

    fn start_fetch(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let source = self.source;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.busy = true;
        self.last_error = false;
        self.status = format!("Fetching {}…", source.label());
        let repaint = ctx.clone();
        thread::spawn(move || {
            let result = image_source::download(source).map_err(|error| format!("{error:#}"));
            let _ = sender.send(BackgroundResult::Fetched(result));
            repaint.request_repaint();
        });
    }

    fn start_apply(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(image) = self.image.clone() else {
            self.status = "Fetch an image before setting the wallpaper.".into();
            return;
        };
        let paths = self.paths.clone();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.busy = true;
        self.last_error = false;
        self.status = "Saving and applying wallpaper…".into();
        let repaint = ctx.clone();
        thread::spawn(move || {
            let result = paths
                .save_image(&image.bytes)
                .and_then(|path| wallpaper::set(&path))
                .map_err(|error| format!("{error:#}"));
            let _ = sender.send(BackgroundResult::Applied(result));
            repaint.request_repaint();
        });
    }

    fn receive_background_result(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.receiver.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.busy = false;
                match result {
                    BackgroundResult::Fetched(Ok(image)) => match decode_preview(&image.bytes) {
                        Ok(preview) => {
                            self.preview = Some(ctx.load_texture(
                                "wallpaper-preview",
                                preview,
                                egui::TextureOptions::LINEAR,
                            ));
                            self.status = format!("{} is ready to apply.", image.source.label());
                            self.image = Some(image);
                        }
                        Err(error) => self.set_error(format!("Could not decode image: {error:#}")),
                    },
                    BackgroundResult::Fetched(Err(error)) => {
                        self.set_error(format!("Could not fetch image: {error}"));
                    }
                    BackgroundResult::Applied(Ok(())) => {
                        self.status = "Wallpaper updated. Enjoy the new view.".into();
                    }
                    BackgroundResult::Applied(Err(error)) => {
                        self.set_error(format!("Could not set wallpaper: {error}"));
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.receiver = Some(receiver);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.busy = false;
                self.set_error("The background task stopped unexpectedly.".into());
            }
        }
    }

    fn set_error(&mut self, message: String) {
        self.status = message;
        self.last_error = true;
    }
}

impl eframe::App for WallpaperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_background_result(ctx);
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Daily Wallpaper").size(28.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("v0.2").small().color(Color32::from_gray(140)));
                });
            });
            ui.label(RichText::new("A calmer desktop, one fresh image at a time.").color(Color32::from_gray(170)));
            ui.add_space(20.0);

            egui::Frame::group(ui.style())
                .fill(Color32::from_rgb(29, 34, 43))
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("IMAGE SOURCE").small().strong().color(Color32::from_rgb(126, 177, 255)));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("source")
                            .width(ui.available_width() - 110.0)
                            .selected_text(self.source.label())
                            .show_ui(ui, |ui| {
                                for source in image_source::Source::ALL {
                                    if ui
                                        .selectable_value(&mut self.source, source, source.label())
                                        .changed()
                                    {
                                        self.preview = None;
                                        self.image = None;
                                        self.status = format!("Ready to fetch {}.", source.label());
                                    }
                                }
                            });
                        if ui.add_enabled(!self.busy, egui::Button::new("Refresh")).clicked() {
                            self.start_fetch(ctx);
                        }
                    });
                });

            ui.add_space(16.0);
            egui::Frame::group(ui.style())
                .fill(Color32::from_rgb(21, 25, 32))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    if let Some(texture) = &self.preview {
                        let available = ui.available_width();
                        let texture_size = texture.size_vec2();
                        let scale = (available / texture_size.x).min(1.0);
                        ui.add(egui::Image::new(texture).fit_to_exact_size(texture_size * scale));
                    } else {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 280.0),
                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| {
                                ui.label(RichText::new("Your next wallpaper will appear here").color(Color32::from_gray(130)));
                            },
                        );
                    }
                });

            ui.add_space(12.0);
            if let Some(image) = &self.image {
                ui.label(RichText::new(&image.title).strong());
                ui.label(RichText::new(&image.credit).small().color(Color32::from_gray(155)));
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if self.busy {
                    ui.spinner();
                }
                let button = if self.busy { "Working…" } else { "Set as wallpaper" };
                if ui
                    .add_enabled(!self.busy && self.image.is_some(), egui::Button::new(button).min_size(egui::vec2(150.0, 34.0)))
                    .clicked()
                {
                    self.start_apply(ctx);
                }
            });
            ui.add_space(12.0);
            let status_color = if self.last_error { Color32::from_rgb(255, 145, 145) } else { Color32::from_gray(175) };
            ui.label(RichText::new(&self.status).color(status_color));
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            ui.label(RichText::new("Bing daily is curated. Random photo uses Picsum Photos. Images are stored only in your local app-data cache when applied.").small().color(Color32::from_gray(125)));
        });
    }
}

fn configure_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_rounding = egui::Rounding::same(12.0);
    visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
    visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
    visuals.widgets.active.rounding = egui::Rounding::same(8.0);
    ctx.set_visuals(visuals);
}

fn decode_preview(bytes: &[u8]) -> anyhow::Result<ColorImage> {
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?
        .thumbnail(PREVIEW_MAX_WIDTH, PREVIEW_MAX_HEIGHT)
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, &image))
}
