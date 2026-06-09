use egui::{ComboBox, DragValue, ScrollArea, TextEdit, Ui};
use xserial_client::config::{DecoderConfig, FramerConfig, PipelineConfig, SessionConfig};
use xserial_core::protocol::Endian;
use xserial_core::protocol::plot::PlotFormat;
use xserial_core::protocol::text::TextEncoding;
use xserial_core::transport::TransportConfig;
use xserial_core::transport::serial::{
    SerialDataBits, SerialFlowControl, SerialParity, SerialStopBits,
};

// ── 表单数据结构（简化版，方便 UI 渲染）────────────────────────────

#[derive(Clone)]
pub struct ConfigForm {
    pub transport: TransportChoice,
    pub port: String,
    pub baud: u32,
    pub serial_data_bits: SerialDataBits,
    pub serial_parity: SerialParity,
    pub serial_stop_bits: SerialStopBits,
    pub serial_flow_control: SerialFlowControl,
    pub addr: String,
    pub udp_bind: String,
    pub udp_remote: String,
    pub pipelines: Vec<PipelineForm>,
    pub history: usize,
    pub auto_reconnect: bool,
}

#[derive(Clone)]
pub struct PipelineForm {
    pub name: String,
    pub framer: FramerChoice,
    pub decoder: DecoderChoice,
    pub text_encoding: TextEncoding,
    pub hex_endian: Endian,
    pub plot_channels: usize,
    pub plot_endian: Endian,
    pub plot_format: PlotFormat,
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
    MixedTextPlot,
}

#[derive(Clone, PartialEq)]
pub enum DecoderChoice {
    Text,
    Hex,
    Plot,
    MixedTextPlot,
}

