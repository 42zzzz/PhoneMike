use std::sync::mpsc::Sender;

use eframe::egui::{self, Color32, Panel, RichText, ScrollArea, Shape, Stroke, StrokeKind, Vec2};

use crate::state::{AppStateHandle, Command, ConnectionStatus};
use crate::update::CURRENT_VERSION;

pub struct PhoneMikeApp {
    state: AppStateHandle,
    cmd_tx: Sender<Command>,

    // UI-local state
    gain: f32,
    noise_gate: f32,
    lowpass_hz: f32,
    wav_recording: bool,
    wav_path: String,
    driver_enabled: bool,

    // Cached snapshot (updated once per frame)
    cached_status: ConnectionStatus,
    cached_rms: f32,
    cached_rms_history: Vec<f32>,
    cached_bytes_recv: u64,
    cached_bytes_dropped: u64,
    cached_elapsed: f64,
    cached_shm_wi: i32,
    cached_shm_ri: i32,
    cached_driver_active: bool,
    cached_gate_active: bool,
    cached_log: Vec<String>,
    cached_update: Option<String>,
}

impl PhoneMikeApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        state: AppStateHandle,
        cmd_tx: Sender<Command>,
    ) -> Self {
        PhoneMikeApp {
            state,
            cmd_tx,
            gain: 1.0,
            noise_gate: 0.0,
            lowpass_hz: 24000.0, // default: effectively bypass
            wav_recording: false,
            wav_path: "output.wav".to_string(),
            driver_enabled: true,
            cached_status: ConnectionStatus::Disconnected,
            cached_rms: 0.0,
            cached_rms_history: Vec::new(),
            cached_bytes_recv: 0,
            cached_bytes_dropped: 0,
            cached_elapsed: 0.0,
            cached_shm_wi: 0,
            cached_shm_ri: 0,
            cached_driver_active: false,
            cached_gate_active: false,
            cached_log: Vec::new(),
            cached_update: None,
        }
    }

    fn snapshot_state(&mut self) {
        if let Ok(st) = self.state.try_lock() {
            self.cached_status = st.status.clone();
            self.cached_rms = st.stats.rms;
            self.cached_rms_history = st.stats.rms_history.iter().copied().collect();
            self.cached_bytes_recv = st.stats.bytes_received;
            self.cached_bytes_dropped = st.stats.bytes_dropped;
            self.cached_elapsed = st.stats.elapsed_secs;
            self.cached_shm_wi = st.stats.shm_write_idx;
            self.cached_shm_ri = st.stats.shm_read_idx;
            self.cached_driver_active = st.stats.driver_active;
            self.cached_gate_active = st.stats.gate_active;
            self.cached_log = st.log.iter().cloned().collect();
            self.cached_update = st.update_available.clone();
        }
    }

    fn send_cmd(&self, cmd: Command) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (dot_color, status_text) = match &self.cached_status {
                ConnectionStatus::Disconnected => (Color32::RED, "Disconnected".to_string()),
                ConnectionStatus::Connecting => (Color32::YELLOW, "Connecting...".to_string()),
                ConnectionStatus::Streaming { sample_rate, channels } => (
                    Color32::GREEN,
                    format!("Streaming \u{2014} {}Hz / {}ch", sample_rate, channels),
                ),
                ConnectionStatus::Error(e) => (Color32::from_rgb(255, 128, 0), format!("Error: {e}")),
            };

            let (response, painter) = ui.allocate_painter(Vec2::splat(14.0), egui::Sense::hover());
            painter.circle_filled(response.rect.center(), 6.0, dot_color);

            ui.label(RichText::new(&status_text).strong());
            ui.separator();
            ui.label("Transport: ADB/TCP");

            if let Some(ref tag) = self.cached_update.clone() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let url = format!(
                        "https://github.com/42zzzz/PhoneMike/releases/tag/{tag}"
                    );
                    if ui.link(RichText::new("Download").color(Color32::from_rgb(80, 180, 255))).clicked() {
                        let _ = std::process::Command::new("cmd")
                            .args(["/c", "start", "", &url])
                            .spawn();
                    }
                    ui.colored_label(
                        Color32::from_rgb(255, 200, 50),
                        format!("\u{25B2} Update {tag} available \u{2014} {CURRENT_VERSION} installed"),
                    );
                });
            }
        });
    }

    fn render_left_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Controls");
        ui.separator();

        let is_active = self.cached_status.is_active();

        let btn_label = if is_active { "Stop" } else { "Start" };
        let btn = ui.add_sized(
            [ui.available_width(), 36.0],
            egui::Button::new(RichText::new(btn_label).size(18.0)),
        );
        if btn.clicked() {
            if is_active {
                self.send_cmd(Command::Stop);
            } else {
                self.send_cmd(Command::Start {
                    use_driver: self.driver_enabled,
                    wav_path: if self.wav_recording {
                        Some(self.wav_path.clone())
                    } else {
                        None
                    },
                    gain: self.gain,
                    noise_gate: self.noise_gate,
                    lowpass_hz: self.lowpass_hz,
                });
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("Output").strong());

        ui.add_enabled_ui(!is_active, |ui| {
            ui.checkbox(&mut self.driver_enabled, "Write to kernel driver")
                .on_hover_text("Stream audio into shared memory so the PhoneMike virtual microphone driver picks it up.");
        });

        let wav_changed = ui.checkbox(&mut self.wav_recording, "Record WAV")
            .on_hover_text("Save raw PCM to a .wav file.")
            .changed();
        ui.add_enabled_ui(self.wav_recording, |ui| {
            ui.horizontal(|ui| {
                ui.label("File:");
                ui.text_edit_singleline(&mut self.wav_path);
            });
        });

        if wav_changed && is_active {
            if self.wav_recording {
                self.send_cmd(Command::StartWav(self.wav_path.clone()));
            } else {
                self.send_cmd(Command::StopWav);
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("Gain").strong());

        let gain_changed = ui
            .add(egui::Slider::new(&mut self.gain, 0.0..=4.0).text("x"))
            .changed();
        if gain_changed && is_active {
            self.send_cmd(Command::SetGain(self.gain));
        }

        ui.add_space(8.0);
        ui.separator();

        // Noise gate header with active indicator
        ui.horizontal(|ui| {
            ui.label(RichText::new("Noise Gate").strong());
            if self.cached_gate_active {
                ui.colored_label(Color32::from_rgb(255, 180, 0), "[GATED]");
            }
        });
        ui.label(RichText::new("0 = off").small().color(Color32::from_gray(140)));

        let gate_changed = ui
            .add(egui::Slider::new(&mut self.noise_gate, 0.0..=0.3)
                .step_by(0.005)
                .text("rms"))
            .changed();
        if gate_changed && is_active {
            self.send_cmd(Command::SetNoiseGate(self.noise_gate));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("Lowpass Filter").strong());
        ui.label(RichText::new("24000 = off (Nyquist)").small().color(Color32::from_gray(140)));

        let lpf_changed = ui
            .add(egui::Slider::new(&mut self.lowpass_hz, 500.0..=24000.0)
                .step_by(100.0)
                .text("Hz"))
            .changed();
        if lpf_changed && is_active {
            self.send_cmd(Command::SetLowpass(self.lowpass_hz));
        }
    }

    fn render_center(&mut self, ui: &mut egui::Ui) {
        ui.heading("Level");
        let available_width = ui.available_width();

        let meter_height = 24.0;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(available_width, meter_height),
            egui::Sense::hover(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, Color32::from_gray(40));

        let fill_width = rect.width() * self.cached_rms.min(1.0);
        let filled = egui::Rect::from_min_size(rect.min, Vec2::new(fill_width, rect.height()));
        let bar_color = if self.cached_gate_active {
            Color32::from_gray(80) // grey when gated
        } else if self.cached_rms < 0.7 {
            Color32::from_rgb(50, 200, 80)
        } else if self.cached_rms < 0.9 {
            Color32::YELLOW
        } else {
            Color32::RED
        };
        painter.rect_filled(filled, 3.0, bar_color);
        painter.rect_stroke(rect, 3.0, Stroke::new(1.0, Color32::from_gray(100)), StrokeKind::Outside);

        // Noise gate threshold line overlay
        if self.noise_gate > 0.0 {
            let gx = rect.left() + rect.width() * self.noise_gate.min(1.0);
            let gate_line_color = Color32::from_rgb(255, 200, 50);
            painter.line_segment(
                [egui::pos2(gx, rect.top()), egui::pos2(gx, rect.bottom())],
                Stroke::new(1.5, gate_line_color),
            );
        }

        ui.add_space(12.0);
        ui.separator();
        ui.heading("Level History");

        let graph_height = 140.0;
        let (rect_g, _) = ui.allocate_exact_size(
            Vec2::new(available_width, graph_height),
            egui::Sense::hover(),
        );
        let gp = ui.painter_at(rect_g);
        gp.rect_filled(rect_g, 3.0, Color32::from_gray(25));

        if self.cached_rms_history.len() >= 2 {
            let n = self.cached_rms_history.len();
            let points: Vec<egui::Pos2> = self
                .cached_rms_history
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let x = rect_g.left() + (i as f32 / (n - 1).max(1) as f32) * rect_g.width();
                    let y = rect_g.bottom() - v.clamp(0.0, 1.0) * rect_g.height();
                    egui::pos2(x, y)
                })
                .collect();
            gp.add(Shape::line(
                points,
                Stroke::new(1.5, Color32::from_rgb(50, 200, 80)),
            ));
        }

        // Gate threshold line on history graph
        if self.noise_gate > 0.0 {
            let gy = rect_g.bottom() - self.noise_gate.min(1.0) * rect_g.height();
            gp.line_segment(
                [egui::pos2(rect_g.left(), gy), egui::pos2(rect_g.right(), gy)],
                Stroke::new(1.0, Color32::from_rgb(255, 200, 50)),
            );
        }

        gp.text(
            egui::pos2(rect_g.left() + 2.0, rect_g.top() + 2.0),
            egui::Align2::LEFT_TOP,
            "1.0",
            egui::FontId::proportional(10.0),
            Color32::from_gray(100),
        );
        gp.text(
            egui::pos2(rect_g.left() + 2.0, rect_g.bottom() - 2.0),
            egui::Align2::LEFT_BOTTOM,
            "0.0",
            egui::FontId::proportional(10.0),
            Color32::from_gray(100),
        );
    }

    fn render_stats_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Stats");
        ui.separator();

        let recv_kb = self.cached_bytes_recv as f64 / 1024.0;
        let drop_kb = self.cached_bytes_dropped as f64 / 1024.0;
        let drop_pct = if self.cached_bytes_recv > 0 {
            self.cached_bytes_dropped as f64 / self.cached_bytes_recv as f64 * 100.0
        } else {
            0.0
        };

        egui::Grid::new("stats_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Elapsed:");
                ui.label(format!("{:.1}s", self.cached_elapsed));
                ui.end_row();

                ui.label("Received:");
                ui.label(format!("{:.1} KB", recv_kb));
                ui.end_row();

                ui.label("Dropped:");
                ui.label(format!("{:.1} KB ({:.1}%)", drop_kb, drop_pct));
                ui.end_row();

                ui.label("RMS:");
                ui.label(format!("{:.3}", self.cached_rms));
                ui.end_row();

                ui.label("Gate:");
                ui.colored_label(
                    if self.cached_gate_active { Color32::from_rgb(255, 180, 0) } else { Color32::from_gray(128) },
                    if self.cached_gate_active { "Closed" } else { "Open" },
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.separator();
        ui.label(RichText::new("Driver / Shared Mem").strong());

        let driver_color = if self.cached_driver_active {
            Color32::GREEN
        } else {
            Color32::from_gray(128)
        };
        ui.colored_label(
            driver_color,
            if self.cached_driver_active { "Active" } else { "Inactive" },
        );

        egui::Grid::new("shm_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("WriteIdx:");
                ui.label(format!("{}", self.cached_shm_wi));
                ui.end_row();
                ui.label("ReadIdx:");
                ui.label(format!("{}", self.cached_shm_ri));
                ui.end_row();
                ui.label("Delta:");
                ui.label(format!("{}", self.cached_shm_wi.wrapping_sub(self.cached_shm_ri)));
                ui.end_row();
            });
    }

    fn render_log_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Log");
        ui.separator();
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.cached_log {
                    ui.label(RichText::new(line).monospace().size(11.0));
                }
            });
    }
}

impl eframe::App for PhoneMikeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.snapshot_state();

        if self.cached_status.is_active() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        }

        Panel::top("top_bar").show_inside(ui, |ui| {
            ui.add_space(4.0);
            self.render_top_bar(ui);
            ui.add_space(4.0);
        });

        Panel::bottom("log_panel")
            .resizable(true)
            .min_size(100.0)
            .default_size(160.0)
            .show_inside(ui, |ui| {
                self.render_log_panel(ui);
            });

        Panel::left("controls_panel")
            .resizable(false)
            .exact_size(240.0)
            .show_inside(ui, |ui| {
                self.render_left_panel(ui);
            });

        Panel::right("stats_panel")
            .resizable(true)
            .default_size(180.0)
            .min_size(150.0)
            .show_inside(ui, |ui| {
                self.render_stats_panel(ui);
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.render_center(ui);
        });
    }
}
