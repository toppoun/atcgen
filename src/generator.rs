use std::fmt::Write as _;

use crate::Error;
use crate::ast::{BinaryOp, ExprNode, Located, RangeEnd, UnaryOp};
use crate::error::Position;
use crate::validation::{
    BoundExpr, BoundRange, Reference, ValidatedFor, ValidatedInput, ValidatedRow,
    ValidatedStatement,
};

const MAX_INTEGERS: usize = 1_000_000;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_LOOP_ITERATIONS: usize = MAX_INTEGERS;

#[derive(Default)]
struct Values {
    current: Option<i64>,
    collection: Vec<i64>,
}

struct Generator {
    rng: SplitMix64,
    values: Vec<Values>,
    output: String,
    integers: usize,
}

pub(crate) fn generate(input: &ValidatedInput<'_>, seed: u64) -> Result<String, Error> {
    let mut generator = Generator {
        rng: SplitMix64 { state: seed },
        values: std::iter::repeat_with(Values::default)
            .take(input.slot_count)
            .collect(),
        output: String::new(),
        integers: 0,
    };
    for statement in &input.statements {
        match statement {
            ValidatedStatement::Row(row) => generator.row(row, false)?,
            ValidatedStatement::For(loop_) => generator.run_loop(loop_)?,
        }
    }
    Ok(generator.output)
}

impl Generator {
    fn run_loop(&mut self, loop_: &ValidatedFor<'_>) -> Result<(), Error> {
        let (start, _, width) = range_bounds(&loop_.range, &self.values)?;
        if width <= 0 {
            return Ok(());
        }
        let position = loop_.range.upper.position;
        let count = usize::try_from(width)
            .ok()
            .filter(|&count| count <= MAX_LOOP_ITERATIONS)
            .ok_or_else(|| {
                Error::at(position, format!("loop iteration limit exceeded ({MAX_LOOP_ITERATIONS}): range `{}` has {width} iterations", loop_.range.source))
            })?;
        if loop_.rows.is_empty() {
            return Ok(());
        }

        // Every row costs a newline, and every declaration produces at least one integer.
        if loop_.rows.len() > (MAX_OUTPUT_BYTES - self.output.len()) / count {
            return Err(output_limit(position));
        }
        let declarations = loop_.slots.len() - 1;
        if declarations > (MAX_INTEGERS - self.integers) / count {
            return Err(Error::at(
                position,
                format!(
                    "integer limit exceeded ({MAX_INTEGERS}) in loop range `{}`",
                    loop_.range.source
                ),
            ));
        }

        for offset in 0..count {
            let index = start
                .checked_add(offset as i64)
                .ok_or_else(|| Error::at(position, "loop index overflow"))?;
            self.values[loop_.slots.start].current = Some(index);
            for row in &loop_.rows {
                self.row(row, true)?;
            }
            for values in &mut self.values[loop_.slots.clone()] {
                values.current = None;
            }
        }
        Ok(())
    }

    fn row(&mut self, row: &ValidatedRow<'_>, repeated: bool) -> Result<(), Error> {
        let row_start = self.output.len();
        for variable in &row.variables {
            let name = variable.name;
            let length_expr = variable.length.as_ref();
            let length = if let Some(expr) = length_expr {
                let length = evaluate(expr, &self.values)?;
                if length <= 0 {
                    return Err(Error::at(
                        expr.position,
                        format!("invalid array length {length}; expected a positive integer"),
                    )
                    .with_expression(expr.source));
                }
                length
            } else {
                1
            };
            let count = usize::try_from(length)
                .ok()
                .filter(|&count| count <= MAX_INTEGERS - self.integers)
                .ok_or_else(|| {
                    let message = format!("integer limit exceeded ({MAX_INTEGERS})");
                    match length_expr {
                        Some(expr) => {
                            Error::at(expr.position, message).with_expression(expr.source)
                        }
                        None => Error::at(name.position, message),
                    }
                })?;
            self.integers += count;

            let (lower, width) = evaluate_range(&variable.range, &self.values)?;

            let values = &mut self.values[variable.slot];
            for _ in 0..count {
                let value = self.rng.sample(lower, width);
                if length_expr.is_none() {
                    values.current = Some(value);
                }
                if repeated || length_expr.is_some() {
                    values.collection.push(value);
                }
                if self.output.len() != row_start {
                    self.output.push(' ');
                }
                write!(self.output, "{value}").expect("writing to a String cannot fail");
                check_output_size(&self.output, name.position)?;
            }
        }
        self.output.push('\n');
        check_output_size(&self.output, row.position)
    }
}