impl Default for ConfigForm {
    fn default() -> Self {
        Self {
            transport: TransportChoice::Tcp,
            port: "/dev/ttyUSB0".into(),
            baud: 115200,
            serial_data_bits: SerialDataBits::Eight,
            serial_parity: SerialParity::None,
            serial_stop_bits: SerialStopBits::One,
            serial_flow_control: SerialFlowControl::None,
            addr: "127.0.0.1:8080".into(),
            udp_bind: "0.0.0.0:9000".into(),
            udp_remote: String::new(),
            pipelines: vec![PipelineForm {
                name: "text".into(),
                framer: FramerChoice::Line,
                decoder: DecoderChoice::Text,
                text_encoding: TextEncoding::Utf8,
                hex_endian: Endian::Big,
                plot_channels: 1,
                plot_endian: Endian::Little,
                plot_format: PlotFormat::Interleaved,
            }],
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
            serial_data_bits: match &config.transport {
                TransportConfig::Serial { data_bits, .. } => *data_bits,
                _ => SerialDataBits::Eight,
            },
            serial_parity: match &config.transport {
                TransportConfig::Serial { parity, .. } => *parity,
                _ => SerialParity::None,
            },
            serial_stop_bits: match &config.transport {
                TransportConfig::Serial { stop_bits, .. } => *stop_bits,
                _ => SerialStopBits::One,
            },
            serial_flow_control: match &config.transport {
                TransportConfig::Serial { flow_control, .. } => *flow_control,
                _ => SerialFlowControl::None,
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
                        FramerConfig::MixedTextPlot { .. } => FramerChoice::MixedTextPlot,
                        FramerConfig::Length { .. } | FramerConfig::Cobs { .. } => {
                            FramerChoice::Line
                        }
                    };
                    let decoder = match &pipeline.decoder {
                        DecoderConfig::Text { .. } => DecoderChoice::Text,
                        DecoderConfig::Hex { .. } => DecoderChoice::Hex,
                        DecoderConfig::Plot { .. } => DecoderChoice::Plot,
                        DecoderConfig::MixedTextPlot { .. } => DecoderChoice::MixedTextPlot,
                    };
                    let plot_channels = match &pipeline.decoder {
                        DecoderConfig::Plot { channels, .. } => *channels,
                        _ => 1,
                    };
                    let hex_endian = match &pipeline.decoder {
                        DecoderConfig::Hex { endian, .. } => *endian,
                        _ => Endian::Big,
                    };
                    let plot_endian = match &pipeline.decoder {
                        DecoderConfig::Plot { endian, .. } => *endian,
                        _ => Endian::Little,
                    };
                    let text_encoding = match &pipeline.decoder {
                        DecoderConfig::Text { encoding } => *encoding,
                        DecoderConfig::MixedTextPlot { encoding } => *encoding,
                        _ => TextEncoding::Utf8,
                    };
                    let plot_format = match &pipeline.decoder {
                        DecoderConfig::Plot { format, .. } => *format,
                        _ => PlotFormat::Interleaved,
                    };
                    PipelineForm {
                        name: pipeline.name.clone(),
                        framer,
                        decoder,
                        text_encoding,
                        hex_endian,
                        plot_channels,
                        plot_endian,
                        plot_format,
                    }
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
                    data_bits: self.serial_data_bits,
                    parity: self.serial_parity,
                    stop_bits: self.serial_stop_bits,
                    flow_control: self.serial_flow_control,
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
                .map(|pipeline| PipelineConfig {
                    name: pipeline.name.clone(),
                    framer: match &pipeline.framer {
                        FramerChoice::Line => FramerConfig::Line {
                            strip_cr: true,
                            max_line_len: 1_048_576,
                        },
                        FramerChoice::Fixed(n) => FramerConfig::Fixed { frame_len: *n },
                        FramerChoice::MixedTextPlot => FramerConfig::MixedTextPlot {
                            strip_cr: true,
                            max_line_len: 1_048_576,
                            max_plot_frame: 1_048_576,
                        },
                    },
                    decoder: match &pipeline.decoder {
                        DecoderChoice::Text => DecoderConfig::Text {
                            encoding: pipeline.text_encoding,
                        },
                        DecoderChoice::Hex => DecoderConfig::Hex {
                            uppercase: false,
                            separator: " ".into(),
                            bytes_per_group: 1,
                            endian: pipeline.hex_endian,
                        },
                        DecoderChoice::Plot => DecoderConfig::Plot {
                            sample_type: xserial_core::protocol::plot::SampleType::F32,
                            endian: pipeline.plot_endian,
                            channels: pipeline.plot_channels.max(1),
                            format: pipeline.plot_format,
                        },
                        DecoderChoice::MixedTextPlot => DecoderConfig::MixedTextPlot {
                            encoding: pipeline.text_encoding,
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
    let mut validation_errors = Vec::new();

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
                ui.horizontal(|ui| {
                    ui.label("Data bits:");
                    ComboBox::from_id_salt("serial_data_bits")
                        .selected_text(match form.serial_data_bits {
                            SerialDataBits::Five => "5",
                            SerialDataBits::Six => "6",
                            SerialDataBits::Seven => "7",
                            SerialDataBits::Eight => "8",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut form.serial_data_bits,
                                SerialDataBits::Five,
                                "5",
                            );
                            ui.selectable_value(
                                &mut form.serial_data_bits,
                                SerialDataBits::Six,
                                "6",
                            );
                            ui.selectable_value(
                                &mut form.serial_data_bits,
                                SerialDataBits::Seven,
                                "7",
                            );
                            ui.selectable_value(
                                &mut form.serial_data_bits,
                                SerialDataBits::Eight,
                                "8",
                            );
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Parity:");
                    ComboBox::from_id_salt("serial_parity")
                        .selected_text(match form.serial_parity {
                            SerialParity::None => "None",
                            SerialParity::Odd => "Odd",
                            SerialParity::Even => "Even",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut form.serial_parity,
                                SerialParity::None,
                                "None",
                            );
                            ui.selectable_value(&mut form.serial_parity, SerialParity::Odd, "Odd");
                            ui.selectable_value(
                                &mut form.serial_parity,
                                SerialParity::Even,
                                "Even",
                            );
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Stop bits:");
                    ComboBox::from_id_salt("serial_stop_bits")
                        .selected_text(match form.serial_stop_bits {
                            SerialStopBits::One => "1",
                            SerialStopBits::Two => "2",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut form.serial_stop_bits,
                                SerialStopBits::One,
                                "1",
                            );
                            ui.selectable_value(
                                &mut form.serial_stop_bits,
                                SerialStopBits::Two,
                                "2",
                            );
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Flow:");
                    ComboBox::from_id_salt("serial_flow_control")
                        .selected_text(match form.serial_flow_control {
                            SerialFlowControl::None => "None",
                            SerialFlowControl::Software => "Software",
                            SerialFlowControl::Hardware => "Hardware",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut form.serial_flow_control,
                                SerialFlowControl::None,
                                "None",
                            );
                            ui.selectable_value(
                                &mut form.serial_flow_control,
                                SerialFlowControl::Software,
                                "Software",
                            );
                            ui.selectable_value(
                                &mut form.serial_flow_control,
                                SerialFlowControl::Hardware,
                                "Hardware",
                            );
                        });
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
        for (i, pipeline) in form.pipelines.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(TextEdit::singleline(&mut pipeline.name).desired_width(60.0));

                    // Framer 选择
                    let fsel = match &pipeline.framer {
                        FramerChoice::Line => "Line".to_string(),
                        FramerChoice::Fixed(n) => format!("Fixed({})", n),
                        FramerChoice::MixedTextPlot => "MixedTextPlot".to_string(),
                    };
                    ComboBox::from_id_salt(format!("f{}", i))
                        .selected_text(fsel)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(false, "Line").clicked() {
                                pipeline.framer = FramerChoice::Line;
                            }
                            if ui.selectable_label(false, "Fixed").clicked() {
                                pipeline.framer = FramerChoice::Fixed(8);
                            }
                            if ui.selectable_label(false, "MixedTextPlot").clicked() {
                                pipeline.framer = FramerChoice::MixedTextPlot;
                                pipeline.decoder = DecoderChoice::MixedTextPlot;
                            }
                        });
                    if let FramerChoice::Fixed(n) = &mut pipeline.framer {
                        ui.add(DragValue::new(n).range(1..=65536));
                    }

                    // Decoder 选择
                    ComboBox::from_id_salt(format!("d{}", i))
                        .selected_text(match pipeline.decoder {
                            DecoderChoice::Text => "Text",
                            DecoderChoice::Hex => "Hex",
                            DecoderChoice::Plot => "Plot",
                            DecoderChoice::MixedTextPlot => "MixedTextPlot",
                        })
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(false, "Text").clicked() {
                                pipeline.decoder = DecoderChoice::Text;
                            }
                            if ui.selectable_label(false, "Hex").clicked() {
                                pipeline.decoder = DecoderChoice::Hex;
                            }
                            if ui.selectable_label(false, "Plot").clicked() {
                                if matches!(pipeline.framer, FramerChoice::Line) {
                                    pipeline.framer = FramerChoice::Fixed(256);
                                }
                                pipeline.decoder = DecoderChoice::Plot;
                            }
                            if ui.selectable_label(false, "MixedTextPlot").clicked() {
                                pipeline.framer = FramerChoice::MixedTextPlot;
                                pipeline.decoder = DecoderChoice::MixedTextPlot;
                            }
                        });

                    if ui.button("✕").clicked() {
                        remove = Some(i);
                    }
                });

                if matches!(pipeline.decoder, DecoderChoice::Plot) {
                    ui.horizontal(|ui| {
                        ui.label("Plot endian:");
                        render_endian_combo(ui, &mut pipeline.plot_endian, format!("plot_endian{}", i));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Plot channels:");
                        ui.add(DragValue::new(&mut pipeline.plot_channels).range(1..=32));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Plot format:");
                        ComboBox::from_id_salt(format!("plot_format{}", i))
                            .selected_text(match pipeline.plot_format {
                                PlotFormat::Interleaved => "Interleaved",
                                PlotFormat::Block => "Block",
                                PlotFormat::XY => "XY",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut pipeline.plot_format,
                                    PlotFormat::Interleaved,
                                    "Interleaved",
                                );
                                ui.selectable_value(
                                    &mut pipeline.plot_format,
                                    PlotFormat::Block,
                                    "Block",
                                );
                                ui.selectable_value(
                                    &mut pipeline.plot_format,
                                    PlotFormat::XY,
                                    "XY",
                                );
                            });
                    });

                    if matches!(pipeline.plot_format, PlotFormat::XY) {
                        pipeline.plot_channels = 2;
                    }

                    match &pipeline.framer {
                        FramerChoice::Line => {
                            let message = "Plot decoder requires Fixed framer for raw binary samples";
                            ui.colored_label(egui::Color32::RED, message);
                            validation_errors
                                .push(format!("pipeline '{}': {message}", pipeline.name));
                        }
                        FramerChoice::Fixed(n)
                            if *n % (pipeline.plot_channels.max(1) * 4) != 0 =>
                        {
                            let message = format!(
                                "Plot decoder currently expects interleaved f32 data, so frame length must be a multiple of {} bytes",
                                pipeline.plot_channels.max(1) * 4
                            );
                            ui.colored_label(egui::Color32::RED, &message);
                            validation_errors
                                .push(format!("pipeline '{}': {message}", pipeline.name));
                        }
                        FramerChoice::MixedTextPlot => {
                            let message = "Raw Plot decoder cannot be used with MixedTextPlot framer";
                            ui.colored_label(egui::Color32::RED, message);
                            validation_errors
                                .push(format!("pipeline '{}': {message}", pipeline.name));
                        }
                        FramerChoice::Fixed(_) => {}
                    }

                    if matches!(pipeline.plot_format, PlotFormat::XY) && pipeline.plot_channels != 2
                    {
                        let message = "XY plot requires exactly 2 channels";
                        ui.colored_label(egui::Color32::RED, message);
                        validation_errors.push(format!(
                            "pipeline '{}': {message}",
                            pipeline.name
                        ));
                    }
                }

                if matches!(pipeline.decoder, DecoderChoice::Hex) {
                    ui.horizontal(|ui| {
                        ui.label("Hex endian:");
                        render_endian_combo(ui, &mut pipeline.hex_endian, format!("hex_endian{}", i));
                    });
                }

                if matches!(pipeline.decoder, DecoderChoice::Text) {
                    ui.horizontal(|ui| {
                        ui.label("Text encoding:");
                        render_text_encoding_combo(ui, &mut pipeline.text_encoding, i);
                    });
                }

                if matches!(pipeline.decoder, DecoderChoice::MixedTextPlot) {
                    ui.horizontal(|ui| {
                        ui.label("Mixed text encoding:");
                        render_text_encoding_combo(ui, &mut pipeline.text_encoding, i);
                    });
                    ui.small("Plot sample type, endian, format, and channels come from the mixed packet header.");
                }

                if matches!(pipeline.decoder, DecoderChoice::MixedTextPlot)
                    && !matches!(pipeline.framer, FramerChoice::MixedTextPlot)
                {
                    let message = "MixedTextPlot decoder requires MixedTextPlot framer";
                    ui.colored_label(egui::Color32::RED, message);
                    validation_errors.push(format!("pipeline '{}': {message}", pipeline.name));
                }
            });
        }

