use crate::Error;
use crate::ast::{
    BinaryOp, ComparisonOp, Condition, ElementInit, ElementRef, Expr, ExprNode, GenerateStmt,
    InitValue, InputStmt, Located, Name, OutputRef, OutputRow, Program, RangeEnd, RangeExpr,
    UnaryOp,
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
    let (generate, _) = generate_body(&mut next_line, 0, false)?;
    next_line("`input {`", false)?.block_header("input")?;
    let (input, _) = input_body(&mut next_line, 0, false, false)?;

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
    allow_else: bool,
) -> Result<(Vec<GenerateStmt<'a>>, bool), Error> {
    let expected = if depth == 0 {
        "a generation operation, `for`, `if`, or `}`"
    } else {
        "an element initialization, `for`, `if`, or `}`"
    };
    let mut body = Vec::new();
    loop {
        let mut line = next_line(expected, false)?;
        if line.consume("}") {
            return Ok((body, line.close_block(allow_else)?));
        }
        if line.starts_keyword("for") {
            let position = line.position();
            check_control_depth(depth, position)?;
            let (variable, range) = line.for_header()?;
            body.push(GenerateStmt::For {
                position,
                variable,
                range,
                body: generate_body(next_line, depth + 1, false)?.0,
            });
        } else if line.is_if_header() {
            let position = line.position();
            check_control_depth(depth, position)?;
            let condition = line.if_header()?;
            let (then_body, has_else) = generate_body(next_line, depth + 1, true)?;
            let else_body = if has_else {
                Some(generate_body(next_line, depth + 1, false)?.0)
            } else {
                None
            };
            body.push(GenerateStmt::If {
                position,
                condition,
                then_body,
                else_body,
            });
        } else {
            let position = line.position();
            let statement = line.generate_line()?;
            if depth != 0 && !matches!(statement, GenerateStmt::InitElement(_)) {
                return Err(Error::at(
                    position,
                    "only array element initialization, `for`, and `if` are allowed in a generate control block",
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
    allow_else: bool,
) -> Result<(Vec<InputStmt<'a>>, bool), Error> {
    let mut body = Vec::new();
    loop {
        let mut line = next_line("an output line, `for`, `if`, `row`, or `}`", !in_row)?;
        if line.consume("}") {
            return Ok((body, line.close_block(allow_else)?));
        }
        if line.starts_keyword("for") {
            let position = line.position();
            check_control_depth(depth, position)?;
            let (variable, range) = line.for_header()?;
            body.push(InputStmt::For {
                position,
                variable,
                range,
                body: input_body(next_line, depth + 1, in_row, false)?.0,
            });
        } else if line.is_if_header() {
            let position = line.position();
            check_control_depth(depth, position)?;
            let condition = line.if_header()?;
            let (then_body, has_else) = input_body(next_line, depth + 1, in_row, true)?;
            let else_body = if has_else {
                Some(input_body(next_line, depth + 1, in_row, false)?.0)
            } else {
                None
            };
            body.push(InputStmt::If {
                position,
                condition,
                then_body,
                else_body,
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
                body: input_body(next_line, depth + 1, true, false)?.0,
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

    // `else` is recognized only on the closing line of its matching `if`.
    fn close_block(&mut self, allow_else: bool) -> Result<bool, Error> {
        if allow_else && self.starts_keyword("else") {
            self.block_header("else")?;
            Ok(true)
        } else {
            self.finish()?;
            Ok(false)
        }
    }

    fn is_if_header(&self) -> bool {
        self.starts_keyword("if")
            && self
                .remaining()
                .trim_end_matches([' ', '\t'])
                .ends_with('{')
    }

    fn if_header(&mut self) -> Result<Condition<'a>, Error> {
        self.keyword("if")?;
        let left = self.expression(0)?;
        let position = self.position();
        let op = [
            ("==", ComparisonOp::Equal),
            ("!=", ComparisonOp::NotEqual),
            ("<=", ComparisonOp::LessEqual),
            (">=", ComparisonOp::GreaterEqual),
            ("<", ComparisonOp::Less),
            (">", ComparisonOp::Greater),
        ]
        .into_iter()
        .find_map(|(token, op)| self.consume(token).then_some(op))
        .ok_or_else(|| Error::at(position, "expected an integer comparison operator"))?;
        let right = self.expression(0)?;
        self.expect("{")?;
        self.finish()?;
        Ok(Condition {
            left,
            op,
            op_position: position,
            right,
        })
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
            Some(b'\'') => {
                let bytes = self.quoted(b'\'')?;
                if bytes.len() != 1 {
                    return Err(Error::at(position, "expected exactly one ASCII character"));
                }
                Ok(ExprNode::CharLiteral(bytes[0]))
            }
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
        let second_index = if self.consume("[") {
            let index = self.expression(nesting + 1)?;
            self.expect("]")?;
            Some(index)
        } else {
            None
        };
        Ok(ElementRef {
            array,
            index,
            second_index,
        })
    }

    fn generate_line(&mut self) -> Result<GenerateStmt<'a>, Error> {
        let name = self.variable_name()?;
        let statement = if self.consume("[") {
            if self.has_initialization() {
                let target = self.element_ref(name, 1)?;
                let value = if self.consume("=") {
                    InitValue::Fixed(self.expression(0)?)
                } else {
                    self.keyword("in")?;
                    if self.remaining().starts_with('"') {
                        InitValue::Alphabet(self.alphabet()?)
                    } else {
                        InitValue::Range(self.range()?)
                    }
                };
                GenerateStmt::InitElement(ElementInit { target, value })
            } else {
                let length = self.expression(0)?;
                self.expect("]")?;
                if self.consume(":") {
                    self.string_declaration(name, Some(length))?
                } else {
                    GenerateStmt::DeclareArray { name, length }
                }
            }
        } else if self.consume(":") {
            self.string_declaration(name, None)?
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
    fn has_initialization(&self) -> bool {
        let remaining = self.remaining();
        let mut nested = 0;
        for (offset, byte) in remaining.bytes().enumerate() {
            match byte {
                b'[' => nested += 1,
                b']' if nested == 0 => {
                    let tail = remaining[offset + 1..].trim_start_matches([' ', '\t']);
                    return tail.starts_with('[')
                        || starts_keyword(tail, "in")
                        || tail.starts_with('=');
                }
                b']' => nested -= 1,
                _ => {}
            }
        }
        false
    }

    fn string_declaration(
        &mut self,
        name: Name<'a>,
        count: Option<Expr<'a>>,
    ) -> Result<GenerateStmt<'a>, Error> {
        self.keyword("string")?;
        self.expect("[")?;
        let length = self.expression(0)?;
        self.expect("]")?;
        Ok(GenerateStmt::DeclareString {
            name,
            count,
            length,
        })
    }

    // Read literal bytes without trimming whitespace inside quotes.
    fn quoted(&mut self, quote: u8) -> Result<Vec<u8>, Error> {
        let position = self.position();
        self.expect(if quote == b'\'' { "'" } else { "\"" })?;
        let mut bytes = Vec::new();
        loop {
            let position = Position {
                line: self.number,
                column: self.length - self.rest.len() + 1,
            };
            let Some(byte) = self.rest.bytes().next() else {
                return Err(Error::at(position, "unterminated quoted literal"));
            };
            if byte == quote {
                self.rest = &self.rest[1..];
                break;
            }
            if byte == b'\\' {
                self.rest = &self.rest[1..];
                let Some(escaped @ (b'\\' | b'\'' | b'"')) = self.rest.bytes().next() else {
                    return Err(Error::at(
                        position,
                        "invalid escape; expected escaped quote or backslash",
                    ));
                };
                bytes.push(escaped);
            } else if (0x21..=0x7e).contains(&byte) {
                bytes.push(byte);
            } else {
                return Err(Error::at(
                    position,
                    "expected a non-whitespace, non-control ASCII character",
                ));
            }
            self.rest = &self.rest[1..];
        }
        if bytes.is_empty() {
            return Err(Error::at(
                position,
                "empty character literal or candidate alphabet",
            ));
        }
        Ok(bytes)
    }

    fn alphabet(&mut self) -> Result<Located<Vec<u8>>, Error> {
        let position = self.position();
        let bytes = self.quoted(b'"')?;
        Ok(Located {
            value: bytes,
            position,
        })
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
