use crate::Error;
use crate::ast::{BinaryOp, ExprNode, Located, Name, RangeEnd, UnaryOp};
use crate::validation::{
    ArrayRef, BoundElement, BoundElementInit, BoundExpr, BoundGenerateStmt, BoundNode, BoundRange,
    ScalarRef,
};

const MAX_ARRAY_ELEMENTS: usize = 1_000_000;
const MAX_GENERATED_INTEGERS: usize = 1_000_000;
const MAX_LOOP_ITERATIONS: i128 = 1_000_000;

pub(crate) enum Value {
    Scalar(i64),
    Array(Vec<Option<i64>>),
}

struct Generator {
    rng: SplitMix64,
    values: Vec<Value>,
    array_elements: usize,
    generated_integers: usize,
}

pub(crate) fn generate(
    statements: &[BoundGenerateStmt<'_>],
    slot_count: usize,
    seed: u64,
) -> Result<Vec<Value>, Error> {
    let mut generator = Generator {
        rng: SplitMix64 { state: seed },
        values: Vec::with_capacity(slot_count),
        array_elements: 0,
        generated_integers: 0,
    };
    for statement in statements {
        match statement {
            BoundGenerateStmt::Scalar { name, range } => {
                let (lower, width) = generator.evaluation(None).nonempty_range(range)?;
                let value = generator.sample(*name, lower, width)?;
                generator.values.push(Value::Scalar(value));
            }
            BoundGenerateStmt::Array(length) => generator.declare_array(length)?,
            BoundGenerateStmt::InitElement(init) => generator.init_element(init, None)?,
            BoundGenerateStmt::For { range, body } => {
                let iterations = generator.evaluation(None).iterations(range)?;
                if !body.is_empty() {
                    for index in iterations {
                        for init in body {
                            generator.init_element(init, Some(index))?;
                        }
                    }
                }
            }
        }
    }
    Ok(generator.values)
}

impl Generator {
    fn evaluation(&self, loop_value: Option<i64>) -> Evaluation<'_> {
        Evaluation {
            values: &self.values,
            loop_value,
        }
    }

    fn declare_array(&mut self, expr: &BoundExpr<'_>) -> Result<(), Error> {
        let length = self.evaluation(None).evaluate(expr)?;
        if length <= 0 {
            return Err(Error::at(
                expr.position,
                format!("invalid array length {length}; expected a positive integer"),
            )
            .with_expression(expr.source));
        }
        let length = usize::try_from(length)
            .ok()
            .filter(|&length| length <= MAX_ARRAY_ELEMENTS)
            .ok_or_else(|| {
                Error::at(
                    expr.position,
                    format!("array length limit exceeded ({MAX_ARRAY_ELEMENTS})"),
                )
                .with_expression(expr.source)
            })?;
        if length > MAX_ARRAY_ELEMENTS - self.array_elements {
            return Err(Error::at(
                expr.position,
                format!("total array capacity limit exceeded ({MAX_ARRAY_ELEMENTS})"),
            )
            .with_expression(expr.source));
        }
        self.array_elements += length;
        self.values.push(Value::Array(vec![None; length]));
        Ok(())
    }

    fn init_element(
        &mut self,
        init: &BoundElementInit<'_>,
        loop_value: Option<i64>,
    ) -> Result<(), Error> {
        let evaluation = self.evaluation(loop_value);
        let target = &init.target;
        let array = target.array;
        let offset = evaluation.offset(target)?;
        if evaluation.array(array)[offset].is_some() {
            return Err(Error::at(
                array.name.position,
                format!(
                    "array element `{}[{}]` is already initialized",
                    array.name.value,
                    offset + 1
                ),
            ));
        }
        let (lower, width) = evaluation.nonempty_range(&init.range)?;
        let value = self.sample(array.name, lower, width)?;
        let Value::Array(elements) = &mut self.values[array.slot] else {
            unreachable!("initialization target is bound to an array");
        };
        elements[offset] = Some(value);
        Ok(())
    }

    fn sample(&mut self, name: Name<'_>, lower: i64, width: i128) -> Result<i64, Error> {
        if self.generated_integers == MAX_GENERATED_INTEGERS {
            return Err(Error::at(
                name.position,
                format!("generated integer limit exceeded ({MAX_GENERATED_INTEGERS})"),
            ));
        }
        self.generated_integers += 1;
        Ok(self.rng.sample(lower, width))
    }
}

