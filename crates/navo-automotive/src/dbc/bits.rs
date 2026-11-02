use super::{Message, Multiplexing, Signal, SignalFormat};
use crate::Endian;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitLayout {
    pub bytes: u8,
    pub cells: Vec<BitCell>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BitCell {
    pub signals: Vec<u16>,
}

impl BitLayout {
    #[must_use]
    pub fn cell(&self, byte: u8, bit: u8) -> &BitCell {
        &self.cells[usize::from(byte) * 8 + usize::from(bit)]
    }

    fn add_signal(&mut self, message: &Message, index: u16) {
        let signal = &message.signals[usize::from(index)];
        let mut bit = signal.start_bit;
        for _ in 0..signal.length {
            if bit >= u64::from(self.bytes) * 8 {
                self.problems
                    .push(format!("{} runs past byte {}", signal.name, bit / 8));
                break;
            }
            let cell = &mut self.cells[bit as usize];
            self.problems.extend(
                cell.signals
                    .iter()
                    .map(|&previous| &message.signals[usize::from(previous)])
                    .filter(|other| overlaps(signal, other))
                    .map(|other| {
                        format!("{} and {} overlap at bit {bit}", other.name, signal.name)
                    }),
            );
            cell.signals.push(index);
            bit = match signal.byte_order {
                Endian::Little => bit + 1,
                Endian::Big if bit.is_multiple_of(8) => bit + 15,
                Endian::Big => bit - 1,
            };
        }
    }
}

fn overlaps(signal: &Signal, other: &Signal) -> bool {
    !matches!(
        (signal.multiplexing, other.multiplexing),
        (Multiplexing::Value(a), Multiplexing::Value(b)) if a != b
    )
}

#[must_use]
pub fn bit_layout(message: &Message) -> BitLayout {
    let bytes = message.dlc.min(64) as u8;
    let mut layout = BitLayout {
        bytes,
        cells: vec![BitCell::default(); usize::from(bytes) * 8],
        problems: Vec::new(),
    };
    for (index, _) in (0..=u16::MAX).zip(&message.signals) {
        layout.add_signal(message, index);
    }
    layout
}

#[must_use]
pub fn formula(signal: &Signal) -> String {
    let kind = match signal.format {
        SignalFormat::Float | SignalFormat::Double => "float",
        SignalFormat::Integer if signal.signed => "signed",
        SignalFormat::Integer => "unsigned",
    };
    format!(
        "phys = raw × {} + {} ({kind} {} bit)",
        signal.factor, signal.offset, signal.length
    )
}
