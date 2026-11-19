use crate::Error;
use crate::error::Position;
use crate::generator::{Evaluation, Value, WorkBudget, initialized_element};
use crate::validation::{BoundInputStmt, BoundOutputRef, BoundRow};

const MAX_OUTPUT_TOKENS: usize = 1_000_000;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

enum Token<'values> {
    Int(i64),
    Char(u8),
    String(&'values [u8]),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    tokens: usize,
    bytes: usize,
}

// Only the immutable program/values, scalar counts, and O(depth) loop storage
// survive the first traversal. No token or row transcript is retained.
pub(crate) struct VerifiedOutput<'program, 'source, 'values> {
    statements: &'program [BoundInputStmt<'source>],
    values: &'values [Value],
    counts: Counts,
    limited: bool,
    loop_values: Vec<i64>,
}

pub(crate) fn validate<'program, 'source, 'values>(
    statements: &'program [BoundInputStmt<'source>],
    values: &'values [Value],
) -> Result<VerifiedOutput<'program, 'source, 'values>, Error> {
    let limited = statements.iter().any(|statement| match statement {
        BoundInputStmt::Row(_) => false,
        BoundInputStmt::RowBlock { .. } | BoundInputStmt::If { .. } => true,
        BoundInputStmt::For { body, .. } => body
            .iter()
            .any(|item| !matches!(item, BoundInputStmt::Row(_))),
    });
    let mut walker = Walker {
        counts: Counts::default(),
        sink: Counter,
    };
    let mut loop_values = Vec::new();
    walker.execute(
        statements,
        values,
        &mut loop_values,
        &mut WorkBudget::new(limited),
        None,
    )?;
    Ok(VerifiedOutput {
        statements,
        values,
        counts: walker.counts,
        limited,
        loop_values,
    })
}

impl VerifiedOutput<'_, '_, '_> {
    pub(crate) fn format(mut self) -> Result<String, Error> {
        let position = self
            .statements
            .first()
            .map_or(Position { line: 1, column: 1 }, BoundInputStmt::position);
        #[cfg(test)]
        {
            if crate::faults::take(crate::faults::Fault::OutputBytes) {
                self.counts.bytes = self.counts.bytes.saturating_sub(1);
            }
            if crate::faults::take(crate::faults::Fault::OutputTokens) {
                self.counts.tokens = self.counts.tokens.saturating_sub(1);
            }
            if crate::faults::take(crate::faults::Fault::FinalCounts) {
                self.counts.tokens += 1;
            }
        }
        let output = crate::runtime::output_string(self.counts.bytes, position)?;
        let mut walker = Walker {
            counts: Counts::default(),
            sink: Formatter {
                output,
                expected_bytes: self.counts.bytes,
            },
        };
        // This is a replay of the same logical input execution, not a second
        // debit from the first execution's budget. Values and RNG never change.
        walker
            .execute(
                self.statements,
                self.values,
                &mut self.loop_values,
                &mut WorkBudget::new(self.limited),
                None,
            )
            .map_err(|error| {
                Error::at(position, format!("internal output inconsistency: {error}"))
            })?;
        if walker.counts != self.counts || walker.sink.output.len() != self.counts.bytes {
            return Err(inconsistent(position));
        }
        Ok(walker.sink.output)
    }
}

fn inconsistent(position: Position) -> Error {
    Error::at(
        position,
        "internal output inconsistency between counting and formatting",
    )
}

trait Sink {
    fn token(&mut self, token: Token<'_>, separator: bool, position: Position)
    -> Result<(), Error>;
    fn end_row(&mut self, position: Position) -> Result<(), Error>;
}

struct Counter;

impl Sink for Counter {
    fn token(
        &mut self,
        _token: Token<'_>,
        _separator: bool,
        _position: Position,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn end_row(&mut self, _position: Position) -> Result<(), Error> {
        Ok(())
    }
}

struct Formatter {
    output: String,
    expected_bytes: usize,
}

impl Formatter {
    fn check_additional(&self, count: usize, position: Position) -> Result<(), Error> {
        let length = self
            .output
            .len()
            .checked_add(count)
            .ok_or_else(|| inconsistent(position))?;
        // Reserve only once. Never use spare allocator capacity to hide a
        // counting mistake, or let push_str grow the output during replay.
        if length > self.expected_bytes || length > self.output.capacity() {
            return Err(inconsistent(position));
        }
        Ok(())
    }

