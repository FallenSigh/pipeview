use egui::{ComboBox, DragValue, ScrollArea, TextEdit, Ui};
use xserial_client::config::{DecoderConfig, FramerConfig, PipelineConfig, SessionConfig};
use xserial_core::protocol::Endian;
use xserial_core::protocol::plot::{PlotFormat, SampleType};
use xserial_core::protocol::text::TextEncoding;
use xserial_core::transport::TransportConfig;
use xserial_core::transport::serial::{
    SerialDataBits, SerialFlowControl, SerialParity, SerialStopBits, SerialTransport,
};

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
    pub line_strip_cr: bool,
    pub line_max_line_len: usize,
    pub fixed_frame_len: usize,
    pub length_len_bytes: usize,
    pub length_endian: Endian,
    pub length_includes_self: bool,
    pub length_max_payload: usize,
    pub cobs_max_frame: usize,
    pub mixed_strip_cr: bool,
    pub mixed_max_line_len: usize,
    pub mixed_max_plot_frame: usize,
    pub hex_uppercase: bool,
    pub hex_separator: String,
    pub hex_bytes_per_group: usize,
    pub hex_endian: Endian,
    pub plot_sample_type: SampleType,
    pub plot_channels: usize,
    pub plot_endian: Endian,
    pub plot_format: PlotFormat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TransportChoice {
    Serial,
    Tcp,
    Udp,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FramerChoice {
    Line,
    Fixed,
    Length,
    Cobs,
    MixedTextPlot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DecoderChoice {
    Text,
    Hex,
    Plot,
    MixedTextPlot,
}

impl PipelineForm {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            framer: FramerChoice::Line,
            decoder: DecoderChoice::Text,
            text_encoding: TextEncoding::Utf8,
            line_strip_cr: true,
            line_max_line_len: 1_048_576,
            fixed_frame_len: 256,
            length_len_bytes: 2,
            length_endian: Endian::Big,
            length_includes_self: false,
            length_max_payload: 1_048_576,
            cobs_max_frame: 1_048_576,
            mixed_strip_cr: true,
            mixed_max_line_len: 1_048_576,
            mixed_max_plot_frame: 1_048_576,
            hex_uppercase: false,
            hex_separator: String::from(" "),
            hex_bytes_per_group: 1,
            hex_endian: Endian::Big,
            plot_sample_type: SampleType::F32,
            plot_channels: 1,
            plot_endian: Endian::Little,
            plot_format: PlotFormat::Interleaved,
        }
    }
}

impl Default for ConfigForm {
    fn default() -> Self {
        Self {
            transport: TransportChoice::Tcp,
            port: String::from("/dev/ttyUSB0"),
            baud: 115200,
            serial_data_bits: SerialDataBits::Eight,
            serial_parity: SerialParity::None,
            serial_stop_bits: SerialStopBits::One,
            serial_flow_control: SerialFlowControl::None,
            addr: String::from("127.0.0.1:8080"),
            udp_bind: String::from("0.0.0.0:9000"),
            udp_remote: String::new(),
            pipelines: vec![PipelineForm::new("text")],
            history: 10_000,
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
                .map(PipelineForm::from_pipeline_config)
                .collect(),
            history: config.history_limit,
            auto_reconnect: config.auto_reconnect,
        }
    }

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
                    remote_addr: if self.udp_remote.trim().is_empty() {
                        None
                    } else {
                        Some(self.udp_remote.clone())
                    },
                },
            },
            pipelines: self
                .pipelines
                .iter()
                .map(PipelineForm::to_pipeline_config)
                .collect(),
            history_limit: self.history,
            auto_reconnect: self.auto_reconnect,
        }
    }
}

