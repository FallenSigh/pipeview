use egui::{ComboBox, DragValue, ScrollArea, TextEdit, Ui};
use xserial_client::config::{DecoderConfig, FramerConfig, PipelineConfig, SessionConfig};
use xserial_core::transport::TransportConfig;

// ── 表单数据结构（简化版，方便 UI 渲染）────────────────────────────

#[derive(Clone)]
pub struct ConfigForm {
    pub transport: TransportChoice,
    pub port: String,
    pub baud: u32,
    pub addr: String,
    pub udp_bind: String,
    pub udp_remote: String,
    pub pipelines: Vec<(String, FramerChoice, DecoderChoice)>,
    pub history: usize,
    pub auto_reconnect: bool,
}

#[derive(Clone, PartialEq)]
pub enum TransportChoice {
    Serial,
    Tcp,
    Udp,
}

#[derive(Clone, PartialEq)]
pub enum FramerChoice {
    Line,
    Fixed(usize),
}

#[derive(Clone, PartialEq)]
pub enum DecoderChoice {
    Text,
    Hex,
    Plot,
}

impl Default for ConfigForm {
    fn default() -> Self {
        Self {
            transport: TransportChoice::Tcp,
            port: "/dev/ttyUSB0".into(),
            baud: 115200,
            addr: "127.0.0.1:8080".into(),
            udp_bind: "0.0.0.0:9000".into(),
            udp_remote: String::new(),
            pipelines: vec![("text".into(), FramerChoice::Line, DecoderChoice::Text)],
            history: 10000,
            auto_reconnect: false,
        }
    }
}

impl ConfigForm {
    pub fn from_session_config(config: &SessionConfig) -> Self {
        Self {
            transport: match &config.transport {
                TransportConfig::Serial { .. } => TransportChoice::Serial,
                TransportConfig::Tcp { .. } => TransportChoice::Tcp,
                TransportConfig::Udp { .. } => TransportChoice::Udp,
            },
            port: match &config.transport {
                TransportConfig::Serial { port, .. } => port.clone(),
                _ => String::from("/dev/ttyUSB0"),
            },
            baud: match &config.transport {
                TransportConfig::Serial { baud_rate, .. } => *baud_rate,
                _ => 115200,
            },
            addr: match &config.transport {
                TransportConfig::Tcp { addr } => addr.clone(),
                _ => String::from("127.0.0.1:8080"),
            },
            udp_bind: match &config.transport {
                TransportConfig::Udp { bind_addr, .. } => bind_addr.clone(),
                _ => String::from("0.0.0.0:9000"),
            },
            udp_remote: match &config.transport {
                TransportConfig::Udp { remote_addr, .. } => remote_addr.clone().unwrap_or_default(),
                _ => String::new(),
            },
            pipelines: config
                .pipelines
                .iter()
                .map(|pipeline| {
                    let framer = match &pipeline.framer {
                        FramerConfig::Line { .. } => FramerChoice::Line,
                        FramerConfig::Fixed { frame_len } => FramerChoice::Fixed(*frame_len),
                        FramerConfig::Length { .. } | FramerConfig::Cobs { .. } => {
                            FramerChoice::Line
                        }
                    };
                    let decoder = match &pipeline.decoder {
                        DecoderConfig::Text { .. } => DecoderChoice::Text,
                        DecoderConfig::Hex { .. } => DecoderChoice::Hex,
                        DecoderConfig::Plot { .. } => DecoderChoice::Plot,
                    };
                    (pipeline.name.clone(), framer, decoder)
                })
                .collect(),
            history: config.history_limit,
            auto_reconnect: config.auto_reconnect,
        }
    }

    // 把表单数据转换成 xserial-client 的 SessionConfig
    pub fn to_session_config(&self) -> SessionConfig {
        SessionConfig {
            transport: match self.transport {
                TransportChoice::Serial => TransportConfig::Serial {
                    port: self.port.clone(),
                    baud_rate: self.baud,
                },
                TransportChoice::Tcp => TransportConfig::Tcp {
                    addr: self.addr.clone(),
                },
                TransportChoice::Udp => TransportConfig::Udp {
                    bind_addr: self.udp_bind.clone(),
                    remote_addr: if self.udp_remote.is_empty() {
                        None
                    } else {
                        Some(self.udp_remote.clone())
                    },
                },
            },
            pipelines: self
                .pipelines
                .iter()
                .map(|(name, fc, dc)| PipelineConfig {
                    name: name.clone(),
                    framer: match fc {
                        FramerChoice::Line => FramerConfig::Line {
                            strip_cr: true,
                            max_line_len: 1_048_576,
                        },
                        FramerChoice::Fixed(n) => FramerConfig::Fixed { frame_len: *n },
                    },
                    decoder: match dc {
                        DecoderChoice::Text => DecoderConfig::Text {
                            encoding: xserial_core::protocol::text::TextEncoding::Utf8,
                        },
                        DecoderChoice::Hex => DecoderConfig::Hex {
                            uppercase: false,
                            separator: " ".into(),
                            bytes_per_group: 1,
                            endian: xserial_core::protocol::Endian::Big,
                        },
                        DecoderChoice::Plot => DecoderConfig::Plot {
                            sample_type: xserial_core::protocol::plot::SampleType::F32,
                            endian: xserial_core::protocol::Endian::Little,
                            channels: 1,
                            format: xserial_core::protocol::plot::PlotFormat::Interleaved,
                        },
                    },
                })
                .collect(),
            history_limit: self.history,
            auto_reconnect: self.auto_reconnect,
        }
    }
}

