use crate::Error;
use crate::ast::{
    BinaryOp, Constraint, Declaration, Expr, ExprNode, For, Input, Located, Name, RangeEnd,
    RangeExpr, Row, Statement, UnaryOp,
};
use crate::error::Position;

const MAX_EXPRESSION_NESTING: usize = 128;

pub(crate) fn parse(source: &str) -> Result<Input<'_>, Error> {
    let mut lines = source
        .lines()
        .enumerate()
        .map(|(index, text)| Line::new(index + 1, text));
    let mut next_line = |expected, keep_blank| {
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

    next_line("`input {`", false)?.block_header("input")?;
    let mut statements = Vec::new();
    loop {
        let mut line = next_line("an input declaration, `for`, or `}`", false)?;
        let position = line.position();
        if line.consume("}") {
            line.finish()?;
            if statements.is_empty() {
                return Err(Error::at(position, "input block is empty"));
            }
            break;
        }
        if line.starts_keyword("for") {
            let (index, range) = line.for_header()?;
            let mut rows = Vec::new();
            loop {
                let mut body = next_line("an input declaration or `}` in the `for` body", true)?;
                if body.consume("}") {
                    body.finish()?;
                    break;
                }
                if body.starts_keyword("for") {
                    return Err(Error::at(body.position(), "nested `for` is not supported"));
                }
                rows.push(body.input_row()?);
            }
            statements.push(Statement::For(For { index, range, rows }));
        } else {
            statements.push(Statement::Row(line.input_row()?));
        }
    }

    next_line("`constraints {`", false)?.block_header("constraints")?;
    let mut constraints = Vec::new();
    loop {
        let mut line = next_line("a constraint or `}`", false)?;
        if line.consume("}") {
            line.finish()?;
            break;
        }
        constraints.push(line.constraint()?);
    }
    if let Some(line) = lines.find(|line| !line.remaining().is_empty()) {
        return Err(Error::at(
            line.position(),
            "unexpected text after constraints block",
        ));
    }
    Ok(Input {
        statements,
        constraints,
    })
}

struct Line<'a> {
    number: usize,
    length: usize,
    rest: &'a str,
}

impl<'a> Line<'a> {
    fn new(number: usize, text: &'a str) -> Self {
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
        self.remaining()
            .strip_prefix(keyword)
            .is_some_and(|rest| !rest.bytes().next().is_some_and(identifier_continue))
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

    fn expression(&mut self) -> Result<Expr<'a>, Error> {
        let position = self.position();
        let source = self.remaining();
        let node = self.sum(0)?;
        let source = &source[..source.len() - self.rest.len()];
        Ok(Expr {
            position,
            source,
            node,
        })
    }

    fn sum(&mut self, nesting: usize) -> Result<ExprNode<Name<'a>>, Error> {
        self.binary_chain(
            nesting,
            Self::product,
            &[("+", BinaryOp::Add), ("-", BinaryOp::Subtract)],
        )
    }

    fn product(&mut self, nesting: usize) -> Result<ExprNode<Name<'a>>, Error> {
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
        operand: fn(&mut Self, usize) -> Result<ExprNode<Name<'a>>, Error>,
        operators: &[(&str, BinaryOp)],
    ) -> Result<ExprNode<Name<'a>>, Error> {
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

    fn unary(&mut self, nesting: usize) -> Result<ExprNode<Name<'a>>, Error> {
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
            Some(byte) if identifier_start(byte) => self.variable_name().map(ExprNode::Variable),
            Some(byte) if byte.is_ascii_digit() => self.literal(position, false),
            _ => Err(Error::at(
                position,
                "expected an integer, scalar variable or `(`",
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

    fn literal(&mut self, position: Position, negative: bool) -> Result<ExprNode<Name<'a>>, Error> {
        self.digits()
            .parse::<i128>()
            .ok()
            .and_then(|value| i64::try_from(if negative { -value } else { value }).ok())
            .map(ExprNode::Literal)
            .ok_or_else(|| Error::at(position, "integer literal is outside the i64 range"))
    }

    fn declaration(&mut self) -> Result<Declaration<'a>, Error> {
        let name = self.variable_name()?;
        let length = if self.consume("[") {
            let length = self.expression()?;
            if self.remaining().starts_with("..") {
                return Err(Error::at(
                    self.position(),
                    "expected an array length expression such as `A[N]`, found a range",
                ));
            }
            self.expect("]")?;
            Some(length)
        } else {
            None
        };
        Ok(Declaration { name, length })
    }

    fn input_row(&mut self) -> Result<Row<'a>, Error> {
        let position = self.position();
        let mut declarations = Vec::new();
        while !self.remaining().is_empty() {
            if !declarations.is_empty() && self.rest.len() == self.remaining().len() {
                return Err(Error::at(
                    self.position(),
                    "expected whitespace between declarations",
                ));
            }
            declarations.push(self.declaration()?);
        }
        Ok(Row {
            position,
            declarations,
        })
    }

    fn constraint(&mut self) -> Result<Constraint<'a>, Error> {
        if self.starts_keyword("for") {
            return Err(Error::at(
                self.position(),
                "`for` is only supported in `input`",
            ));
        }
        let target = self.variable_name()?;
        if self.remaining().starts_with('[') {
            return Err(Error::at(
                self.position(),
                "constraints use a bare variable name, e.g. `A in 1..=10`",
            ));
        }
        self.keyword("in")?;
        let range = self.range()?;
        self.finish()?;
        Ok(Constraint { target, range })
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
        let lower = self.expression()?;
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
        let upper = self.expression()?;
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