impl PipelineForm {
    fn from_pipeline_config(config: &PipelineConfig) -> Self {
        let mut form = Self::new(config.name.clone());

        match &config.framer {
            FramerConfig::Line {
                strip_cr,
                max_line_len,
            } => {
                form.framer = FramerChoice::Line;
                form.line_strip_cr = *strip_cr;
                form.line_max_line_len = *max_line_len;
            }
            FramerConfig::Fixed { frame_len } => {
                form.framer = FramerChoice::Fixed;
                form.fixed_frame_len = *frame_len;
            }
            FramerConfig::Length {
                len_bytes,
                endian,
                length_includes_self,
                max_payload,
            } => {
                form.framer = FramerChoice::Length;
                form.length_len_bytes = *len_bytes;
                form.length_endian = *endian;
                form.length_includes_self = *length_includes_self;
                form.length_max_payload = *max_payload;
            }
            FramerConfig::Cobs { max_frame } => {
                form.framer = FramerChoice::Cobs;
                form.cobs_max_frame = *max_frame;
            }
            FramerConfig::MixedTextPlot {
                strip_cr,
                max_line_len,
                max_plot_frame,
            } => {
                form.framer = FramerChoice::MixedTextPlot;
                form.mixed_strip_cr = *strip_cr;
                form.mixed_max_line_len = *max_line_len;
                form.mixed_max_plot_frame = *max_plot_frame;
            }
        }

        match &config.decoder {
            DecoderConfig::Text { encoding } => {
                form.decoder = DecoderChoice::Text;
                form.text_encoding = *encoding;
            }
            DecoderConfig::Hex {
                uppercase,
                separator,
                bytes_per_group,
                endian,
            } => {
                form.decoder = DecoderChoice::Hex;
                form.hex_uppercase = *uppercase;
                form.hex_separator = separator.clone();
                form.hex_bytes_per_group = *bytes_per_group;
                form.hex_endian = *endian;
            }
            DecoderConfig::Plot {
                sample_type,
                endian,
                channels,
                format,
            } => {
                form.decoder = DecoderChoice::Plot;
                form.plot_sample_type = *sample_type;
                form.plot_endian = *endian;
                form.plot_channels = *channels;
                form.plot_format = *format;
            }
            DecoderConfig::MixedTextPlot { encoding } => {
                form.decoder = DecoderChoice::MixedTextPlot;
                form.text_encoding = *encoding;
            }
        }

        form
    }

    fn to_pipeline_config(&self) -> PipelineConfig {
        PipelineConfig {
            name: self.name.clone(),
            framer: match self.framer {
                FramerChoice::Line => FramerConfig::Line {
                    strip_cr: self.line_strip_cr,
                    max_line_len: self.line_max_line_len,
                },
                FramerChoice::Fixed => FramerConfig::Fixed {
                    frame_len: self.fixed_frame_len,
                },
                FramerChoice::Length => FramerConfig::Length {
                    len_bytes: self.length_len_bytes,
                    endian: self.length_endian,
                    length_includes_self: self.length_includes_self,
                    max_payload: self.length_max_payload,
                },
                FramerChoice::Cobs => FramerConfig::Cobs {
                    max_frame: self.cobs_max_frame,
                },
                FramerChoice::MixedTextPlot => FramerConfig::MixedTextPlot {
                    strip_cr: self.mixed_strip_cr,
                    max_line_len: self.mixed_max_line_len,
                    max_plot_frame: self.mixed_max_plot_frame,
                },
            },
            decoder: match self.decoder {
                DecoderChoice::Text => DecoderConfig::Text {
                    encoding: self.text_encoding,
                },
                DecoderChoice::Hex => DecoderConfig::Hex {
                    uppercase: self.hex_uppercase,
                    separator: self.hex_separator.clone(),
                    bytes_per_group: self.hex_bytes_per_group.max(1),
                    endian: self.hex_endian,
                },
                DecoderChoice::Plot => DecoderConfig::Plot {
                    sample_type: self.plot_sample_type,
                    endian: self.plot_endian,
                    channels: self.plot_channels.max(1),
                    format: self.plot_format,
                },
                DecoderChoice::MixedTextPlot => DecoderConfig::MixedTextPlot {
                    encoding: self.text_encoding,
                },
            },
        }
    }
}

