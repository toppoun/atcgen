use std::collections::HashMap;
use std::fmt::Write as _;

use crate::Error;
use crate::ast::{BinaryOp, Expr, ExprNode, Located, RangeEnd, RangeExpr, UnaryOp};
use crate::error::Position;
use crate::validation::ValidatedInput;

const MAX_INTEGERS: usize = 1_000_000;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn generate(input: &ValidatedInput<'_>, seed: u64) -> Result<String, Error> {
    let mut rng = SplitMix64 { state: seed };
    let mut scalars = HashMap::new();
    let mut output = String::new();
    let mut integers = 0;

    for row in &input.rows {
        let row_start = output.len();
        for variable in row {
            let declaration = &variable.declaration;
            let constraint = &variable.constraint;
            let name = declaration.name;
            let length_expr = declaration.length.as_ref();
            let length = if let Some(expr) = length_expr {
                let length = evaluate(expr, &scalars)?;
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
                .filter(|&count| count <= MAX_INTEGERS - integers)
                .ok_or_else(|| {
                    let message = format!("integer limit exceeded ({MAX_INTEGERS})");
                    match length_expr {
                        Some(expr) => {
                            Error::at(expr.position, message).with_expression(expr.source)
                        }
                        None => Error::at(name.position, message),
                    }
                })?;
            integers += count;

            let (lower, width) = evaluate_range(&constraint.range, &scalars)?;

            for _ in 0..count {
                let value = rng.sample(lower, width);
                if declaration.length.is_none() {
                    scalars.insert(name.value, value);
                }
                if output.len() != row_start {
                    output.push(' ');
                }
                write!(output, "{value}").expect("writing to a String cannot fail");
                check_output_size(&output, name.position)?;
            }
        }
        output.push('\n');
        check_output_size(&output, row[0].declaration.name.position)?;
    }
    Ok(output)
}

fn evaluate_range(
    range: &RangeExpr<'_>,
    scalars: &HashMap<&str, i64>,
) -> Result<(i64, i128), Error> {
    let lower = evaluate(&range.lower, scalars)?;
    let upper = evaluate(&range.upper, scalars)?;
    // Widen before subtraction so even a range spanning all i64 values fits.
    let difference = i128::from(upper) - i128::from(lower);
    let width = match range.end {
        RangeEnd::Exclusive => difference,
        RangeEnd::Inclusive => difference + 1,
    };
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

fn evaluate(expr: &Expr<'_>, scalars: &HashMap<&str, i64>) -> Result<i64, Error> {
    evaluate_node(&expr.node, scalars).map_err(|error| error.with_expression(expr.source))
}

fn evaluate_node(node: &ExprNode<'_>, scalars: &HashMap<&str, i64>) -> Result<i64, Error> {
    match node {
        ExprNode::Literal(value) => Ok(*value),
        ExprNode::Variable(name) => scalars.get(name.value).copied().ok_or_else(|| {
            Error::at(
                name.position,
                format!("scalar `{}` is not available", name.value),
            )
        }),
        ExprNode::Unary { op, operand } => {
            let value = evaluate_node(operand, scalars)?;
            match op.value {
                UnaryOp::Plus => Ok(value),
                UnaryOp::Minus => value
                    .checked_neg()
                    .ok_or_else(|| Error::at(op.position, "integer overflow")),
            }
        }
        ExprNode::Binary { first, rest } => {
            let mut value = evaluate_node(first, scalars)?;
            for (op, operand) in rest {
                let rhs = evaluate_node(operand, scalars)?;
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
        return Err(Error::at(
            position,
            format!("output size limit exceeded ({MAX_OUTPUT_BYTES} bytes)"),
        ));
    }
    Ok(())
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