// ── 渲染函数 ──────────────────────────────────────────────────────

pub fn render(ui: &mut Ui, form: &mut ConfigForm, submit_label: &str) -> Option<SessionConfig> {
    let mut result = None;

    ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Transport");

        // 传输类型下拉
        ComboBox::from_label("Type")
            .selected_text(match form.transport {
                TransportChoice::Serial => "Serial",
                TransportChoice::Tcp => "TCP",
                TransportChoice::Udp => "UDP",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut form.transport, TransportChoice::Serial, "Serial");
                ui.selectable_value(&mut form.transport, TransportChoice::Tcp, "TCP");
                ui.selectable_value(&mut form.transport, TransportChoice::Udp, "UDP");
            });

        // 根据类型显示不同字段
        match form.transport {
            TransportChoice::Serial => {
                ui.horizontal(|ui| {
                    ui.label("Port:");
                    ui.text_edit_singleline(&mut form.port);
                });
                ui.horizontal(|ui| {
                    ui.label("Baud:");
                    ui.add(DragValue::new(&mut form.baud).range(300..=12_000_000));
                });
            }
            TransportChoice::Tcp => {
                ui.horizontal(|ui| {
                    ui.label("Addr:");
                    ui.text_edit_singleline(&mut form.addr);
                });
            }
            TransportChoice::Udp => {
                ui.horizontal(|ui| {
                    ui.label("Bind:");
                    ui.text_edit_singleline(&mut form.udp_bind);
                });
                ui.horizontal(|ui| {
                    ui.label("Remote:");
                    ui.text_edit_singleline(&mut form.udp_remote);
                });
            }
        }

        ui.separator();
        ui.heading("Pipelines");

        let mut remove = None;

        // 管道列表
        for (i, (name, fc, dc)) in form.pipelines.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(TextEdit::singleline(name).desired_width(60.0));

                    // Framer 选择
                    let fsel = match fc {
                        FramerChoice::Line => "Line".to_string(),
                        FramerChoice::Fixed(n) => format!("Fixed({})", n),
                    };
                    ComboBox::from_id_salt(format!("f{}", i))
                        .selected_text(fsel)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(false, "Line").clicked() {
                                *fc = FramerChoice::Line;
                            }
                            if ui.selectable_label(false, "Fixed").clicked() {
                                *fc = FramerChoice::Fixed(8);
                            }
                        });
                    if let FramerChoice::Fixed(n) = fc {
                        ui.add(DragValue::new(n).range(1..=65536));
                    }

                    // Decoder 选择
                    ComboBox::from_id_salt(format!("d{}", i))
                        .selected_text(match dc {
                            DecoderChoice::Text => "Text",
                            DecoderChoice::Hex => "Hex",
                            DecoderChoice::Plot => "Plot",
                        })
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(false, "Text").clicked() {
                                *dc = DecoderChoice::Text;
                            }
                            if ui.selectable_label(false, "Hex").clicked() {
                                *dc = DecoderChoice::Hex;
                            }
                            if ui.selectable_label(false, "Plot").clicked() {
                                *dc = DecoderChoice::Plot;
                            }
                        });

                    if ui.button("✕").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }

        if let Some(i) = remove {
            form.pipelines.remove(i);
        }

        if ui.button("+ Add Pipeline").clicked() {
            form.pipelines.push((
                format!("p{}", form.pipelines.len()),
                FramerChoice::Line,
                DecoderChoice::Text,
            ));
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("History:");
            ui.add(DragValue::new(&mut form.history).range(100..=1_000_000));
        });
        ui.checkbox(&mut form.auto_reconnect, "Auto reconnect");

        ui.separator();
        if ui.button(submit_label).clicked() {
            result = Some(form.to_session_config());
        }
    });

    result
}