    fn append(&mut self, bytes: &[u8], position: Position) -> Result<(), Error> {
        self.check_additional(bytes.len(), position)?;
        let text = std::str::from_utf8(bytes).map_err(|_| inconsistent(position))?;
        self.output.push_str(text);
        Ok(())
    }
}

impl Sink for Formatter {
    fn token(
        &mut self,
        token: Token<'_>,
        separator: bool,
        position: Position,
    ) -> Result<(), Error> {
        if separator {
            self.append(b" ", position)?;
        }
        match token {
            Token::Int(value) => {
                let mut buffer = [0_u8; 20];
                self.append(integer_bytes(value, &mut buffer), position)
            }
            Token::Char(byte) => {
                self.check_additional(1, position)?;
                if !byte.is_ascii() {
                    return Err(inconsistent(position));
                }
                self.output.push(char::from(byte));
                Ok(())
            }
            Token::String(bytes) => self.append(bytes, position),
        }
    }
    fn end_row(&mut self, position: Position) -> Result<(), Error> {
        self.check_additional(1, position)?;
        self.output.push('\n');
        Ok(())
    }
}

fn integer_bytes(value: i64, buffer: &mut [u8; 20]) -> &[u8] {
    let mut magnitude = value.unsigned_abs();
    let mut start = buffer.len();
    loop {
        start -= 1;
        buffer[start] = b'0' + (magnitude % 10) as u8;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    if value < 0 {
        start -= 1;
        buffer[start] = b'-';
    }
    &buffer[start..]
}

struct Walker<S> {
    counts: Counts,
    sink: S,
}

impl<S: Sink> Walker<S> {
    fn execute(
        &mut self,
        statements: &[BoundInputStmt<'_>],
        values: &[Value],
        loop_values: &mut Vec<i64>,
        work: &mut WorkBudget,
        row_start: Option<usize>,
    ) -> Result<(), Error> {
        for statement in statements {
            work.charge(1, statement.position())?;
            let evaluation = Evaluation {
                values,
                loop_values,
            };
            match statement {
                BoundInputStmt::Row(row) => {
                    let start = row_start.unwrap_or(self.counts.tokens);
                    self.append(row, &evaluation, start)?;
                    if row_start.is_none() {
                        self.end_row(row.position)?;
                    }
                }
                BoundInputStmt::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    let body = if evaluation.condition(condition)? {
                        then_body.as_slice()
                    } else {
                        else_body.as_deref().unwrap_or_default()
                    };
                    self.execute(body, values, loop_values, work, row_start)?;
                }
                BoundInputStmt::RowBlock { position, body } => {
                    let start = self.counts.tokens;
                    self.execute(body, values, loop_values, work, Some(start))?;
                    self.end_row(*position)?;
                }
                BoundInputStmt::For { range, body, .. } => {
                    let iterations = evaluation.iterations(range, work)?;
                    if !body.is_empty() {
                        for index in iterations {
                            if loop_values.len() == loop_values.capacity() {
                                crate::runtime::reserve_vec(
                                    loop_values,
                                    1,
                                    statement.position(),
                                    "output loop values",
                                )?;
                            }
                            loop_values.push(index);
                            self.execute(body, values, loop_values, work, row_start)?;
                            loop_values.pop();
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn append(
        &mut self,
        row: &BoundRow<'_>,
        evaluation: &Evaluation<'_, '_>,
        start: usize,
    ) -> Result<(), Error> {
        for item in &row.items {
            let position = item.position;
            let token = match &item.value {
                BoundOutputRef::Scalar(slot) => Token::Int(evaluation.scalar(*slot)),
                BoundOutputRef::Element(element) => Token::Int(evaluation.element(element)?),
                BoundOutputRef::Char(element) => Token::Char(evaluation.char_element(element)?),
                BoundOutputRef::WholeString(array) => {
                    Token::String(evaluation.initialized_string_row(*array, 0)?)
                }
                BoundOutputRef::StringRow(element) => {
                    let row = evaluation.string_row_index(element)?;
                    Token::String(evaluation.initialized_string_row(element.array, row)?)
                }
                BoundOutputRef::WholeStringArray(array) => {
                    let string = evaluation.string(*array);
                    let rows = string.cells.len() / string.width;
                    self.check_token_limit(rows, position)?;
                    for row in 0..rows {
                        let bytes = evaluation.initialized_string_row(*array, row)?;
                        self.add_value(Token::String(bytes), start, position)?;
                    }
                    continue;
                }
                BoundOutputRef::WholeArray(array) => {
                    let elements = evaluation.array(*array);
                    self.check_token_limit(elements.len(), position)?;
                    for (offset, &value) in elements.iter().enumerate() {
                        let value = initialized_element(*array, offset, value)?;
                        self.add_value(Token::Int(value), start, position)?;
                    }
                    continue;
                }
            };
            self.check_token_limit(1, position)?;
            self.add_value(token, start, position)?;
        }

        Ok(())
    }

    fn end_row(&mut self, position: Position) -> Result<(), Error> {
        self.add_bytes(1, position)?;
        self.sink.end_row(position)?;
        Ok(())
    }

    fn check_token_limit(&self, count: usize, position: Position) -> Result<(), Error> {
        if count > MAX_OUTPUT_TOKENS - self.counts.tokens {
            return Err(Error::at(
                position,
                format!("output token limit exceeded ({MAX_OUTPUT_TOKENS})"),
            ));
        }
        Ok(())
    }

    fn add_value(
        &mut self,
        token: Token<'_>,
        row_start: usize,
        position: Position,
    ) -> Result<(), Error> {
        let length = match &token {
            Token::Int(value) => {
                value.unsigned_abs().max(1).ilog10() as usize + 1 + usize::from(*value < 0)
            }
            Token::Char(_) => 1,
            Token::String(bytes) => bytes.len(),
        };
        let separator = usize::from(self.counts.tokens != row_start);
        self.add_bytes(length + separator, position)?;
        self.sink.token(token, separator != 0, position)?;
        self.counts.tokens += 1;
        Ok(())
    }

    fn add_bytes(&mut self, count: usize, position: Position) -> Result<(), Error> {
        if count > MAX_OUTPUT_BYTES - self.counts.bytes {
            return Err(Error::at(
                position,
                format!("output size limit exceeded ({MAX_OUTPUT_BYTES} bytes)"),
            ));
        }
        self.counts.bytes += count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faults::{self, Fault};
    use crate::{generator, parser, validation};

    fn check(source: &str, expected: &str) {
        let program = validation::validate(parser::parse(source).unwrap()).unwrap();
        let values = generator::generate(&program.generate, program.slot_count, 42).unwrap();
        let plan = validate(&program.input, &values).unwrap();
        assert_eq!(plan.counts.bytes, expected.len());
        assert_eq!(
            plan.counts.tokens,
            expected.split_ascii_whitespace().count()
        );
        let output = plan.format().unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn counts_and_formats_mixed_arrays_grid_nested_branches_rows_and_i64_extremes() {
        let source = "generate {\nA[2]\nA[1] in -9223372036854775808..=-9223372036854775808\n\
            A[2] in 9223372036854775807..=9223372036854775807\nS : string[2]\n\
            S[1] = 'a'\nS[2] = 'b'\nG[2] : string[1]\nG[1][1] = 'x'\nG[2][1] = 'y'\n}\n\
            input {\n\nrow {\nA S G\nif A[1] < 0 {\nfor i in 1..=2 {\n\
            if i == 1 {\nA[i] G[i]\n} else {\nS[2] A[i]\n}\n}\n} else {\nA[99]\n}\n}\n\
            for j in 1..1 {\nA[99]\n}\nrow {\n}\n}\n";
        check(
            source,
            "\n-9223372036854775808 9223372036854775807 ab x y -9223372036854775808 x b 9223372036854775807\n\n",
        );
    }

    #[test]
    fn zero_tokens_can_have_many_rows_and_no_output_is_also_valid() {
        check(
            "generate {\n}\ninput {\nfor i in 1..=1024 {\n\n\n}\n}\n",
            &"\n".repeat(2048),
        );
        check("generate {\n}\ninput {\n}\n", "");
    }

    #[test]
    fn first_traversal_never_allocates_the_output_string() {
        let program = validation::validate(
            parser::parse("generate {\nN in 7..=7\n}\ninput {\nN N\n}\n").unwrap(),
        )
        .unwrap();
        let values = generator::generate(&program.generate, program.slot_count, 42).unwrap();
        faults::with(Fault::Allocation("output buffer"), || {
            let plan = validate(&program.input, &values).unwrap();
            assert_eq!(
                plan.counts,
                Counts {
                    tokens: 2,
                    bytes: 4
                }
            );
            assert!(
                plan.format()
                    .unwrap_err()
                    .to_string()
                    .contains("unable to allocate output buffer")
            );
        });
    }

    #[test]
    fn second_traversal_rejects_byte_token_and_final_count_mismatches() {
        for fault in [Fault::OutputBytes, Fault::OutputTokens, Fault::FinalCounts] {
            faults::with(fault, || {
                let error = crate::generate("generate {\nN in 7..=7\n}\ninput {\nN N\n}\n", 42)
                    .unwrap_err();
                assert!(error.to_string().contains("internal output inconsistency"));
            });
        }
    }

    #[test]
    fn large_runtime_reservations_report_injected_failures() {
        for (source, area) in [
            ("generate {\nA[1000000]\n}\ninput {\n}\n", "integer array"),
            (
                "generate {\nS : string[1000000]\n}\ninput {\n}\n",
                "character cells",
            ),
            (
                "generate {\nN in 7..=7\n}\ninput {\nN\n}\n",
                "declaration values",
            ),
        ] {
            faults::with(Fault::Allocation(area), || {
                let error = crate::generate(source, 42).unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains(&format!("unable to allocate {area}"))
                );
            });
        }
    }
}
