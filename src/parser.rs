use crate::Error;
use crate::ast::{Constraint, Declaration, Endpoint, Input, Located, Name, Value};
use crate::error::Position;

pub(crate) fn parse(source: &str) -> Result<Input<'_>, Error> {
    let mut lines = source
        .lines()
        .enumerate()
        .map(|(index, text)| Line::new(index + 1, text))
        .filter(|line| !line.remaining().is_empty());
    let mut next_line = |expected| {
        lines.next().ok_or_else(|| {
            let position = Position {
                line: source.bytes().filter(|&byte| byte == b'\n').count() + 1,
                column: source.rsplit('\n').next().unwrap_or_default().len() + 1,
            };
            Error::at(position, format!("expected {expected}"))
        })
    };

    next_line("`input {`")?.block_header("input")?;
    let mut rows = Vec::new();
    loop {
        let mut line = next_line("an input declaration or `}`")?;
        let position = line.position();
        if line.consume("}") {
            line.finish()?;
            if rows.is_empty() {
                return Err(Error::at(position, "input block is empty"));
            }
            break;
        }
        rows.push(line.input_row()?);
    }

    next_line("`constraints {`")?.block_header("constraints")?;
    let mut constraints = Vec::new();
    loop {
        let mut line = next_line("a constraint or `}`")?;
        if line.consume("}") {
            line.finish()?;
            break;
        }
        constraints.push(line.constraint()?);
    }
    if let Some(line) = lines.next() {
        return Err(Error::at(
            line.position(),
            "unexpected text after constraints block",
        ));
    }
    Ok(Input { rows, constraints })
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
            .take_while(|&byte| identifier_start(byte) || byte.is_ascii_digit())
            .count();
        let (value, rest) = rest.split_at(length);
        self.rest = rest;
        Ok(Located { value, position })
    }

    fn endpoint(&mut self) -> Result<Endpoint<'a>, Error> {
        let position = self.position();
        let rest = self.remaining();
        if rest.bytes().next().is_some_and(identifier_start) {
            let name = self.identifier()?;
            return Ok(Located {
                value: Value::Scalar(name.value),
                position,
            });
        }

        let digits = rest.strip_prefix('-').unwrap_or(rest);
        let digit_count = digits.bytes().take_while(u8::is_ascii_digit).count();
        if digit_count == 0 {
            return Err(Error::at(
                position,
                "expected an integer or scalar variable",
            ));
        }
        let length = (rest.len() - digits.len()) + digit_count;
        let (literal, rest) = rest.split_at(length);
        let value = literal
            .parse()
            .map_err(|_| Error::at(position, "integer literal is outside the i64 range"))?;
        self.rest = rest;
        Ok(Located {
            value: Value::Integer(value),
            position,
        })
    }

    fn declaration(&mut self) -> Result<Declaration<'a>, Error> {
        let name = self.identifier()?;
        let length = if self.consume("[") {
            let first = self.endpoint()?;
            if !matches!(first.value, Value::Integer(1)) {
                return Err(Error::at(first.position, "array indices must start at 1"));
            }
            self.expect("..")?;
            let length = self.endpoint()?;
            self.expect("]")?;
            Some(length)
        } else {
            None
        };
        Ok(Declaration { name, length })
    }

    fn input_row(&mut self) -> Result<Vec<Declaration<'a>>, Error> {
        let mut row = vec![self.declaration()?];
        while !self.remaining().is_empty() {
            if self.rest.len() == self.remaining().len() {
                return Err(Error::at(
                    self.position(),
                    "expected whitespace between declarations",
                ));
            }
            row.push(self.declaration()?);
        }
        Ok(row)
    }

    fn constraint(&mut self) -> Result<Constraint<'a>, Error> {
        let target = self.identifier()?;
        let array_elements = self.consume("[");
        if array_elements {
            self.expect("*")?;
            self.expect("]")?;
        }
        self.keyword("in")?;
        let lower = self.endpoint()?;
        self.expect("..")?;
        let upper = self.endpoint()?;
        self.finish()?;
        Ok(Constraint {
            target,
            array_elements,
            lower,
            upper,
        })
    }
}

fn identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}