pub fn render(ui: &mut Ui, form: &mut ConfigForm, submit_label: &str) -> Option<SessionConfig> {
    let mut result = None;
    let mut validation_errors = Vec::new();

    ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Transport");
        render_transport_section(ui, form);

        ui.separator();
        ui.heading("Pipelines");

        let mut remove = None;
        for (index, pipeline) in form.pipelines.iter_mut().enumerate() {
            ui.group(|ui| {
                render_pipeline_header(ui, pipeline, index, &mut remove);
                render_framer_settings(ui, pipeline, index);
                render_decoder_settings(ui, pipeline, index);
                validate_pipeline(ui, pipeline, &mut validation_errors);
            });
        }

        if let Some(index) = remove {
            form.pipelines.remove(index);
        }

        if ui.button("+ Add Pipeline").clicked() {
            form.pipelines
                .push(PipelineForm::new(format!("p{}", form.pipelines.len())));
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
        if ui.button(submit_label).clicked() && validation_errors.is_empty() {
            result = Some(form.to_session_config());
        }
    });

    result
}

fn render_transport_section(ui: &mut Ui, form: &mut ConfigForm) {
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

    match form.transport {
        TransportChoice::Serial => {
            render_serial_port_combo(ui, form);
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
                        ui.selectable_value(&mut form.serial_data_bits, SerialDataBits::Five, "5");
                        ui.selectable_value(&mut form.serial_data_bits, SerialDataBits::Six, "6");
                        ui.selectable_value(&mut form.serial_data_bits, SerialDataBits::Seven, "7");
                        ui.selectable_value(&mut form.serial_data_bits, SerialDataBits::Eight, "8");
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
                        ui.selectable_value(&mut form.serial_parity, SerialParity::None, "None");
                        ui.selectable_value(&mut form.serial_parity, SerialParity::Odd, "Odd");
                        ui.selectable_value(&mut form.serial_parity, SerialParity::Even, "Even");
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
                        ui.selectable_value(&mut form.serial_stop_bits, SerialStopBits::One, "1");
                        ui.selectable_value(&mut form.serial_stop_bits, SerialStopBits::Two, "2");
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
}

fn render_serial_port_combo(ui: &mut Ui, form: &mut ConfigForm) {
    let available_ports = available_serial_ports();
    let selected_text = selected_serial_port_text(&form.port, &available_ports);

    ui.horizontal(|ui| {
        ui.label("Port:");
        ComboBox::from_id_salt("serial_port")
            .width(220.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                if available_ports.is_empty() {
                    ui.add_enabled(false, egui::Label::new("No serial ports found"));
                } else {
                    for port in &available_ports {
                        ui.selectable_value(&mut form.port, port.clone(), port);
                    }
                }

                let current_port = form.port.clone();
                if !current_port.trim().is_empty()
                    && !available_ports.iter().any(|port| port == &current_port)
                {
                    ui.separator();
                    ui.selectable_value(
                        &mut form.port,
                        current_port.clone(),
                        format!("{current_port} (current)"),
                    );
                }
            });
    });
}

fn available_serial_ports() -> Vec<String> {
    let mut ports: Vec<_> = SerialTransport::list_ports()
        .into_iter()
        .map(|port| port.port_name)
        .collect();
    ports.sort();
    ports.dedup();
    ports
}

fn selected_serial_port_text(current_port: &str, available_ports: &[String]) -> String {
    if current_port.trim().is_empty() {
        if available_ports.is_empty() {
            String::from("No serial ports found")
        } else {
            String::from("Select port")
        }
    } else {
        current_port.to_owned()
    }
}

fn render_pipeline_header(
    ui: &mut Ui,
    pipeline: &mut PipelineForm,
    index: usize,
    remove: &mut Option<usize>,
) {
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.add(TextEdit::singleline(&mut pipeline.name).desired_width(90.0));

        ComboBox::from_id_salt(format!("framer_{index}"))
            .selected_text(match pipeline.framer {
                FramerChoice::Line => "Line",
                FramerChoice::Fixed => "Fixed",
                FramerChoice::Length => "Length",
                FramerChoice::Cobs => "Cobs",
                FramerChoice::MixedTextPlot => "MixedTextPlot",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut pipeline.framer, FramerChoice::Line, "Line");
                ui.selectable_value(&mut pipeline.framer, FramerChoice::Fixed, "Fixed");
                ui.selectable_value(&mut pipeline.framer, FramerChoice::Length, "Length");
                ui.selectable_value(&mut pipeline.framer, FramerChoice::Cobs, "Cobs");
                if ui
                    .selectable_label(
                        matches!(pipeline.framer, FramerChoice::MixedTextPlot),
                        "MixedTextPlot",
                    )
                    .clicked()
                {
                    pipeline.framer = FramerChoice::MixedTextPlot;
                    pipeline.decoder = DecoderChoice::MixedTextPlot;
                }
            });

        ComboBox::from_id_salt(format!("decoder_{index}"))
            .selected_text(match pipeline.decoder {
                DecoderChoice::Text => "Text",
                DecoderChoice::Hex => "Hex",
                DecoderChoice::Plot => "Plot",
                DecoderChoice::MixedTextPlot => "MixedTextPlot",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut pipeline.decoder, DecoderChoice::Text, "Text");
                ui.selectable_value(&mut pipeline.decoder, DecoderChoice::Hex, "Hex");
                if ui
                    .selectable_label(matches!(pipeline.decoder, DecoderChoice::Plot), "Plot")
                    .clicked()
                {
                    if matches!(pipeline.framer, FramerChoice::Line) {
                        pipeline.framer = FramerChoice::Fixed;
                    }
                    pipeline.decoder = DecoderChoice::Plot;
                }
                if ui
                    .selectable_label(
                        matches!(pipeline.decoder, DecoderChoice::MixedTextPlot),
                        "MixedTextPlot",
                    )
                    .clicked()
                {
                    pipeline.framer = FramerChoice::MixedTextPlot;
                    pipeline.decoder = DecoderChoice::MixedTextPlot;
                }
            });

        if ui.button("Delete").clicked() {
            *remove = Some(index);
        }
    });
}