fn range_bounds(range: &BoundRange<'_>, values: &[Values]) -> Result<(i64, i64, i128), Error> {
    let lower = evaluate(&range.lower, values)?;
    let upper = evaluate(&range.upper, values)?;
    // Widen before subtraction so even a range spanning all i64 values fits.
    let difference = i128::from(upper) - i128::from(lower);
    let width = match range.end {
        RangeEnd::Exclusive => difference,
        RangeEnd::Inclusive => difference + 1,
    };
    Ok((lower, upper, width))
}

fn evaluate_range(range: &BoundRange<'_>, values: &[Values]) -> Result<(i64, i128), Error> {
    let (lower, upper, width) = range_bounds(range, values)?;
    if width <= 0 {
        return Err(Error::at(
            range.upper.position,
            format!(
                "invalid range `{}`: empty with lower bound {lower} and upper bound {upper}",
                range.source,
            ),
        ));
    }
    Ok((lower, width))
}

fn evaluate(expr: &BoundExpr<'_>, values: &[Values]) -> Result<i64, Error> {
    evaluate_node(&expr.node, values).map_err(|error| error.with_expression(expr.source))
}

fn evaluate_node(node: &ExprNode<Reference<'_>>, values: &[Values]) -> Result<i64, Error> {
    match node {
        ExprNode::Literal(value) => Ok(*value),
        ExprNode::Variable(reference) => values[reference.slot].current.ok_or_else(|| {
            Error::at(
                reference.name.position,
                format!("scalar `{}` is not available", reference.name.value),
            )
        }),
        ExprNode::Unary { op, operand } => {
            let value = evaluate_node(operand, values)?;
            match op.value {
                UnaryOp::Plus => Ok(value),
                UnaryOp::Minus => value
                    .checked_neg()
                    .ok_or_else(|| Error::at(op.position, "integer overflow")),
            }
        }
        ExprNode::Binary { first, rest } => {
            let mut value = evaluate_node(first, values)?;
            for (op, operand) in rest {
                let rhs = evaluate_node(operand, values)?;
                value = evaluate_binary(*op, value, rhs)?;
            }
            Ok(value)
        }
    }
}

fn evaluate_binary(op: Located<BinaryOp>, lhs: i64, rhs: i64) -> Result<i64, Error> {
    let value = match op.value {
        BinaryOp::Add => lhs.checked_add(rhs),
        BinaryOp::Subtract => lhs.checked_sub(rhs),
        BinaryOp::Multiply => lhs.checked_mul(rhs),
        BinaryOp::Divide if rhs == 0 => return Err(Error::at(op.position, "division by zero")),
        BinaryOp::Remainder if rhs == 0 => return Err(Error::at(op.position, "remainder by zero")),
        BinaryOp::Divide => lhs.checked_div(rhs),
        BinaryOp::Remainder => lhs.checked_rem(rhs),
    };
    value.ok_or_else(|| Error::at(op.position, "integer overflow"))
}

fn check_output_size(output: &str, position: Position) -> Result<(), Error> {
    if output.len() > MAX_OUTPUT_BYTES {
        return Err(output_limit(position));
    }
    Ok(())
}

fn output_limit(position: Position) -> Error {
    Error::at(
        position,
        format!("output size limit exceeded ({MAX_OUTPUT_BYTES} bytes)"),
    )
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut word = self.state;
        word = (word ^ (word >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        word = (word ^ (word >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        word ^ (word >> 31)
    }

    fn sample(&mut self, lower: i64, width: i128) -> i64 {
        let offset = if let Ok(width) = u64::try_from(width) {
            // Discard 2^64 mod width values so all residues are equally likely.
            let threshold = width.wrapping_neg() % width;
            loop {
                let word = self.next();
                if word >= threshold {
                    break word % width;
                }
            }
        } else {
            // The full i64 range has exactly 2^64 possible values.
            self.next()
        };
        i64::try_from(i128::from(lower) + i128::from(offset)).expect("sample is in the i64 range")
    }
}
