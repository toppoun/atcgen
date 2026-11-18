use std::fmt::Write as _;

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

#[derive(Default)]
pub(crate) struct VerifiedOutput<'values> {
    tokens: Vec<Token<'values>>,
    // An end offset per line, including empty lines. The token limit fits in u32.
    row_ends: Vec<u32>,
    bytes: usize,
}

pub(crate) fn validate<'values>(
    statements: &[BoundInputStmt<'_>],
    values: &'values [Value],
) -> Result<VerifiedOutput<'values>, Error> {
    let limited = statements.iter().any(|statement| match statement {
        BoundInputStmt::Row(_) => false,
        BoundInputStmt::RowBlock { .. } | BoundInputStmt::If { .. } => true,
        BoundInputStmt::For { body, .. } => body
            .iter()
            .any(|item| !matches!(item, BoundInputStmt::Row(_))),
    });
    let mut output = VerifiedOutput::default();
    output.execute(
        statements,
        values,
        &mut Vec::new(),
        &mut WorkBudget::new(limited),
        None,
    )?;
    Ok(output)
}

impl<'values> VerifiedOutput<'values> {
    fn execute(
        &mut self,
        statements: &[BoundInputStmt<'_>],
        values: &'values [Value],
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
                    let start = row_start.unwrap_or(self.tokens.len());
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
                    let start = self.tokens.len();
                    self.execute(body, values, loop_values, work, Some(start))?;
                    self.end_row(*position)?;
                }
                BoundInputStmt::For { range, body, .. } => {
                    let iterations = evaluation.iterations(range, work)?;
                    if !body.is_empty() {
                        for index in iterations {
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
        evaluation: &Evaluation<'values, '_>,
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
        self.row_ends
            .push(u32::try_from(self.tokens.len()).expect("output token count is bounded"));
        Ok(())
    }

    fn check_token_limit(&self, count: usize, position: Position) -> Result<(), Error> {
        if count > MAX_OUTPUT_TOKENS - self.tokens.len() {
            return Err(Error::at(
                position,
                format!("output token limit exceeded ({MAX_OUTPUT_TOKENS})"),
            ));
        }
        Ok(())
    }

    fn add_value(
        &mut self,
        token: Token<'values>,
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
        let separator = usize::from(self.tokens.len() != row_start);
        self.add_bytes(length + separator, position)?;
        self.tokens.push(token);
        Ok(())
    }

    fn add_bytes(&mut self, count: usize, position: Position) -> Result<(), Error> {
        if count > MAX_OUTPUT_BYTES - self.bytes {
            return Err(Error::at(
                position,
                format!("output size limit exceeded ({MAX_OUTPUT_BYTES} bytes)"),
            ));
        }
        self.bytes += count;
        Ok(())
    }

    pub(crate) fn format(self) -> String {
        let mut output = String::with_capacity(self.bytes);
        let mut start = 0;
        for end in self.row_ends {
            let end = end as usize;
            for (offset, value) in self.tokens[start..end].iter().enumerate() {
                if offset != 0 {
                    output.push(' ');
                }
                match value {
                    Token::Int(value) => {
                        write!(output, "{value}").expect("writing to a String cannot fail")
                    }
                    Token::Char(byte) => output.push(char::from(*byte)),
                    Token::String(bytes) => {
                        output.push_str(std::str::from_utf8(bytes).expect("validated ASCII string"))
                    }
                }
            }
            output.push('\n');
            start = end;
        }
        debug_assert_eq!(output.len(), self.bytes);
        output
    }
}
