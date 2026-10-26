use std::collections::HashMap;

use a2lfile::{A2lError, A2lObjectName};

use super::{A2lModel, AxisDescription, Characteristic, CompuMethod, Measurement};

pub(super) struct Block {
    pub kind: String,
    pub depth: usize,
    pub line: u64,
    pub text: String,
    pub overflow: bool,
    pub last_line: u64,
    pub lines: Vec<u64>,
}

impl Block {
    pub(super) fn push(&mut self, text: &str, line: u64) {
        let newlines = usize::from(line != self.last_line);
        let extra = newlines.saturating_add(text.len()).saturating_add(1);
        self.overflow |= self.text.len().saturating_add(extra) > crate::description::MAX_RECORD;
        if !self.overflow {
            if newlines > 0 {
                self.text.push('\n');
                self.lines.push(line);
            }
            self.text.push_str(text);
            self.text.push(' ');
            let embedded_lines = text.bytes().filter(|byte| *byte == b'\n').count() as u64;
            self.lines
                .extend((1..=embedded_lines).map(|offset| line + offset));
            self.last_line = line + embedded_lines;
        }
    }

    fn source_line(&self, relative: u64) -> u64 {
        self.lines
            .get(relative.saturating_sub(2) as usize)
            .copied()
            .unwrap_or(self.last_line)
    }
}

impl A2lModel {
    pub(super) fn parse(&mut self, block: Block, module: usize) {
        if block.overflow {
            self.report.problem(
                block.line,
                "A2L row block exceeds 256 KiB or contains invalid tokens",
            );
            return;
        }
        let (major, minor) = self.version.unwrap_or((1, 71));
        let text = format!(
            "ASAP2_VERSION {major} {minor} /begin PROJECT P \"\" /begin MODULE M \"\"\n{}\n/end MODULE /end PROJECT",
            block.text
        );
        match a2lfile::load_from_string(&text, None, false) {
            Ok((parsed, warnings)) => {
                for warning in warnings {
                    self.problem(&warning, &block);
                }
                for parsed_module in parsed.project.module {
                    self.take(parsed_module, module, block.line);
                }
            }
            Err(error) => self.problem(&error, &block),
        }
    }

    fn take(&mut self, parsed: a2lfile::Module, module: usize, line: u64) {
        for row in parsed.measurement {
            self.take_measurement(row, module, line);
        }
        for row in parsed.characteristic {
            self.take_characteristic(row, module, line);
        }
        for row in parsed.compu_method {
            self.take_compu_method(row, module, line);
        }
    }

    fn take_measurement(&mut self, row: a2lfile::Measurement, module: usize, line: u64) {
        if !self.report.retain(
            line,
            row.get_name().len()
                + row.conversion.len()
                + row.phys_unit.as_ref().map_or(0, |unit| unit.unit.len())
                + 16,
        ) {
            return;
        }
        self.measurements.push(Measurement {
            module,
            line,
            name: row.get_name().to_owned(),
            address: row.ecu_address.map(|address| address.address),
            data_type: row.datatype.to_string(),
            conversion: row.conversion,
            unit: row.phys_unit.map(|unit| unit.unit.into()),
            lower_limit: row.lower_limit,
            upper_limit: row.upper_limit,
        });
    }

    fn take_characteristic(&mut self, row: a2lfile::Characteristic, module: usize, line: u64) {
        if !self.report.retain(
            line,
            row.get_name().len()
                + row.conversion.len()
                + row.deposit.len()
                + row
                    .axis_descr
                    .iter()
                    .map(|axis| {
                        axis.input_quantity.len()
                            + axis.conversion.len()
                            + std::mem::size_of::<AxisDescription>()
                    })
                    .sum::<usize>()
                + row.phys_unit.as_ref().map_or(0, |unit| unit.unit.len()),
        ) {
            return;
        }
        self.characteristics.push(Characteristic {
            module,
            line,
            name: row.get_name().to_owned(),
            kind: row.characteristic_type.to_string(),
            axes: row
                .axis_descr
                .into_iter()
                .map(|axis| AxisDescription {
                    input_quantity: axis.input_quantity,
                    conversion: axis.conversion,
                })
                .collect(),
            address: row.address,
            record_layout: row.deposit,
            conversion: row.conversion,
            unit: row.phys_unit.map(|unit| unit.unit.into()),
            lower_limit: row.lower_limit,
            upper_limit: row.upper_limit,
        });
    }

