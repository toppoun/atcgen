use std::collections::HashMap;
use std::fmt::Write as _;

use crate::Error;
use crate::ast::{Endpoint, Value};
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
            let length = declaration
                .length
                .map_or(1, |length| evaluate(length, &scalars));
            let length_position = declaration
                .length
                .map_or(name.position, |length| length.position);
            if length <= 0 {
                return Err(Error::at(
                    length_position,
                    format!("invalid array length {length}; expected a positive integer"),
                ));
            }
            let count = usize::try_from(length)
                .ok()
                .filter(|&count| count <= MAX_INTEGERS - integers)
                .ok_or_else(|| {
                    Error::at(
                        length_position,
                        format!("integer limit exceeded ({MAX_INTEGERS})"),
                    )
                })?;
            integers += count;

            let lower = evaluate(constraint.lower, &scalars);
            let upper = evaluate(constraint.upper, &scalars);
            if lower > upper {
                return Err(Error::at(
                    constraint.upper.position,
                    format!("invalid range: lower bound {lower} exceeds upper bound {upper}"),
                ));
            }

            for _ in 0..count {
                let value = rng.inclusive(lower, upper);
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

fn evaluate(endpoint: Endpoint<'_>, scalars: &HashMap<&str, i64>) -> i64 {
    match endpoint.value {
        Value::Integer(value) => value,
        Value::Scalar(name) => scalars[name],
    }
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

    fn inclusive(&mut self, lower: i64, upper: i64) -> i64 {
        let width = i128::from(upper) - i128::from(lower) + 1;
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