pub(crate) struct Evaluation<'values> {
    pub values: &'values [Value],
    pub loop_value: Option<i64>,
}

impl Evaluation<'_> {
    fn range_bounds(&self, range: &BoundRange<'_>) -> Result<(i64, i64, i128), Error> {
        let lower = self.evaluate(&range.lower)?;
        let upper = self.evaluate(&range.upper)?;
        // Widen before subtraction so even a range spanning all i64 values fits.
        let difference = i128::from(upper) - i128::from(lower);
        let width = match range.end {
            RangeEnd::Exclusive => difference,
            RangeEnd::Inclusive => difference + 1,
        };
        Ok((lower, upper, width))
    }

    fn nonempty_range(&self, range: &BoundRange<'_>) -> Result<(i64, i128), Error> {
        let (lower, upper, width) = self.range_bounds(range)?;
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

    pub(crate) fn iterations(
        &self,
        range: &BoundRange<'_>,
    ) -> Result<impl Iterator<Item = i64> + use<>, Error> {
        let (lower, _, width) = self.range_bounds(range)?;
        if width > MAX_LOOP_ITERATIONS {
            return Err(Error::at(
                range.upper.position,
                format!(
                    "loop iteration limit exceeded ({MAX_LOOP_ITERATIONS}): range `{}` has {width} iterations",
                    range.source,
                ),
            ));
        }
        let count = i64::try_from(width.max(0)).expect("loop width is bounded");
        // The last offset stays within the evaluated upper bound, including i64::MAX.
        Ok((0..count).map(move |offset| lower + offset))
    }

    pub(crate) fn scalar(&self, slot: usize) -> i64 {
        let Value::Scalar(value) = self.values[slot] else {
            unreachable!("scalar reference is bound to a scalar");
        };
        value
    }

    pub(crate) fn array(&self, array: ArrayRef<'_>) -> &[Option<i64>] {
        let Value::Array(elements) = &self.values[array.slot] else {
            unreachable!("array reference is bound to an array");
        };
        elements
    }

    fn offset(&self, element: &BoundElement<'_>) -> Result<usize, Error> {
        let index = self.evaluate(&element.index)?;
        let length = self.array(element.array).len();
        usize::try_from(index)
            .ok()
            .filter(|&index| (1..=length).contains(&index))
            .map(|index| index - 1)
            .ok_or_else(|| {
                Error::at(
                    element.index.position,
                    format!(
                        "index {index} is out of bounds for array `{}` of length {length}",
                        element.array.name.value,
                    ),
                )
            })
    }

    pub(crate) fn element(&self, element: &BoundElement<'_>) -> Result<i64, Error> {
        let offset = self.offset(element)?;
        initialized_element(element.array, offset, self.array(element.array)[offset])
    }

    fn evaluate(&self, expr: &BoundExpr<'_>) -> Result<i64, Error> {
        self.evaluate_node(&expr.node, expr.source)
    }

    fn evaluate_node(&self, node: &BoundNode<'_>, source: &str) -> Result<i64, Error> {
        match node {
            ExprNode::Literal(value) => Ok(*value),
            ExprNode::Variable(reference) => Ok(match reference {
                ScalarRef::Global(slot) => self.scalar(*slot),
                ScalarRef::LoopIndex => self.loop_value.expect("loop variable is in scope"),
            }),
            ExprNode::Element(element) => self.element(element),
            ExprNode::Unary { op, operand } => {
                let value = self.evaluate_node(operand, source)?;
                match op.value {
                    UnaryOp::Plus => Ok(value),
                    UnaryOp::Minus => value.checked_neg().ok_or_else(|| {
                        Error::at(op.position, "integer overflow").with_expression(source)
                    }),
                }
            }
            ExprNode::Binary { first, rest } => {
                let mut value = self.evaluate_node(first, source)?;
                for (op, operand) in rest {
                    let rhs = self.evaluate_node(operand, source)?;
                    value = evaluate_binary(*op, value, rhs)
                        .map_err(|error| error.with_expression(source))?;
                }
                Ok(value)
            }
        }
    }
}

pub(crate) fn initialized_element(
    array: ArrayRef<'_>,
    offset: usize,
    value: Option<i64>,
) -> Result<i64, Error> {
    value.ok_or_else(|| {
        Error::at(
            array.name.position,
            format!(
                "array element `{}[{}]` is uninitialized",
                array.name.value,
                offset + 1
            ),
        )
    })
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
