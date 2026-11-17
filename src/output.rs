use std::fmt::Write as _;

use crate::Error;
use crate::error::Position;
use crate::generator::{Evaluation, Value, WorkBudget, initialized_element};
use crate::validation::{BoundInputStmt, BoundOutputRef, BoundRow};

const MAX_OUTPUT_INTEGERS: usize = 1_000_000;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct VerifiedOutput {
    integers: Vec<i64>,
    // An end offset per line, including empty lines. The integer limit fits in u32.
    row_ends: Vec<u32>,
    bytes: usize,
}

pub(crate) fn validate(
    statements: &[BoundInputStmt<'_>],
    values: &[Value],
) -> Result<VerifiedOutput, Error> {
    let limited = statements.iter().any(|statement| match statement {
        BoundInputStmt::Row(_) => false,
        BoundInputStmt::RowBlock { .. } => true,
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

impl VerifiedOutput {
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
                    let start = row_start.unwrap_or(self.integers.len());
                    self.append(row, &evaluation, start)?;
                    if row_start.is_none() {
                        self.end_row(row.position)?;
                    }
                }
                BoundInputStmt::RowBlock { position, body } => {
                    let start = self.integers.len();
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
        evaluation: &Evaluation<'_>,
        start: usize,
    ) -> Result<(), Error> {
        for item in &row.items {
            let position = item.position;
            let value = match &item.value {
                BoundOutputRef::Scalar(slot) => evaluation.scalar(*slot),
                BoundOutputRef::Element(element) => evaluation.element(element)?,
                BoundOutputRef::WholeArray(array) => {
                    let elements = evaluation.array(*array);
                    self.check_integer_limit(elements.len(), position)?;
                    for (offset, &value) in elements.iter().enumerate() {
                        let value = initialized_element(*array, offset, value)?;
                        self.add_value(value, start, position)?;
                    }
                    continue;
                }
            };
            self.check_integer_limit(1, position)?;
            self.add_value(value, start, position)?;
        }

        Ok(())
    }

    fn end_row(&mut self, position: Position) -> Result<(), Error> {
        self.add_bytes(1, position)?;
        self.row_ends
            .push(u32::try_from(self.integers.len()).expect("output integer count is bounded"));
        Ok(())
    }

    fn check_integer_limit(&self, count: usize, position: Position) -> Result<(), Error> {
        if count > MAX_OUTPUT_INTEGERS - self.integers.len() {
            return Err(Error::at(
                position,
                format!("output integer limit exceeded ({MAX_OUTPUT_INTEGERS})"),
            ));
        }
        Ok(())
    }

    fn add_value(&mut self, value: i64, row_start: usize, position: Position) -> Result<(), Error> {
        let digits = value.unsigned_abs().max(1).ilog10() as usize + 1;
        let sign = usize::from(value < 0);
        let separator = usize::from(self.integers.len() != row_start);
        self.add_bytes(digits + sign + separator, position)?;
        self.integers.push(value);
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
            for (offset, value) in self.integers[start..end].iter().enumerate() {
                if offset != 0 {
                    output.push(' ');
                }
                write!(output, "{value}").expect("writing to a String cannot fail");
            }
            output.push('\n');
            start = end;
        }
        debug_assert_eq!(output.len(), self.bytes);
        output
    }
}