    fn take_compu_method(&mut self, row: a2lfile::CompuMethod, module: usize, line: u64) {
        if self
            .report
            .retain(line, row.get_name().len() + row.unit.len())
        {
            self.compu_methods.push(CompuMethod {
                module,
                line,
                name: row.get_name().to_owned(),
                conversion_type: row.conversion_type.to_string(),
                coeffs: row
                    .coeffs
                    .map(|coeffs| [coeffs.a, coeffs.b, coeffs.c, coeffs.d, coeffs.e, coeffs.f]),
                coeffs_linear: row.coeffs_linear.map(|coeffs| [coeffs.a, coeffs.b]),
                unit: row.unit.into(),
            });
        }
    }

    fn problem(&mut self, error: &A2lError, block: &Block) {
        let reason = error.to_string();
        let relative = error_line(error);
        self.report.problem(
            block.source_line(relative),
            reason.chars().take(1024).collect::<String>(),
        );
    }

    pub(super) fn resolve_units(&mut self) {
        let units: HashMap<_, _> = self
            .compu_methods
            .iter()
            .map(|method| ((method.module, method.name.as_str()), &method.unit))
            .collect();
        for row in &mut self.measurements {
            if row.unit.is_none() {
                row.unit = units
                    .get(&(row.module, row.conversion.as_str()))
                    .map(|unit| (*unit).clone());
            }
        }
        for row in &mut self.characteristics {
            if row.unit.is_none() {
                row.unit = units
                    .get(&(row.module, row.conversion.as_str()))
                    .map(|unit| (*unit).clone());
            }
        }
    }
}

fn error_line(error: &A2lError) -> u64 {
    match error {
        A2lError::ParserError { parser_error } => parser_line(parser_error),
        A2lError::TokenizerError { tokenizer_error } => tokenizer_line(tokenizer_error),
        _ => 2,
    }
    .into()
}

fn parser_line(error: &a2lfile::ParserError) -> u32 {
    use a2lfile::ParserError::*;
    match error {
        UnexpectedTokenType { error_line, .. }
        | MalformedNumber { error_line, .. }
        | InvalidEnumValue { error_line, .. }
        | InvalidMultiplicityTooMany { error_line, .. }
        | InvalidMultiplicityNotPresent { error_line, .. }
        | IncorrectBlockError { error_line, .. }
        | IncorrectKeywordError { error_line, .. }
        | IncorrectEndTag { error_line, .. }
        | UnknownSubBlock { error_line, .. }
        | UnexpectedEOF { error_line, .. }
        | StringTooLong { error_line, .. }
        | BlockRefDeprecated { error_line, .. }
        | BlockRefTooNew { error_line, .. }
        | EnumRefDeprecated { error_line, .. }
        | EnumRefTooNew { error_line, .. }
        | InvalidBegin { error_line, .. }
        | InvalidIdentifier { error_line, .. }
        | A2mlError { error_line, .. }
        | AdditionalTokensError { error_line, .. } => *error_line,
        _ => 2,
    }
}

fn tokenizer_line(error: &a2lfile::TokenizerError) -> u32 {
    use a2lfile::TokenizerError::*;
    match error {
        IncludeFileError { line, .. }
        | IncompleteIncludeError { line, .. }
        | IncludeRecursionError { line, .. }
        | InvalidA2lToken { line, .. }
        | InvalidNumericalConstant { line, .. }
        | UnclosedComment { line, .. }
        | UnclosedString { line, .. }
        | MissingWhitespace { line, .. } => *line,
        _ => 2,
    }
}
