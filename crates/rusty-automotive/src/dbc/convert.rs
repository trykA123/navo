use can_dbc::{
    AttributeValue, Comment as LibraryComment, MessageId, NumericValue, SignalExtendedValueType,
    ValueDescription,
};

use super::{
    Comment, CommentTarget, DbcModel, Message, MultiplexRange, Multiplexing, Number, Signal,
    SignalFormat, Value, ValueTable, ValueTarget,
};
use crate::{CanId, Endian};

pub(super) fn id(value: MessageId) -> CanId {
    match value {
        MessageId::Standard(raw) => CanId {
            raw: u32::from(raw),
            extended: false,
        },
        MessageId::Extended(raw) => CanId {
            raw,
            extended: true,
        },
    }
}

fn number(value: NumericValue) -> Number {
    match value {
        NumericValue::Uint(n) => Number::Unsigned(n),
        NumericValue::Int(n) => Number::Signed(n),
        NumericValue::Double(n) => Number::Float(n),
    }
}

fn signal(value: can_dbc::Signal) -> Signal {
    Signal {
        name: value.name,
        start_bit: value.start_bit,
        length: value.size,
        byte_order: match value.byte_order {
            can_dbc::ByteOrder::LittleEndian => Endian::Little,
            can_dbc::ByteOrder::BigEndian => Endian::Big,
        },
        signed: value.value_type == can_dbc::ValueType::Signed,
        format: SignalFormat::Integer,
        factor: value.factor,
        offset: value.offset,
        min: number(value.min),
        max: number(value.max),
        unit: value.unit,
        receivers: value.receivers,
        multiplexing: match value.multiplexer_indicator {
            can_dbc::MultiplexIndicator::Plain => Multiplexing::Plain,
            can_dbc::MultiplexIndicator::Multiplexor => Multiplexing::Switch,
            can_dbc::MultiplexIndicator::MultiplexedSignal(n) => Multiplexing::Value(n),
            can_dbc::MultiplexIndicator::MultiplexorAndMultiplexedSignal(n) => {
                Multiplexing::SwitchValue(n)
            }
        },
    }
}

fn comment(value: LibraryComment) -> Comment {
    let (target, text) = match value {
        LibraryComment::Plain { comment } => (CommentTarget::File, comment),
        LibraryComment::Node { name, comment } => (CommentTarget::Node(name), comment),
        LibraryComment::Message {
            id: message,
            comment,
        } => (CommentTarget::Message(id(message)), comment),
        LibraryComment::Signal {
            message_id,
            name,
            comment,
        } => (
            CommentTarget::Signal {
                message: id(message_id),
                signal: name,
            },
            comment,
        ),
        LibraryComment::EnvVar { name, comment } => (CommentTarget::Environment(name), comment),
    };
    Comment { target, text }
}

fn values(values: Vec<can_dbc::ValDescription>) -> Vec<Value> {
    values
        .into_iter()
        .map(|value| Value {
            value: value.id,
            label: value.description,
        })
        .collect()
}

fn cycle(value: AttributeValue) -> Option<f64> {
    let number = match value {
        AttributeValue::Uint(n) => n as f64,
        AttributeValue::Int(n) => n as f64,
        AttributeValue::Double(n) => n,
        AttributeValue::String(_) => return None,
    };
    (number.is_finite() && number >= 0.0).then_some(number)
}

impl DbcModel {
    pub(super) fn take(
        &mut self,
        parsed: can_dbc::Dbc,
        current: &mut Option<usize>,
        context: (u64, bool),
    ) {
        let (line, _) = context;
        self.nodes
            .extend(parsed.nodes.into_iter().map(|node| node.0));
        for message in parsed.messages {
            self.take_message(message, current, context);
        }
        self.comments
            .extend(parsed.comments.into_iter().map(comment));
        self.value_tables
            .extend(parsed.value_tables.into_iter().map(|table| ValueTable {
                target: ValueTarget::Table(table.name),
                values: values(table.descriptions),
            }));
        self.take_values(parsed.value_descriptions);
        self.take_ranges(parsed.extended_multiplex);
        for entry in parsed.signal_extended_value_type_list {
            let format = match entry.signal_extended_value_type {
                SignalExtendedValueType::SignedOrUnsignedInteger => SignalFormat::Integer,
                SignalExtendedValueType::IEEEfloat32Bit => SignalFormat::Float,
                SignalExtendedValueType::IEEEdouble64bit => SignalFormat::Double,
            };
            self.formats
                .insert((id(entry.message_id), entry.signal_name), format);
        }
        for attribute in parsed.attribute_values_message {
            if attribute.name == "GenMsgCycleTime" {
                self.take_cycle(Some(id(attribute.message_id)), attribute.value, line);
            }
        }
        for attribute in parsed.attribute_defaults {
            if attribute.name == "GenMsgCycleTime" {
                self.take_cycle(None, attribute.value, line);
            }
        }
    }