fn render_framer_settings(ui: &mut Ui, pipeline: &mut PipelineForm, index: usize) {
    ui.separator();
    ui.label("Framer settings");

    match pipeline.framer {
        FramerChoice::Line => {
            ui.checkbox(&mut pipeline.line_strip_cr, "Strip CR");
            ui.horizontal(|ui| {
                ui.label("Max line length:");
                ui.add(DragValue::new(&mut pipeline.line_max_line_len).range(1..=16_777_216));
            });
        }
        FramerChoice::Fixed => {
            ui.horizontal(|ui| {
                ui.label("Frame length:");
                ui.add(DragValue::new(&mut pipeline.fixed_frame_len).range(1..=16_777_216));
            });
        }
        FramerChoice::Length => {
            ui.horizontal(|ui| {
                ui.label("Length bytes:");
                ComboBox::from_id_salt(format!("length_len_bytes_{index}"))
                    .selected_text(pipeline.length_len_bytes.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut pipeline.length_len_bytes, 1, "1");
                        ui.selectable_value(&mut pipeline.length_len_bytes, 2, "2");
                        ui.selectable_value(&mut pipeline.length_len_bytes, 4, "4");
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Length endian:");
                render_endian_combo(
                    ui,
                    &mut pipeline.length_endian,
                    format!("length_endian_{index}"),
                );
            });
            ui.checkbox(
                &mut pipeline.length_includes_self,
                "Length field includes header bytes",
            );
            ui.horizontal(|ui| {
                ui.label("Max payload:");
                ui.add(DragValue::new(&mut pipeline.length_max_payload).range(0..=16_777_216));
            });
        }
        FramerChoice::Cobs => {
            ui.horizontal(|ui| {
                ui.label("Max frame:");
                ui.add(DragValue::new(&mut pipeline.cobs_max_frame).range(1..=16_777_216));
            });
        }
        FramerChoice::MixedTextPlot => {
            ui.checkbox(&mut pipeline.mixed_strip_cr, "Strip CR");
            ui.horizontal(|ui| {
                ui.label("Max text line:");
                ui.add(DragValue::new(&mut pipeline.mixed_max_line_len).range(1..=16_777_216));
            });
            ui.horizontal(|ui| {
                ui.label("Max plot frame:");
                ui.add(DragValue::new(&mut pipeline.mixed_max_plot_frame).range(1..=16_777_216));
            });
        }
    }
}

