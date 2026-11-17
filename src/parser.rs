use crate::Error;
use crate::ast::{
    BinaryOp, ElementInit, ElementRef, Expr, ExprNode, GenerateStmt, InputStmt, Located, Name,
    OutputRef, OutputRow, Program, RangeEnd, RangeExpr, UnaryOp,
};
use crate::error::Position;

const MAX_EXPRESSION_NESTING: usize = 128;
const MAX_CONTROL_NESTING: usize = 32;

pub(crate) fn parse(source: &str) -> Result<Program<'_>, Error> {
    let mut lines = source
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, text)| Line::new(index + 1, text));
    let mut next_line = |expected: &str, keep_blank| {
        lines
            .find(|line| keep_blank || !line.remaining().is_empty())
            .ok_or_else(|| {
                let position = Position {
                    line: source.bytes().filter(|&byte| byte == b'\n').count() + 1,
                    column: source.rsplit('\n').next().unwrap_or_default().len() + 1,
                };
                Error::at(position, format!("expected {expected}"))
            })
    };

    next_line("`generate {`", false)?.block_header("generate")?;
    let generate = generate_body(&mut next_line, 0)?;
    next_line("`input {`", false)?.block_header("input")?;
    let input = input_body(&mut next_line, 0, false)?;

    if let Some(line) = lines.find(|line| !line.remaining().is_empty()) {
        return Err(Error::at(
            line.position(),
            "unexpected text after input block",
        ));
    }
    Ok(Program { generate, input })
}

fn generate_body<'a>(
    next_line: &mut impl FnMut(&str, bool) -> Result<Line<'a>, Error>,
    depth: usize,
) -> Result<Vec<GenerateStmt<'a>>, Error> {
    let expected = if depth == 0 {
        "a generation operation, `for`, or `}`"
    } else {
        "an element initialization, `for`, or `}`"
    };
    let mut body = Vec::new();
    loop {
        let mut line = next_line(expected, false)?;
        if line.consume("}") {
            line.finish()?;
            return Ok(body);
        }
        if line.starts_keyword("for") {
            let position = line.position();
            check_control_depth(depth, position)?;
            let (variable, range) = line.for_header()?;
            body.push(GenerateStmt::For {
                position,
                variable,
                range,
                body: generate_body(next_line, depth + 1)?,
            });
        } else {
            let position = line.position();
            let statement = line.generate_line()?;
            if depth != 0 && !matches!(statement, GenerateStmt::InitElement(_)) {
                return Err(Error::at(
                    position,
                    "only array element initialization and nested `for` are allowed in a generate loop",
                ));
            }
            body.push(statement);
        }
    }
}

fn input_body<'a>(
    next_line: &mut impl FnMut(&str, bool) -> Result<Line<'a>, Error>,
    depth: usize,
    in_row: bool,
) -> Result<Vec<InputStmt<'a>>, Error> {
    let mut body = Vec::new();
    loop {
        let mut line = next_line("an output line, `for`, `row`, or `}`", !in_row)?;
        if line.consume("}") {
            line.finish()?;
            return Ok(body);
        }
        if line.starts_keyword("for") {
            let position = line.position();
            check_control_depth(depth, position)?;
            let (variable, range) = line.for_header()?;
            body.push(InputStmt::For {
                position,
                variable,
                range,
                body: input_body(next_line, depth + 1, in_row)?,
            });
        } else if line
            .remaining()
            .strip_prefix("row")
            .is_some_and(|tail| tail.trim_matches([' ', '\t']) == "{")
        {
            let position = line.position();
            if in_row {
                return Err(Error::at(position, "nested `row` is not allowed"));
            }
            check_control_depth(depth, position)?;
            body.push(InputStmt::RowBlock {
                position,
                body: input_body(next_line, depth + 1, true)?,
            });
        } else {
            body.push(InputStmt::Row(line.output_row()?));
        }
    }
}

fn check_control_depth(depth: usize, position: Position) -> Result<(), Error> {
    if depth == MAX_CONTROL_NESTING {
        return Err(Error::at(
            position,
            format!("control nesting limit exceeded ({MAX_CONTROL_NESTING})"),
        ));
    }
    Ok(())
}

struct Line<'a> {
    number: usize,
    length: usize,
    rest: &'a str,
}

impl<'a> Line<'a> {
    fn new(number: usize, text: &'a str) -> Self {
        let text = text
            .strip_suffix('\n')
            .map_or(text, |line| line.strip_suffix('\r').unwrap_or(line));
        Self {
            number,
            length: text.len(),
            rest: text,
        }
    }

