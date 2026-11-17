use std::collections::{HashMap, HashSet};

use crate::Error;
use crate::ast::{
    ElementInit, ElementRef, Expr, ExprNode, GenerateStmt, InputStmt, Located, Name, OutputRef,
    OutputRow, Program, RangeExpr,
};
use crate::error::Position;

pub(crate) struct ValidatedProgram<'a> {
    pub generate: Vec<BoundGenerateStmt<'a>>,
    pub input: Vec<BoundInputStmt<'a>>,
    pub slot_count: usize,
}

pub(crate) enum BoundGenerateStmt<'a> {
    Scalar {
        name: Name<'a>,
        range: BoundRange<'a>,
    },
    Array {
        name: Name<'a>,
        length: BoundExpr<'a>,
    },
    InitElement(BoundElementInit<'a>),
    For {
        position: Position,
        range: BoundRange<'a>,
        body: Vec<BoundGenerateStmt<'a>>,
    },
}

impl BoundGenerateStmt<'_> {
    pub(crate) fn position(&self) -> Position {
        match self {
            Self::Scalar { name, .. } | Self::Array { name, .. } => name.position,
            Self::InitElement(init) => init.target.array.name.position,
            Self::For { position, .. } => *position,
        }
    }
}

pub(crate) struct BoundElementInit<'a> {
    pub target: BoundElement<'a>,
    pub range: BoundRange<'a>,
}

pub(crate) enum BoundInputStmt<'a> {
    Row(BoundRow<'a>),
    RowBlock {
        position: Position,
        body: Vec<BoundInputStmt<'a>>,
    },
    For {
        position: Position,
        range: BoundRange<'a>,
        body: Vec<BoundInputStmt<'a>>,
    },
}

impl BoundInputStmt<'_> {
    pub(crate) fn position(&self) -> Position {
        match self {
            Self::Row(row) => row.position,
            Self::RowBlock { position, .. } | Self::For { position, .. } => *position,
        }
    }
}

pub(crate) struct BoundRow<'a> {
    pub position: Position,
    pub items: Vec<Located<BoundOutputRef<'a>>>,
}

pub(crate) enum BoundOutputRef<'a> {
    Scalar(usize),
    WholeArray(ArrayRef<'a>),
    Element(BoundElement<'a>),
}

#[derive(Clone, Copy)]
pub(crate) enum ScalarRef {
    Global(usize),
    LoopIndex(usize),
}

#[derive(Clone, Copy)]
pub(crate) struct ArrayRef<'a> {
    pub name: Name<'a>,
    pub slot: usize,
}

pub(crate) type BoundElement<'a> = ElementRef<'a, ScalarRef, ArrayRef<'a>>;
pub(crate) type BoundExpr<'a> = Expr<'a, ScalarRef, ArrayRef<'a>>;
pub(crate) type BoundRange<'a> = RangeExpr<'a, ScalarRef, ArrayRef<'a>>;
pub(crate) type BoundNode<'a> = ExprNode<'a, ScalarRef, ArrayRef<'a>>;

#[derive(Clone, Copy)]
enum Kind {
    Scalar,
    Array,
}

#[derive(Clone, Copy)]
struct Binding {
    slot: usize,
    kind: Kind,
}

#[derive(Default)]
struct Names<'a> {
    globals: HashMap<&'a str, Binding>,
    loop_variables: HashSet<&'a str>,
}