        if let Some(i) = remove {
            form.pipelines.remove(i);
        }

        if ui.button("+ Add Pipeline").clicked() {
            form.pipelines.push(PipelineForm {
                name: format!("p{}", form.pipelines.len()),
                framer: FramerChoice::Line,
                decoder: DecoderChoice::Text,
                text_encoding: TextEncoding::Utf8,
                hex_endian: Endian::Big,
                plot_channels: 1,
                plot_endian: Endian::Little,
                plot_format: PlotFormat::Interleaved,
            });
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("History:");
            ui.add(DragValue::new(&mut form.history).range(100..=1_000_000));
        });
        ui.checkbox(&mut form.auto_reconnect, "Auto reconnect");

        ui.separator();
        for error in &validation_errors {
            ui.colored_label(egui::Color32::RED, error);
        }
        if ui.button(submit_label).clicked() {
            if validation_errors.is_empty() {
                result = Some(form.to_session_config());
            }
        }
    });

    result
}

fn render_text_encoding_combo(ui: &mut Ui, encoding: &mut TextEncoding, index: usize) {
    ComboBox::from_id_salt(format!("text_encoding{}", index))
        .selected_text(match encoding {
            TextEncoding::Utf8 => "UTF-8",
            TextEncoding::Latin1 => "Latin1",
            TextEncoding::Ascii => "ASCII",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(encoding, TextEncoding::Utf8, "UTF-8");
            ui.selectable_value(encoding, TextEncoding::Latin1, "Latin1");
            ui.selectable_value(encoding, TextEncoding::Ascii, "ASCII");
        });
}

fn render_endian_combo(ui: &mut Ui, endian: &mut Endian, id: impl std::hash::Hash) {
    ComboBox::from_id_salt(id)
        .selected_text(match endian {
            Endian::Little => "Little",
            Endian::Big => "Big",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(endian, Endian::Little, "Little");
            ui.selectable_value(endian, Endian::Big, "Big");
        });
}