    fn remaining(&self) -> &'a str {
        self.rest.trim_start_matches([' ', '\t'])
    }

    fn position(&self) -> Position {
        Position {
            line: self.number,
            column: self.length - self.remaining().len() + 1,
        }
    }

    fn consume(&mut self, symbol: &str) -> bool {
        if let Some(rest) = self.remaining().strip_prefix(symbol) {
            self.rest = rest;
            return true;
        }
        false
    }

    fn expect(&mut self, symbol: &str) -> Result<(), Error> {
        if self.consume(symbol) {
            return Ok(());
        }
        Err(Error::at(self.position(), format!("expected `{symbol}`")))
    }

    fn keyword(&mut self, keyword: &str) -> Result<(), Error> {
        let name = self.identifier()?;
        if name.value == keyword {
            return Ok(());
        }
        Err(Error::at(name.position, format!("expected `{keyword}`")))
    }

    fn starts_keyword(&self, keyword: &str) -> bool {
        starts_keyword(self.remaining(), keyword)
    }

    fn finish(&self) -> Result<(), Error> {
        if self.remaining().is_empty() {
            return Ok(());
        }
        Err(Error::at(self.position(), "expected end of line"))
    }

    fn block_header(&mut self, name: &str) -> Result<(), Error> {
        self.keyword(name)?;
        self.expect("{")?;
        self.finish()
    }

    fn identifier(&mut self) -> Result<Name<'a>, Error> {
        let position = self.position();
        let rest = self.remaining();
        if !rest.bytes().next().is_some_and(identifier_start) {
            return Err(Error::at(position, "expected an identifier"));
        }
        let length = rest
            .bytes()
            .take_while(|&byte| identifier_continue(byte))
            .count();
        let (value, rest) = rest.split_at(length);
        self.rest = rest;
        Ok(Located { value, position })
    }

    fn variable_name(&mut self) -> Result<Name<'a>, Error> {
        let name = self.identifier()?;
        if name.value == "for" {
            return Err(Error::at(name.position, "`for` is a reserved keyword"));
        }
        Ok(name)
    }

    fn expression(&mut self, nesting: usize) -> Result<Expr<'a>, Error> {
        let position = self.position();
        let source = self.remaining();
        let node = self.sum(nesting)?;
        let source = &source[..source.len() - self.rest.len()];
        Ok(Expr {
            position,
            source,
            node,
        })
    }

    fn sum(&mut self, nesting: usize) -> Result<ExprNode<'a>, Error> {
        self.binary_chain(
            nesting,
            Self::product,
            &[("+", BinaryOp::Add), ("-", BinaryOp::Subtract)],
        )
    }

    fn product(&mut self, nesting: usize) -> Result<ExprNode<'a>, Error> {
        self.binary_chain(
            nesting,
            Self::unary,
            &[
                ("*", BinaryOp::Multiply),
                ("/", BinaryOp::Divide),
                ("%", BinaryOp::Remainder),
            ],
        )
    }

    fn binary_chain(
        &mut self,
        nesting: usize,
        operand: fn(&mut Self, usize) -> Result<ExprNode<'a>, Error>,
        operators: &[(&str, BinaryOp)],
    ) -> Result<ExprNode<'a>, Error> {
        let first = operand(self, nesting)?;
        let mut rest = Vec::new();
        loop {
            let position = self.position();
            let Some(value) = operators
                .iter()
                .find_map(|&(symbol, op)| self.consume(symbol).then_some(op))
            else {
                break;
            };
            rest.push((Located { value, position }, operand(self, nesting)?));
        }
        if rest.is_empty() {
            Ok(first)
        } else {
            Ok(ExprNode::Binary {
                first: Box::new(first),
                rest,
            })
        }
    }

    fn unary(&mut self, nesting: usize) -> Result<ExprNode<'a>, Error> {
        let position = self.position();
        if nesting > MAX_EXPRESSION_NESTING {
            return Err(Error::at(
                position,
                format!("expression nesting limit exceeded ({MAX_EXPRESSION_NESTING})"),
            ));
        }

        if self.consume("-") {
            // A signed literal can represent i64::MIN directly.
            if self
                .remaining()
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_digit)
            {
                return self.literal(position, true);
            }
            let op = Located {
                value: UnaryOp::Minus,
                position,
            };
            let operand = Box::new(self.unary(nesting + 1)?);
            return Ok(ExprNode::Unary { op, operand });
        }
        if self.consume("+") {
            let op = Located {
                value: UnaryOp::Plus,
                position,
            };
            let operand = Box::new(self.unary(nesting + 1)?);
            return Ok(ExprNode::Unary { op, operand });
        }
        if self.consume("(") {
            let node = self.sum(nesting + 1)?;
            self.expect(")")?;
            return Ok(node);
        }
        match self.remaining().bytes().next() {
            Some(byte) if identifier_start(byte) => {
                let name = self.variable_name()?;
                if self.consume("[") {
                    let element = self.element_ref(name, nesting + 1)?;
                    Ok(ExprNode::Element(Box::new(element)))
                } else {
                    Ok(ExprNode::Variable(name))
                }
            }
            Some(byte) if byte.is_ascii_digit() => self.literal(position, false),
            _ => Err(Error::at(
                position,
                "expected an integer, variable, array element or `(`",
            )),
        }
    }

    fn digits(&mut self) -> &'a str {
        let rest = self.remaining();
        let length = rest.bytes().take_while(u8::is_ascii_digit).count();
        let (digits, rest) = rest.split_at(length);
        self.rest = rest;
        digits
    }

    fn literal(&mut self, position: Position, negative: bool) -> Result<ExprNode<'a>, Error> {
        self.digits()
            .parse::<i128>()
            .ok()
            .and_then(|value| i64::try_from(if negative { -value } else { value }).ok())
            .map(ExprNode::Literal)
            .ok_or_else(|| Error::at(position, "integer literal is outside the i64 range"))
    }

    fn element_ref(&mut self, array: Name<'a>, nesting: usize) -> Result<ElementRef<'a>, Error> {
        let index = self.expression(nesting)?;
        self.expect("]")?;
        Ok(ElementRef { array, index })
    }

    fn generate_line(&mut self) -> Result<GenerateStmt<'a>, Error> {
        let name = self.variable_name()?;
        let statement = if self.consume("[") {
            if self.has_init_range() {
                let target = self.element_ref(name, 1)?;
                self.keyword("in")?;
                let range = self.range()?;
                GenerateStmt::InitElement(ElementInit { target, range })
            } else {
                let length = self.expression(0)?;
                self.expect("]")?;
                GenerateStmt::DeclareArray { name, length }
            }
        } else {
            self.keyword("in")?;
            let range = self.range()?;
            GenerateStmt::GenerateScalar { name, range }
        };
        self.finish()?;
        Ok(statement)
    }

    // Classify the brackets before parsing their expression, since a length
    // starts at depth 0 and an element subscript starts at depth 1.
    fn has_init_range(&self) -> bool {
        let remaining = self.remaining();
        let mut nested = 0;
        for (offset, byte) in remaining.bytes().enumerate() {
            match byte {
                b'[' => nested += 1,
                b']' if nested == 0 => {
                    return starts_keyword(
                        remaining[offset + 1..].trim_start_matches([' ', '\t']),
                        "in",
                    );
                }
                b']' => nested -= 1,
                _ => {}
            }
        }
        false
    }

    fn output_row(&mut self) -> Result<OutputRow<'a>, Error> {
        let position = Position {
            line: self.number,
            column: 1,
        };
        let mut items = Vec::new();
        while !self.remaining().is_empty() {
            if !items.is_empty() && self.rest.len() == self.remaining().len() {
                return Err(Error::at(
                    self.position(),
                    "expected whitespace between output references",
                ));
            }
            let name = self.variable_name()?;
            let item = if self.consume("[") {
                OutputRef::Element(self.element_ref(name, 1)?)
            } else {
                OutputRef::Name(name)
            };
            items.push(item);
        }
        Ok(OutputRow { position, items })
    }

    fn for_header(&mut self) -> Result<(Name<'a>, RangeExpr<'a>), Error> {
        self.keyword("for")?;
        let index = self.variable_name()?;
        self.keyword("in")?;
        let range = self.range()?;
        self.expect("{")?;
        self.finish()?;
        Ok((index, range))
    }

    fn range(&mut self) -> Result<RangeExpr<'a>, Error> {
        let source = self.remaining();
        let lower = self.expression(0)?;
        let position = self.position();
        for invalid in ["...", "..=="] {
            if self.consume(invalid) {
                return Err(Error::at(
                    position,
                    format!("invalid range operator `{invalid}`"),
                ));
            }
        }
        let end = if self.consume("..=") {
            RangeEnd::Inclusive
        } else if self.consume("..") {
            RangeEnd::Exclusive
        } else {
            return Err(Error::at(position, "expected range operator `..` or `..=`"));
        };
        let upper = self.expression(0)?;
        let source = &source[..source.len() - self.rest.len()];
        Ok(RangeExpr {
            lower,
            upper,
            end,
            source,
        })
    }
}

fn identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn identifier_continue(byte: u8) -> bool {
    identifier_start(byte) || byte.is_ascii_digit()
}

fn starts_keyword(text: &str, keyword: &str) -> bool {
    text.strip_prefix(keyword)
        .is_some_and(|rest| !rest.bytes().next().is_some_and(identifier_continue))
}
