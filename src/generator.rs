use crate::Error;
use crate::ast::{BinaryOp, ComparisonOp, ExprNode, Located, Name, RangeEnd, UnaryOp};
use crate::error::Position;
use crate::validation::{
    ArrayRef, BoundCondition, BoundElement, BoundElementInit, BoundExclusion, BoundExpr,
    BoundGenerateStmt, BoundInitValue, BoundNode, BoundRange, BoundUniqueInit, ScalarRef,
};

const MAX_ARRAY_ELEMENTS: usize = 1_000_000;
const MAX_CHAR_CELLS: usize = 1_000_000;
const MAX_GENERATED_INTEGERS: usize = 1_000_000;
const MAX_LOOP_ITERATIONS: i128 = 1_000_000;
const MAX_EXECUTION_WORK: usize = 10_000_000;
const MAX_UNIQUE_WIDTH: usize = 1_000_000;
const MAX_UNIQUE_EXCLUSIONS: usize = 1_000_000;
const MAX_UNIQUE_PAYLOAD_BYTES: usize = 16_000_000;

pub(crate) struct WorkBudget(Option<usize>);

impl WorkBudget {
    pub(crate) fn new(limited: bool) -> Self {
        Self(limited.then_some(MAX_EXECUTION_WORK))
    }

    pub(crate) fn charge(&mut self, units: usize, position: Position) -> Result<(), Error> {
        if let Some(remaining) = &mut self.0 {
            *remaining = remaining.checked_sub(units).ok_or_else(|| {
                Error::at(
                    position,
                    format!("execution work limit exceeded ({MAX_EXECUTION_WORK})"),
                )
            })?;
        }
        Ok(())
    }
}

pub(crate) enum Value {
    Scalar(i64),
    Array(Vec<Option<i64>>),
    String(StringValue),
    StringArray(StringValue),
}

pub(crate) struct StringValue {
    pub width: usize,
    // Zero cannot be a valid character, so it marks an uninitialized cell.
    pub cells: Vec<u8>,
}

struct Generator {
    rng: SplitMix64,
    values: Vec<Value>,
    array_elements: usize,
    generated_integers: usize,
    char_cells: usize,
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
        char_cells: 0,
    };
    let limited = statements.iter().any(|statement| match statement {
        BoundGenerateStmt::If { .. } | BoundGenerateStmt::InitUnique(_) => true,
        BoundGenerateStmt::For { body, .. } => body.iter().any(|item| {
            matches!(
                item,
                BoundGenerateStmt::For { .. } | BoundGenerateStmt::If { .. }
            )
        }),
        _ => false,
    });
    generator.execute(statements, &mut Vec::new(), &mut WorkBudget::new(limited))?;
    Ok(generator.values)
}