fn render_decoder_settings(ui: &mut Ui, pipeline: &mut PipelineForm, index: usize) {
    ui.separator();
    ui.label("Decoder settings");

    match pipeline.decoder {
        DecoderChoice::Text => {
            ui.horizontal(|ui| {
                ui.label("Text encoding:");
                render_text_encoding_combo(
                    ui,
                    &mut pipeline.text_encoding,
                    format!("text_encoding_{index}"),
                );
            });
        }
        DecoderChoice::Hex => {
            ui.checkbox(&mut pipeline.hex_uppercase, "Uppercase output");
            ui.horizontal(|ui| {
                ui.label("Separator:");
                ui.text_edit_singleline(&mut pipeline.hex_separator);
            });
            ui.horizontal(|ui| {
                ui.label("Bytes per group:");
                ui.add(DragValue::new(&mut pipeline.hex_bytes_per_group).range(1..=32));
            });
            ui.horizontal(|ui| {
                ui.label("Hex endian:");
                render_endian_combo(ui, &mut pipeline.hex_endian, format!("hex_endian_{index}"));
            });
        }
        DecoderChoice::Plot => {
            ui.horizontal(|ui| {
                ui.label("Sample type:");
                render_sample_type_combo(
                    ui,
                    &mut pipeline.plot_sample_type,
                    format!("plot_sample_type_{index}"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Plot endian:");
                render_endian_combo(
                    ui,
                    &mut pipeline.plot_endian,
                    format!("plot_endian_{index}"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Channels:");
                ui.add(DragValue::new(&mut pipeline.plot_channels).range(1..=64));
            });
            ui.horizontal(|ui| {
                ui.label("Plot format:");
                render_plot_format_combo(
                    ui,
                    &mut pipeline.plot_format,
                    format!("plot_format_{index}"),
                );
            });
            if matches!(pipeline.plot_format, PlotFormat::XY) {
                pipeline.plot_channels = 2;
            }
        }
        DecoderChoice::MixedTextPlot => {
            ui.horizontal(|ui| {
                ui.label("Mixed text encoding:");
                render_text_encoding_combo(
                    ui,
                    &mut pipeline.text_encoding,
                    format!("mixed_text_encoding_{index}"),
                );
            });
            ui.small(
                "Plot sample type, endian, format, and channels come from the mixed packet header.",
            );
        }
    }
}

fn validate_pipeline(ui: &mut Ui, pipeline: &PipelineForm, errors: &mut Vec<String>) {
    let mut push_error = |message: String| {
        ui.colored_label(egui::Color32::RED, &message);
        errors.push(format!("pipeline '{}': {}", pipeline.name, message));
    };

    if matches!(pipeline.framer, FramerChoice::MixedTextPlot)
        && !matches!(pipeline.decoder, DecoderChoice::MixedTextPlot)
    {
        push_error(String::from(
            "MixedTextPlot framer requires MixedTextPlot decoder",
        ));
    }

    if matches!(pipeline.decoder, DecoderChoice::MixedTextPlot)
        && !matches!(pipeline.framer, FramerChoice::MixedTextPlot)
    {
        push_error(String::from(
            "MixedTextPlot decoder requires MixedTextPlot framer",
        ));
    }

    if matches!(pipeline.decoder, DecoderChoice::Plot) {
        if matches!(
            pipeline.framer,
            FramerChoice::Line | FramerChoice::MixedTextPlot
        ) {
            push_error(String::from(
                "Raw Plot decoder requires Fixed, Length, or Cobs framing",
            ));
        }

        if matches!(pipeline.plot_format, PlotFormat::XY) && pipeline.plot_channels != 2 {
            push_error(String::from("XY plot requires exactly 2 channels"));
        }

        if matches!(pipeline.framer, FramerChoice::Fixed) {
            let channel_count = if matches!(pipeline.plot_format, PlotFormat::XY) {
                2
            } else {
                pipeline.plot_channels.max(1)
            };
            let sample_bytes = pipeline.plot_sample_type.byte_size();
            let frame_unit = channel_count.saturating_mul(sample_bytes);
            if frame_unit == 0 || pipeline.fixed_frame_len % frame_unit != 0 {
                push_error(format!(
                    "Fixed frame length must be a multiple of {} bytes for the current plot layout",
                    frame_unit
                ));
            }
        }
    }

    if matches!(pipeline.framer, FramerChoice::Length)
        && !matches!(pipeline.length_len_bytes, 1 | 2 | 4)
    {
        push_error(String::from("Length framer len_bytes must be 1, 2, or 4"));
    }
}

fn render_text_encoding_combo(ui: &mut Ui, encoding: &mut TextEncoding, id: impl std::hash::Hash) {
    ComboBox::from_id_salt(id)
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

fn render_sample_type_combo(ui: &mut Ui, sample_type: &mut SampleType, id: impl std::hash::Hash) {
    ComboBox::from_id_salt(id)
        .selected_text(sample_type.name())
        .show_ui(ui, |ui| {
            for option in [
                SampleType::I8,
                SampleType::U8,
                SampleType::I16,
                SampleType::U16,
                SampleType::I32,
                SampleType::U32,
                SampleType::I64,
                SampleType::U64,
                SampleType::F32,
                SampleType::F64,
            ] {
                ui.selectable_value(sample_type, option, option.name());
            }
        });
}

fn render_plot_format_combo(ui: &mut Ui, plot_format: &mut PlotFormat, id: impl std::hash::Hash) {
    ComboBox::from_id_salt(id)
        .selected_text(match plot_format {
            PlotFormat::Interleaved => "Interleaved",
            PlotFormat::Block => "Block",
            PlotFormat::XY => "XY",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(plot_format, PlotFormat::Interleaved, "Interleaved");
            ui.selectable_value(plot_format, PlotFormat::Block, "Block");
            ui.selectable_value(plot_format, PlotFormat::XY, "XY");
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_form_roundtrip_preserves_expanded_pipeline_settings() {
        let session = SessionConfig {
            transport: TransportConfig::Tcp {
                addr: String::from("127.0.0.1:9001"),
            },
            pipelines: vec![
                PipelineConfig {
                    name: String::from("hex"),
                    framer: FramerConfig::Length {
                        len_bytes: 4,
                        endian: Endian::Little,
                        length_includes_self: true,
                        max_payload: 8192,
                    },
                    decoder: DecoderConfig::Hex {
                        uppercase: true,
                        separator: String::from(":"),
                        bytes_per_group: 2,
                        endian: Endian::Little,
                    },
                },
                PipelineConfig {
                    name: String::from("plot"),
                    framer: FramerConfig::Cobs { max_frame: 4096 },
                    decoder: DecoderConfig::Plot {
                        sample_type: SampleType::I16,
                        endian: Endian::Big,
                        channels: 3,
                        format: PlotFormat::Block,
                    },
                },
            ],
            history_limit: 4096,
            auto_reconnect: true,
        };

        let form = ConfigForm::from_session_config(&session);
        let rebuilt = form.to_session_config();

        assert_eq!(
            serde_json::to_string(&rebuilt).unwrap(),
            serde_json::to_string(&session).unwrap()
        );
    }

    #[test]
    fn selected_serial_port_text_prefers_current_value() {
        let ports = vec![String::from("/dev/ttyUSB0")];
        assert_eq!(
            selected_serial_port_text("/dev/ttyUSB1", &ports),
            "/dev/ttyUSB1"
        );
    }

    #[test]
    fn selected_serial_port_text_uses_placeholder_when_empty() {
        assert_eq!(selected_serial_port_text("", &[]), "No serial ports found");
        assert_eq!(
            selected_serial_port_text("", &[String::from("/dev/ttyUSB0")]),
            "Select port"
        );
    }
}