    fn take_values(&mut self, tables: Vec<ValueDescription>) {
        for table in tables {
            let (target, entries) = match table {
                ValueDescription::Signal {
                    message_id,
                    name,
                    value_descriptions,
                } => (
                    ValueTarget::Signal {
                        message: id(message_id),
                        signal: name,
                    },
                    value_descriptions,
                ),
                ValueDescription::EnvironmentVariable {
                    name,
                    value_descriptions,
                } => (ValueTarget::Environment(name), value_descriptions),
            };
            self.value_tables.push(ValueTable {
                target,
                values: values(entries),
            });
        }
    }

    fn take_ranges(&mut self, ranges: Vec<can_dbc::ExtendedMultiplex>) {
        self.multiplex_ranges
            .extend(ranges.into_iter().map(|value| {
                MultiplexRange {
                    message: id(value.message_id),
                    signal: value.signal_name,
                    switch: value.multiplexor_signal_name,
                    ranges: value
                        .mappings
                        .into_iter()
                        .map(|range| range.min_value..=range.max_value)
                        .collect(),
                }
            }));
    }

    fn take_cycle(&mut self, id: Option<CanId>, value: AttributeValue, line: u64) {
        let Some(value) = cycle(value) else {
            self.report.problem(line, "invalid GenMsgCycleTime");
            return;
        };
        let value = (value > 0.0).then_some(value);
        if let Some(id) = id {
            self.cycles.insert(id, value);
        } else {
            self.cycle_time_default_ms = value;
        }
    }

    fn take_message(
        &mut self,
        value: can_dbc::Message,
        current: &mut Option<usize>,
        context: (u64, bool),
    ) {
        let (line, is_signal) = context;
        if is_signal {
            if let Some(message) = current.and_then(|at| self.messages.get_mut(at)) {
                message
                    .signals
                    .extend(value.signals.into_iter().map(signal));
            } else {
                self.report.problem(line, "signal has no retained message");
            }
            return;
        }
        let id = id(value.id);
        if self.lookup.contains_key(&id) {
            self.report.problem(line, "duplicate message ID");
            *current = None;
            return;
        }
        *current = Some(self.messages.len());
        self.lookup.insert(id, self.messages.len());
        self.messages.push(Message {
            line,
            id,
            name: value.name,
            dlc: value.size,
            sender: value.transmitter,
            cycle_time_ms: None,
            signals: value.signals.into_iter().map(signal).collect(),
        });
    }
}

pub(super) fn rows(parsed: &can_dbc::Dbc) -> usize {
    let messages = parsed
        .messages
        .iter()
        .map(|message| {
            1 + message
                .signals
                .iter()
                .map(|signal| 1 + signal.receivers.len())
                .sum::<usize>()
        })
        .sum::<usize>();
    let values = parsed
        .value_tables
        .iter()
        .map(|table| 1 + table.descriptions.len())
        .sum::<usize>()
        + parsed
            .value_descriptions
            .iter()
            .map(|table| match table {
                ValueDescription::Signal {
                    value_descriptions, ..
                }
                | ValueDescription::EnvironmentVariable {
                    value_descriptions, ..
                } => 1 + value_descriptions.len(),
            })
            .sum::<usize>();
    let ranges = parsed
        .extended_multiplex
        .iter()
        .map(|range| 1 + range.mappings.len())
        .sum::<usize>();
    parsed.nodes.len()
        + messages
        + values
        + ranges
        + parsed.comments.len()
        + parsed.attribute_values_message.len()
        + parsed.attribute_defaults.len()
        + parsed.signal_extended_value_type_list.len()
}