impl<'a> Names<'a> {
    fn declare(&mut self, name: Name<'a>, kind: Kind) -> Result<(), Error> {
        if self.loop_variables.contains(name.value) {
            return Err(collision(name));
        }
        let slot = self.globals.len();
        match self.globals.entry(name.value) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Binding { slot, kind });
                Ok(())
            }
            std::collections::hash_map::Entry::Occupied(_) => Err(Error::at(
                name.position,
                format!("duplicate declaration of `{}`", name.value),
            )),
        }
    }

    fn register_loop(&mut self, name: Name<'a>) -> Result<(), Error> {
        if self.globals.contains_key(name.value) {
            return Err(collision(name));
        }
        self.loop_variables.insert(name.value);
        Ok(())
    }

    fn register_generate(&mut self, statements: &[GenerateStmt<'a>]) -> Result<(), Error> {
        for statement in statements {
            match statement {
                GenerateStmt::GenerateScalar { name, .. } => self.declare(*name, Kind::Scalar)?,
                GenerateStmt::DeclareArray { name, .. } => self.declare(*name, Kind::Array)?,
                GenerateStmt::For { variable, body, .. } => {
                    self.register_loop(*variable)?;
                    self.register_generate(body)?;
                }
                GenerateStmt::InitElement(_) => {}
            }
        }
        Ok(())
    }

    fn register_input(&mut self, statements: &[InputStmt<'a>]) -> Result<(), Error> {
        for statement in statements {
            match statement {
                InputStmt::For { variable, body, .. } => {
                    self.register_loop(*variable)?;
                    self.register_input(body)?;
                }
                InputStmt::RowBlock { body, .. } => self.register_input(body)?,
                InputStmt::Row(_) => {}
            }
        }
        Ok(())
    }
}

fn collision(name: Name<'_>) -> Error {
    Error::at(
        name.position,
        format!("name collision for `{}`", name.value),
    )
}

pub(crate) fn validate(program: Program<'_>) -> Result<ValidatedProgram<'_>, Error> {
    let mut names = Names::default();
    names.register_generate(&program.generate)?;
    names.register_input(&program.input)?;

    let mut resolver = Resolver {
        names: &names,
        available: 0,
        loop_variables: Vec::new(),
    };
    let generate = resolver.generate_body(program.generate)?;
    let input = resolver.input_body(program.input)?;
    Ok(ValidatedProgram {
        generate,
        input,
        slot_count: names.globals.len(),
    })
}

struct Resolver<'scope, 'a> {
    names: &'scope Names<'a>,
    // Only this prefix of the global slots has been declared at this point.
    available: usize,
    loop_variables: Vec<&'a str>,
}

impl<'a> Resolver<'_, 'a> {
    fn generate_body(
        &mut self,
        statements: Vec<GenerateStmt<'a>>,
    ) -> Result<Vec<BoundGenerateStmt<'a>>, Error> {
        statements
            .into_iter()
            .map(|statement| self.generate_stmt(statement))
            .collect()
    }

    fn generate_stmt(
        &mut self,
        statement: GenerateStmt<'a>,
    ) -> Result<BoundGenerateStmt<'a>, Error> {
        Ok(match statement {
            GenerateStmt::GenerateScalar { name, range } => {
                let range = self.range(range)?;
                self.available += 1;
                BoundGenerateStmt::Scalar { name, range }
            }
            GenerateStmt::DeclareArray { name, length } => {
                let length = self.expr(length)?;
                self.available += 1;
                BoundGenerateStmt::Array { name, length }
            }
            GenerateStmt::InitElement(init) => BoundGenerateStmt::InitElement(self.init(init)?),
            GenerateStmt::For {
                position,
                variable,
                range,
                body,
            } => {
                let range = self.range(range)?;
                self.enter_loop(variable)?;
                let body = self.generate_body(body)?;
                self.loop_variables.pop();
                BoundGenerateStmt::For {
                    position,
                    range,
                    body,
                }
            }
        })
    }

    fn input_body(
        &mut self,
        statements: Vec<InputStmt<'a>>,
    ) -> Result<Vec<BoundInputStmt<'a>>, Error> {
        statements
            .into_iter()
            .map(|statement| self.input_stmt(statement))
            .collect()
    }

    fn input_stmt(&mut self, statement: InputStmt<'a>) -> Result<BoundInputStmt<'a>, Error> {
        Ok(match statement {
            InputStmt::Row(row) => BoundInputStmt::Row(self.row(row)?),
            InputStmt::RowBlock { position, body } => BoundInputStmt::RowBlock {
                position,
                body: self.input_body(body)?,
            },
            InputStmt::For {
                position,
                variable,
                range,
                body,
            } => {
                let range = self.range(range)?;
                self.enter_loop(variable)?;
                let body = self.input_body(body)?;
                self.loop_variables.pop();
                BoundInputStmt::For {
                    position,
                    range,
                    body,
                }
            }
        })
    }

    fn enter_loop(&mut self, variable: Name<'a>) -> Result<(), Error> {
        if self.loop_variables.contains(&variable.value) {
            return Err(collision(variable));
        }
        self.loop_variables.push(variable.value);
        Ok(())
    }

    fn global(&self, name: Name<'a>) -> Result<Binding, Error> {
        let message = match self.names.globals.get(name.value) {
            Some(&binding) if binding.slot < self.available => return Ok(binding),
            Some(_) => format!("`{}` is not available before its declaration", name.value),
            None if self.names.loop_variables.contains(name.value) => {
                format!("loop variable `{}` is outside this scope", name.value)
            }
            None => format!("undefined variable `{}`", name.value),
        };
        Err(Error::at(name.position, message))
    }

    fn loop_index(&self, name: Name<'a>) -> Option<usize> {
        self.loop_variables
            .iter()
            .position(|&variable| variable == name.value)
    }

    fn element(&self, element: ElementRef<'a>) -> Result<BoundElement<'a>, Error> {
        let name = element.array;
        if self.loop_index(name).is_some() {
            return Err(Error::at(
                name.position,
                format!("loop variable `{}` cannot be indexed", name.value),
            ));
        }
        let binding = self.global(name)?;
        match binding.kind {
            Kind::Scalar => Err(Error::at(
                name.position,
                format!("`{}` is a scalar; expected an array", name.value),
            )),
            Kind::Array => Ok(BoundElement {
                array: ArrayRef {
                    name,
                    slot: binding.slot,
                },
                index: self.expr(element.index)?,
            }),
        }
    }

    fn init(&self, init: ElementInit<'a>) -> Result<BoundElementInit<'a>, Error> {
        Ok(BoundElementInit {
            target: self.element(init.target)?,
            range: self.range(init.range)?,
        })
    }

    fn range(&self, range: RangeExpr<'a>) -> Result<BoundRange<'a>, Error> {
        Ok(BoundRange {
            lower: self.expr(range.lower)?,
            upper: self.expr(range.upper)?,
            end: range.end,
            source: range.source,
        })
    }

    fn expr(&self, expr: Expr<'a>) -> Result<BoundExpr<'a>, Error> {
        Ok(Expr {
            position: expr.position,
            source: expr.source,
            node: self.node(expr.node)?,
        })
    }

    fn node(&self, node: ExprNode<'a>) -> Result<BoundNode<'a>, Error> {
        Ok(match node {
            ExprNode::Literal(value) => ExprNode::Literal(value),
            ExprNode::Variable(name) => {
                let reference = if let Some(index) = self.loop_index(name) {
                    ScalarRef::LoopIndex(index)
                } else {
                    let binding = self.global(name)?;
                    if let Kind::Array = binding.kind {
                        return Err(Error::at(
                            name.position,
                            format!("`{}` is an array; expected a scalar", name.value),
                        ));
                    }
                    ScalarRef::Global(binding.slot)
                };
                ExprNode::Variable(reference)
            }
            ExprNode::Element(element) => ExprNode::Element(Box::new(self.element(*element)?)),
            ExprNode::Unary { op, operand } => ExprNode::Unary {
                op,
                operand: Box::new(self.node(*operand)?),
            },
            ExprNode::Binary { first, rest } => ExprNode::Binary {
                first: Box::new(self.node(*first)?),
                rest: rest
                    .into_iter()
                    .map(|(op, operand)| Ok((op, self.node(operand)?)))
                    .collect::<Result<_, Error>>()?,
            },
        })
    }

    fn row(&self, row: OutputRow<'a>) -> Result<BoundRow<'a>, Error> {
        let items = row
            .items
            .into_iter()
            .map(|item| self.output_ref(item))
            .collect::<Result<_, _>>()?;
        Ok(BoundRow {
            position: row.position,
            items,
        })
    }

    fn output_ref(&self, item: OutputRef<'a>) -> Result<Located<BoundOutputRef<'a>>, Error> {
        let (name, value) = match item {
            OutputRef::Name(name) => {
                if self.loop_index(name).is_some() {
                    return Err(Error::at(
                        name.position,
                        format!("loop variable `{}` cannot be output directly", name.value),
                    ));
                }
                let Binding { slot, kind } = self.global(name)?;
                let value = match kind {
                    Kind::Scalar => BoundOutputRef::Scalar(slot),
                    Kind::Array => BoundOutputRef::WholeArray(ArrayRef { name, slot }),
                };
                (name, value)
            }
            OutputRef::Element(element) => (
                element.array,
                BoundOutputRef::Element(self.element(element)?),
            ),
        };
        Ok(Located {
            value,
            position: name.position,
        })
    }
}