impl Generator {
    fn execute(
        &mut self,
        statements: &[BoundGenerateStmt<'_>],
        loop_values: &mut Vec<i64>,
        work: &mut WorkBudget,
    ) -> Result<(), Error> {
        for statement in statements {
            // Unique initialization charges its header together with internal
            // work, after checking the target, range, exclusions and candidates.
            if !matches!(statement, BoundGenerateStmt::InitUnique(_)) {
                work.charge(1, statement.position())?;
            }
            match statement {
                BoundGenerateStmt::Scalar { name, range } => {
                    let (lower, width) = self.evaluation(loop_values).nonempty_range(range)?;
                    let value = self.sample(*name, lower, width)?;
                    self.values.push(Value::Scalar(value));
                }
                BoundGenerateStmt::Array { length, .. } => self.declare_array(length)?,
                BoundGenerateStmt::String { count, length, .. } => {
                    self.declare_string(count.as_ref(), length)?
                }
                BoundGenerateStmt::InitElement(init) => self.init_element(init, loop_values)?,
                BoundGenerateStmt::InitUnique(init) => self.init_unique(init, work)?,
                BoundGenerateStmt::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    let body = if self.evaluation(loop_values).condition(condition)? {
                        then_body.as_slice()
                    } else {
                        else_body.as_deref().unwrap_or_default()
                    };
                    self.execute(body, loop_values, work)?;
                }
                BoundGenerateStmt::For { range, body, .. } => {
                    let iterations = self.evaluation(loop_values).iterations(range, work)?;
                    if !body.is_empty() {
                        for index in iterations {
                            loop_values.push(index);
                            self.execute(body, loop_values, work)?;
                            loop_values.pop();
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn evaluation<'values>(
        &'values self,
        loop_values: &'values [i64],
    ) -> Evaluation<'values, 'values> {
        Evaluation {
            values: &self.values,
            loop_values,
        }
    }

    fn declare_array(&mut self, expr: &BoundExpr<'_>) -> Result<(), Error> {
        let length = self.evaluation(&[]).evaluate(expr)?;
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
        loop_values: &[i64],
    ) -> Result<(), Error> {
        if !matches!(init.value, BoundInitValue::Range(_)) {
            return self.init_char(init, loop_values);
        }
        let evaluation = self.evaluation(loop_values);
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
        let BoundInitValue::Range(range) = &init.value else {
            unreachable!()
        };
        let (lower, width) = evaluation.nonempty_range(range)?;
        let value = self.sample(array.name, lower, width)?;
        let Value::Array(elements) = &mut self.values[array.slot] else {
            unreachable!("initialization target is bound to an array");
        };
        elements[offset] = Some(value);
        Ok(())
    }

    fn init_unique(
        &mut self,
        init: &BoundUniqueInit<'_>,
        work: &mut WorkBudget,
    ) -> Result<(), Error> {
        let evaluation = self.evaluation(&[]);
        let target = evaluation.array(init.target);
        if let Some(offset) = target.iter().position(Option::is_some) {
            return Err(Error::at(
                init.target.name.position,
                format!(
                    "array element `{}[{}]` is already initialized",
                    init.target.name.value,
                    offset + 1
                ),
            ));
        }
        let count = target.len();
        let (lower, width) = evaluation.nonempty_range(&init.range)?;
        let width = usize::try_from(width)
            .ok()
            .filter(|&width| width <= MAX_UNIQUE_WIDTH)
            .ok_or_else(|| {
                Error::at(
                    init.range.lower.position,
                    format!("unique candidate width limit exceeded ({MAX_UNIQUE_WIDTH})"),
                )
            })?;

        // Lengths alone allow a single fallible allocation. Do not evaluate any
        // exclusion values, or report a later read-limit failure, out of order.
        let exclusion_capacity = init.exclusions.iter().fold(0usize, |capacity, exclusion| {
            let length = match exclusion {
                BoundExclusion::Integer(_) => 1,
                BoundExclusion::Array(array) => evaluation.array(*array).len(),
            };
            capacity.saturating_add(length).min(MAX_UNIQUE_EXCLUSIONS)
        });
        let mut excluded = unique_buffer(exclusion_capacity, init.position)?;
        for exclusion in &init.exclusions {
            let position = exclusion.position();
            match exclusion {
                BoundExclusion::Integer(expr) => {
                    check_unique_exclusion_count(excluded.len(), position)?;
                    excluded.push(evaluation.evaluate(expr)?);
                }
                BoundExclusion::Array(array) => {
                    for (offset, &value) in evaluation.array(*array).iter().enumerate() {
                        check_unique_exclusion_count(excluded.len(), position)?;
                        excluded.push(initialized_element(*array, offset, value)?);
                    }
                }
            }
        }
        let exclusion_count = excluded.len();
        excluded.sort_unstable();
        excluded.dedup();
        let end = i128::from(lower) + width as i128;
        excluded.retain(|&value| value >= lower && i128::from(value) < end);
        let available = width - excluded.len();
        if count > available {
            return Err(Error::at(
                init.position,
                format!(
                    "insufficient unique candidates: need {count}, range width {width}, \
                     distinct in-range exclusions {}, available {available}",
                    excluded.len()
                ),
            ));
        }
        check_unique_payload(width, exclusion_count, init.position)?;
        if count > MAX_GENERATED_INTEGERS - self.generated_integers {
            return Err(Error::at(
                init.target.name.position,
                format!("generated integer limit exceeded ({MAX_GENERATED_INTEGERS})"),
            ));
        }
        work.charge(
            unique_work(count, width, exclusion_count, init.position)?,
            init.target.name.position,
        )?;
        let mut candidates = unique_buffer(available, init.position)?;
        let mut exclusions = excluded.iter().peekable();
        for offset in 0..width {
            // Widen before adding, including when the range ends at i64::MAX.
            let value = i64::try_from(i128::from(lower) + offset as i128)
                .expect("candidate is inside the validated i64 range");
            if exclusions.peek().is_some_and(|&&next| next == value) {
                exclusions.next();
            } else {
                candidates.push(value);
            }
        }

        // All fallible checks and allocations are complete. The ascending
        // candidate list and these forward swaps define the seed contract.
        for index in 0..count {
            let selected = self
                .rng
                .sample((index + 1) as i64, (available - index) as i128)
                as usize
                - 1;
            candidates.swap(index, selected);
        }
        let Value::Array(target) = &mut self.values[init.target.slot] else {
            unreachable!("unique initialization is bound to an integer array");
        };
        for (cell, value) in target.iter_mut().zip(candidates) {
            *cell = Some(value);
        }
        self.generated_integers += count;
        Ok(())
    }

    fn declare_string(
        &mut self,
        count: Option<&BoundExpr<'_>>,
        length: &BoundExpr<'_>,
    ) -> Result<(), Error> {
        // Evaluate dimensions once, in source order, before allocating any cells.
        let rows = count
            .map(|expr| self.evaluation(&[]).dimension(expr))
            .transpose()?
            .unwrap_or(1);
        let width = self.evaluation(&[]).dimension(length)?;
        let cells = rows
            .checked_mul(width)
            .filter(|&cells| cells <= MAX_CHAR_CELLS)
            .ok_or_else(|| {
                Error::at(
                    length.position,
                    format!("string capacity limit exceeded ({MAX_CHAR_CELLS})"),
                )
            })?;
        if cells > MAX_CHAR_CELLS - self.char_cells {
            return Err(Error::at(
                length.position,
                format!("total character capacity limit exceeded ({MAX_CHAR_CELLS})"),
            ));
        }
        self.char_cells += cells;
        let value = StringValue {
            width,
            cells: vec![0; cells],
        };
        self.values.push(if count.is_some() {
            Value::StringArray(value)
        } else {
            Value::String(value)
        });
        Ok(())
    }

    fn init_char(&mut self, init: &BoundElementInit<'_>, loop_values: &[i64]) -> Result<(), Error> {
        let evaluation = self.evaluation(loop_values);
        let target = &init.target;
        let array = target.array;
        let offset = evaluation.char_offset(target)?;
        if evaluation.string(array).cells[offset] != 0 {
            return Err(evaluation.char_state_error(array, offset, "already initialized"));
        }
        let value = match &init.value {
            BoundInitValue::Alphabet(bytes) => {
                let index = self.rng.sample(0, bytes.len() as i128) as usize;
                bytes[index]
            }
            BoundInitValue::Fixed(expr) => evaluation.character(expr)?,
            BoundInitValue::Range(_) => unreachable!(),
        };
        let (Value::String(string) | Value::StringArray(string)) = &mut self.values[array.slot]
        else {
            unreachable!("character initialization is bound to a string");
        };
        string.cells[offset] = value;
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

fn check_unique_exclusion_count(count: usize, position: Position) -> Result<(), Error> {
    if count >= MAX_UNIQUE_EXCLUSIONS {
        return Err(Error::at(
            position,
            format!("unique exclusion read limit exceeded ({MAX_UNIQUE_EXCLUSIONS})"),
        ));
    }
    Ok(())
}

fn check_unique_payload(width: usize, exclusions: usize, position: Position) -> Result<(), Error> {
    let bytes = width
        .checked_add(exclusions)
        .and_then(|elements| elements.checked_mul(size_of::<i64>()));
    if !bytes.is_some_and(|bytes| bytes <= MAX_UNIQUE_PAYLOAD_BYTES) {
        return Err(Error::at(
            position,
            format!("unique workspace payload limit exceeded ({MAX_UNIQUE_PAYLOAD_BYTES} bytes)"),
        ));
    }
    Ok(())
}

fn unique_work(
    count: usize,
    width: usize,
    exclusions: usize,
    position: Position,
) -> Result<usize, Error> {
    // ceil(log2(e + 1)) = bit_length(e), including e = 0.
    let logarithm = usize::BITS - exclusions.leading_zeros();
    let units = 1 + count as u128 + width as u128 + exclusions as u128 * u128::from(logarithm);
    usize::try_from(units).map_err(|_| {
        Error::at(
            position,
            format!("execution work limit exceeded ({MAX_EXECUTION_WORK})"),
        )
    })
}

fn unique_buffer(capacity: usize, position: Position) -> Result<Vec<i64>, Error> {
    let mut values = Vec::new();
    values.try_reserve_exact(capacity).map_err(|error| {
        Error::at(
            position,
            format!("unable to allocate unique sampling workspace: {error}"),
        )
    })?;
    Ok(values)
}

pub(crate) struct Evaluation<'values, 'loops> {
    pub values: &'values [Value],
    pub loop_values: &'loops [i64],
}

impl<'values> Evaluation<'values, '_> {
    pub(crate) fn condition(&self, condition: &BoundCondition<'_>) -> Result<bool, Error> {
        let condition = match condition {
            BoundCondition::Char(condition) => {
                let left = self.character(&condition.left)?;
                let right = self.character(&condition.right)?;
                return Ok(match condition.op {
                    ComparisonOp::Equal => left == right,
                    ComparisonOp::NotEqual => left != right,
                    _ => unreachable!("character comparison is statically checked"),
                });
            }
            BoundCondition::Int(condition) => condition,
        };
        let left = self.evaluate(&condition.left)?;
        let right = self.evaluate(&condition.right)?;
        Ok(match condition.op {
            ComparisonOp::Equal => left == right,
            ComparisonOp::NotEqual => left != right,
            ComparisonOp::Less => left < right,
            ComparisonOp::LessEqual => left <= right,
            ComparisonOp::Greater => left > right,
            ComparisonOp::GreaterEqual => left >= right,
        })
    }

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
        work: &mut WorkBudget,
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
        work.charge(
            usize::try_from(count).expect("loop width fits in usize"),
            range.upper.position,
        )?;
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

    fn dimension(&self, expr: &BoundExpr<'_>) -> Result<usize, Error> {
        let value = self.evaluate(expr)?;
        if value <= 0 {
            return Err(Error::at(
                expr.position,
                format!("invalid string dimension {value}; expected a positive integer"),
            )
            .with_expression(expr.source));
        }
        usize::try_from(value)
            .ok()
            .filter(|&value| value <= MAX_CHAR_CELLS)
            .ok_or_else(|| {
                Error::at(
                    expr.position,
                    format!("string dimension limit exceeded ({MAX_CHAR_CELLS})"),
                )
                .with_expression(expr.source)
            })
    }

    pub(crate) fn string(&self, array: ArrayRef<'_>) -> &'values StringValue {
        let (Value::String(value) | Value::StringArray(value)) = &self.values[array.slot] else {
            unreachable!("string reference is statically checked");
        };
        value
    }

    fn char_state_error(&self, array: ArrayRef<'_>, offset: usize, state: &str) -> Error {
        let string = self.string(array);
        let subscript = if matches!(self.values[array.slot], Value::StringArray(_)) {
            format!(
                "[{}][{}]",
                offset / string.width + 1,
                offset % string.width + 1
            )
        } else {
            format!("[{}]", offset + 1)
        };
        Error::at(
            array.name.position,
            format!(
                "character cell `{}{subscript}` is {state}",
                array.name.value
            ),
        )
    }

    fn checked_index(
        &self,
        expr: &BoundExpr<'_>,
        length: usize,
        array: ArrayRef<'_>,
    ) -> Result<usize, Error> {
        let index = self.evaluate(expr)?;
        usize::try_from(index)
            .ok()
            .filter(|&index| (1..=length).contains(&index))
            .map(|index| index - 1)
            .ok_or_else(|| {
                Error::at(
                    expr.position,
                    format!(
                        "index {index} is out of bounds for `{}` (length {length}; expected 1..={length})",
                        array.name.value
                    ),
                )
            })
    }

    pub(crate) fn string_row_index(&self, element: &BoundElement<'_>) -> Result<usize, Error> {
        let string = self.string(element.array);
        self.checked_index(
            &element.index,
            string.cells.len() / string.width,
            element.array,
        )
    }

    fn char_offset(&self, element: &BoundElement<'_>) -> Result<usize, Error> {
        let string = self.string(element.array);
        if let Some(column) = &element.second_index {
            // Validate the outer index before evaluating the inner expression.
            let row = self.string_row_index(element)?;
            let column = self.checked_index(column, string.width, element.array)?;
            Ok(row * string.width + column)
        } else {
            self.checked_index(&element.index, string.width, element.array)
        }
    }

    pub(crate) fn char_element(&self, element: &BoundElement<'_>) -> Result<u8, Error> {
        let offset = self.char_offset(element)?;
        let value = self.string(element.array).cells[offset];
        if value == 0 {
            Err(self.char_state_error(element.array, offset, "uninitialized"))
        } else {
            Ok(value)
        }
    }

    pub(crate) fn initialized_string_row(
        &self,
        array: ArrayRef<'_>,
        row: usize,
    ) -> Result<&'values [u8], Error> {
        let string = self.string(array);
        let start = row * string.width;
        let bytes = &string.cells[start..start + string.width];
        if let Some(offset) = bytes.iter().position(|&byte| byte == 0) {
            return Err(self.char_state_error(array, start + offset, "uninitialized"));
        }
        Ok(bytes)
    }

    fn character(&self, expr: &BoundExpr<'_>) -> Result<u8, Error> {
        match &expr.node {
            ExprNode::CharLiteral(value) => Ok(*value),
            ExprNode::Element(element) => self.char_element(element),
            _ => unreachable!("character expression is statically checked"),
        }
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
            ExprNode::CharLiteral(_) => unreachable!("integer expression is statically checked"),
            ExprNode::Variable(reference) => Ok(match reference {
                ScalarRef::Global(slot) => self.scalar(*slot),
                ScalarRef::LoopIndex(index) => self.loop_values[*index],
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

#[cfg(test)]
mod unique_internal_tests {
    use super::*;
    use crate::{parser, validation};

    const POSITION: Position = Position { line: 3, column: 6 };

    #[test]
    fn unique_workspace_payload_boundaries_and_overflow_are_checked() {
        check_unique_payload(999_999, 1_000_000, POSITION).unwrap();
        check_unique_payload(1_000_000, 1_000_000, POSITION).unwrap();
        // The independent W/e limits make a payload overflow unreachable from
        // valid DSL. Exercise the defensive check directly, including overflow.
        for (width, exclusions) in [(1_000_001, 1_000_000), (usize::MAX, 1), (usize::MAX, 0)] {
            let error = check_unique_payload(width, exclusions, POSITION).unwrap_err();
            assert!(
                error
                    .to_string()
                    .starts_with("3:6: unique workspace payload limit exceeded")
            );
        }
        assert!(
            unique_buffer(usize::MAX, POSITION)
                .unwrap_err()
                .to_string()
                .contains("unable to allocate unique sampling workspace")
        );
    }

    #[test]
    fn unique_work_formula_uses_raw_reads_and_does_not_overflow() {
        for (reads, term) in [
            (0, 0),
            (1, 1),
            (2, 4),
            (3, 6),
            (4, 12),
            (1_000_000, 20_000_000),
        ] {
            assert_eq!(unique_work(5, 10, reads, POSITION).unwrap(), 16 + term);
        }
        assert!(unique_work(usize::MAX, usize::MAX, usize::MAX, POSITION).is_err());
        check_unique_exclusion_count(999_999, POSITION).unwrap();
        assert!(check_unique_exclusion_count(1_000_000, POSITION).is_err());
    }

    #[test]
    fn unique_preflight_failures_leave_rng_target_and_integer_count_unchanged() {
        for (body, expected, exhausted_integers, work_left) in [
            (
                "A[2]\nA[2] in 7..=7\nA in unique(1/0..=9)",
                "already initialized",
                false,
                0,
            ),
            ("A[2]\nA in unique(1/0..=9)", "division by zero", false, 0),
            ("A[2]\nA in unique(1..1)", "invalid range", false, 0),
            (
                "A[2]\nA in unique(1..=1000001)",
                "candidate width limit",
                false,
                0,
            ),
            (
                "A[2]\nE[1]\nA in unique(1..=9) except(E)",
                "uninitialized",
                false,
                0,
            ),
            (
                "A[2]\nA in unique(1..=9) except(1/0)",
                "division by zero",
                false,
                0,
            ),
            (
                "A[2]\nA in unique(1..=2) except(1,1)",
                "insufficient unique candidates",
                false,
                0,
            ),
            (
                "A[2]\nA in unique(1..=2)",
                "generated integer limit",
                true,
                0,
            ),
            ("A[2]\nA in unique(1..=2)", "execution work limit", false, 4),
        ] {
            let source = format!("generate {{\n{body}\n}}\ninput {{\n}}\n");
            let program = validation::validate(parser::parse(&source).unwrap()).unwrap();
            let (last, prefix) = program.generate.split_last().unwrap();
            let BoundGenerateStmt::InitUnique(init) = last else {
                panic!("expected unique")
            };
            let mut generator = Generator {
                rng: SplitMix64 { state: 42 },
                values: Vec::new(),
                array_elements: 0,
                generated_integers: 0,
                char_cells: 0,
            };
            generator
                .execute(prefix, &mut Vec::new(), &mut WorkBudget::new(true))
                .unwrap();
            if exhausted_integers {
                generator.generated_integers = MAX_GENERATED_INTEGERS;
            }
            let before_state = generator.rng.state;
            let before_count = generator.generated_integers;
            let before_target = generator.evaluation(&[]).array(init.target).to_vec();
            let error = generator
                .execute(
                    std::slice::from_ref(last),
                    &mut Vec::new(),
                    &mut WorkBudget(Some(work_left)),
                )
                .unwrap_err();
            assert!(error.to_string().contains(expected), "{body}: {error}");
            assert_eq!(generator.rng.state, before_state, "{body}");
            assert_eq!(generator.generated_integers, before_count, "{body}");
            assert_eq!(
                generator.evaluation(&[]).array(init.target),
                before_target,
                "{body}"
            );
        }
    }
}
