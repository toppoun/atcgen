use std::collections::{HashMap, HashSet};

use crate::Error;
use crate::ast::{
    ComparisonOp, Condition, ElementInit, ElementRef, Expr, ExprNode, GenerateStmt, InitValue,
    InputStmt, Located, Name, OutputRef, OutputRow, Program, RangeExpr, UniqueInit,
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
    String {
        name: Name<'a>,
        count: Option<BoundExpr<'a>>,
        length: BoundExpr<'a>,
    },
    InitElement(BoundElementInit<'a>),
    InitUnique(BoundUniqueInit<'a>),
    If {
        position: Position,
        condition: BoundCondition<'a>,
        then_body: Vec<BoundGenerateStmt<'a>>,
        else_body: Option<Vec<BoundGenerateStmt<'a>>>,
    },
    For {
        position: Position,
        range: BoundRange<'a>,
        body: Vec<BoundGenerateStmt<'a>>,
    },
}

impl BoundGenerateStmt<'_> {
    pub(crate) fn position(&self) -> Position {
        match self {
            Self::Scalar { name, .. } | Self::Array { name, .. } | Self::String { name, .. } => {
                name.position
            }
            Self::InitElement(init) => init.target.array.name.position,
            Self::InitUnique(init) => init.target.name.position,
            Self::For { position, .. } | Self::If { position, .. } => *position,
        }
    }
}

pub(crate) struct BoundUniqueInit<'a> {
    pub target: ArrayRef<'a>,
    pub position: Position,
    pub range: BoundRange<'a>,
    pub exclusions: Vec<BoundExclusion<'a>>,
}

pub(crate) enum BoundExclusion<'a> {
    Integer(BoundExpr<'a>),
    Array(ArrayRef<'a>),
}

impl BoundExclusion<'_> {
    pub(crate) fn position(&self) -> Position {
        match self {
            Self::Integer(expr) => expr.position,
            Self::Array(array) => array.name.position,
        }
    }
}

pub(crate) struct BoundElementInit<'a> {
    pub target: BoundElement<'a>,
    pub value: BoundInitValue<'a>,
}

pub(crate) enum BoundInitValue<'a> {
    Range(BoundRange<'a>),
    Alphabet(Vec<u8>),
    Fixed(BoundExpr<'a>),
}

pub(crate) enum BoundInputStmt<'a> {
    Row(BoundRow<'a>),
    If {
        position: Position,
        condition: BoundCondition<'a>,
        then_body: Vec<BoundInputStmt<'a>>,
        else_body: Option<Vec<BoundInputStmt<'a>>>,
    },
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
            Self::RowBlock { position, .. }
            | Self::For { position, .. }
            | Self::If { position, .. } => *position,
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
    WholeString(ArrayRef<'a>),
    WholeStringArray(ArrayRef<'a>),
    StringRow(BoundElement<'a>),
    Char(BoundElement<'a>),
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
pub(crate) enum BoundCondition<'a> {
    Int(Condition<'a, ScalarRef, ArrayRef<'a>>),
    Char(Condition<'a, ScalarRef, ArrayRef<'a>>),
}
pub(crate) type BoundNode<'a> = ExprNode<'a, ScalarRef, ArrayRef<'a>>;

#[derive(Clone, Copy)]
enum Kind {
    Scalar,
    Array,
    String,
    StringArray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type {
    Int,
    Char,
    String,
    StringArray,
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
                GenerateStmt::DeclareString { name, count, .. } => self.declare(
                    *name,
                    if count.is_some() {
                        Kind::StringArray
                    } else {
                        Kind::String
                    },
                )?,
                GenerateStmt::For { variable, body, .. } => {
                    self.register_loop(*variable)?;
                    self.register_generate(body)?;
                }
                GenerateStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.register_generate(then_body)?;
                    if let Some(body) = else_body {
                        self.register_generate(body)?;
                    }
                }
                GenerateStmt::InitElement(_) | GenerateStmt::InitUnique(_) => {}
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
                InputStmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.register_input(then_body)?;
                    if let Some(body) = else_body {
                        self.register_input(body)?;
                    }
                }
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
            GenerateStmt::DeclareString {
                name,
                count,
                length,
            } => {
                let count = count.map(|expr| self.expr(expr)).transpose()?;
                let length = self.expr(length)?;
                self.available += 1;
                BoundGenerateStmt::String {
                    name,
                    count,
                    length,
                }
            }
            GenerateStmt::InitElement(init) => BoundGenerateStmt::InitElement(self.init(init)?),
            GenerateStmt::InitUnique(init) => {
                BoundGenerateStmt::InitUnique(self.unique_init(init)?)
            }
            GenerateStmt::If {
                position,
                condition,
                then_body,
                else_body,
            } => BoundGenerateStmt::If {
                position,
                condition: self.condition(condition)?,
                then_body: self.generate_body(then_body)?,
                else_body: else_body.map(|body| self.generate_body(body)).transpose()?,
            },
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
            InputStmt::If {
                position,
                condition,
                then_body,
                else_body,
            } => BoundInputStmt::If {
                position,
                condition: self.condition(condition)?,
                then_body: self.input_body(then_body)?,
                else_body: else_body.map(|body| self.input_body(body)).transpose()?,
            },
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

    fn element(&self, element: ElementRef<'a>) -> Result<(BoundElement<'a>, Type), Error> {
        let name = element.array;
        if self.loop_index(name).is_some() {
            return Err(Error::at(
                name.position,
                format!("loop variable `{}` cannot be indexed", name.value),
            ));
        }
        let binding = self.global(name)?;
        if matches!(binding.kind, Kind::Scalar) {
            return Err(Error::at(
                name.position,
                format!("`{}` is a scalar; expected an array", name.value),
            ));
        }
        if element.second_index.is_some() && !matches!(binding.kind, Kind::StringArray) {
            return Err(Error::at(
                name.position,
                "only a StringArray accepts two subscripts",
            ));
        }
        let ty = match binding.kind {
            Kind::Array => Type::Int,
            Kind::String => Type::Char,
            Kind::StringArray if element.second_index.is_some() => Type::Char,
            Kind::StringArray => Type::String,
            Kind::Scalar => unreachable!(),
        };
        Ok((
            BoundElement {
                array: ArrayRef {
                    name,
                    slot: binding.slot,
                },
                index: self.expr(element.index)?,
                second_index: element
                    .second_index
                    .map(|expr| self.expr(expr))
                    .transpose()?,
            },
            ty,
        ))
    }

    fn init(&self, init: ElementInit<'a>) -> Result<BoundElementInit<'a>, Error> {
        let position = init.target.array.position;
        let (target, ty) = self.element(init.target)?;
        let value = match init.value {
            InitValue::Range(range) => {
                require_type(ty, Type::Int, position)?;
                BoundInitValue::Range(self.range(range)?)
            }
            InitValue::Alphabet(bytes) => {
                require_type(ty, Type::Char, position)?;
                let mut seen = [false; 128];
                for &byte in &bytes.value {
                    if std::mem::replace(&mut seen[usize::from(byte)], true) {
                        return Err(Error::at(
                            bytes.position,
                            format!(
                                "duplicate character `{}` in candidate alphabet",
                                char::from(byte)
                            ),
                        ));
                    }
                }
                BoundInitValue::Alphabet(bytes.value)
            }
            InitValue::Fixed(expr) => {
                require_type(ty, Type::Char, position)?;
                let position = expr.position;
                let (expr, ty) = self.typed_expr(expr)?;
                require_type(ty, Type::Char, position)?;
                BoundInitValue::Fixed(expr)
            }
        };
        Ok(BoundElementInit { target, value })
    }

    fn unique_init(&self, init: UniqueInit<'a>) -> Result<BoundUniqueInit<'a>, Error> {
        let binding = self.global(init.target)?;
        if !matches!(binding.kind, Kind::Array) {
            return Err(Error::at(
                init.target.position,
                "unique initialization requires a declared integer array",
            ));
        }
        let range = self.range(init.range)?;
        let exclusions = init
            .exclusions
            .into_iter()
            .map(|expr| {
                reject_self_exclusion(&expr.node, init.target)?;
                if let ExprNode::Variable(name) = &expr.node {
                    let binding = self.global(*name)?;
                    if matches!(binding.kind, Kind::Array) {
                        return Ok(BoundExclusion::Array(ArrayRef {
                            name: *name,
                            slot: binding.slot,
                        }));
                    }
                }
                Ok(BoundExclusion::Integer(self.expr(expr)?))
            })
            .collect::<Result<_, Error>>()?;
        Ok(BoundUniqueInit {
            target: ArrayRef {
                name: init.target,
                slot: binding.slot,
            },
            position: init.position,
            range,
            exclusions,
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

    fn condition(&self, condition: Condition<'a>) -> Result<BoundCondition<'a>, Error> {
        let position = condition.left.position;
        let right_position = condition.right.position;
        let (left, ty) = self.typed_expr(condition.left)?;
        let (right, right_ty) = self.typed_expr(condition.right)?;
        require_type(right_ty, ty, right_position)?;
        let bound = Condition {
            left,
            op: condition.op,
            op_position: condition.op_position,
            right,
        };
        match ty {
            Type::Int => Ok(BoundCondition::Int(bound)),
            Type::Char if matches!(condition.op, ComparisonOp::Equal | ComparisonOp::NotEqual) => {
                Ok(BoundCondition::Char(bound))
            }
            Type::Char => Err(Error::at(
                condition.op_position,
                "Char comparisons support only `==` and `!=`",
            )),
            _ => Err(Error::at(
                position,
                "whole strings and arrays cannot be compared",
            )),
        }
    }

    fn expr(&self, expr: Expr<'a>) -> Result<BoundExpr<'a>, Error> {
        let position = expr.position;
        let (expr, ty) = self.typed_expr(expr)?;
        require_type(ty, Type::Int, position)?;
        Ok(expr)
    }

    fn typed_expr(&self, expr: Expr<'a>) -> Result<(BoundExpr<'a>, Type), Error> {
        let (node, ty) = self.node(expr.node, expr.position)?;
        Ok((
            Expr {
                position: expr.position,
                source: expr.source,
                node,
            },
            ty,
        ))
    }

    fn node(&self, node: ExprNode<'a>, position: Position) -> Result<(BoundNode<'a>, Type), Error> {
        Ok(match node {
            ExprNode::Literal(value) => (ExprNode::Literal(value), Type::Int),
            ExprNode::CharLiteral(value) => (ExprNode::CharLiteral(value), Type::Char),
            ExprNode::Variable(name) => {
                let (reference, ty) = if let Some(index) = self.loop_index(name) {
                    (ScalarRef::LoopIndex(index), Type::Int)
                } else {
                    let binding = self.global(name)?;
                    let ty = match binding.kind {
                        Kind::Array => {
                            return Err(Error::at(
                                name.position,
                                format!("`{}` is an array; expected a scalar", name.value),
                            ));
                        }
                        Kind::Scalar => Type::Int,
                        Kind::String => Type::String,
                        Kind::StringArray => Type::StringArray,
                    };
                    (ScalarRef::Global(binding.slot), ty)
                };
                (ExprNode::Variable(reference), ty)
            }
            ExprNode::Element(element) => {
                let (element, ty) = self.element(*element)?;
                (ExprNode::Element(Box::new(element)), ty)
            }
            ExprNode::Unary { op, operand } => {
                let (operand, ty) = self.node(*operand, position)?;
                require_type(ty, Type::Int, op.position)?;
                (
                    ExprNode::Unary {
                        op,
                        operand: Box::new(operand),
                    },
                    Type::Int,
                )
            }
            ExprNode::Binary { first, rest } => {
                let (first, ty) = self.node(*first, position)?;
                require_type(
                    ty,
                    Type::Int,
                    rest.first().map_or(position, |(op, _)| op.position),
                )?;
                let rest = rest
                    .into_iter()
                    .map(|(op, operand)| {
                        let (operand, ty) = self.node(operand, op.position)?;
                        require_type(ty, Type::Int, op.position)?;
                        Ok((op, operand))
                    })
                    .collect::<Result<_, Error>>()?;
                (
                    ExprNode::Binary {
                        first: Box::new(first),
                        rest,
                    },
                    Type::Int,
                )
            }
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
                    Kind::String => BoundOutputRef::WholeString(ArrayRef { name, slot }),
                    Kind::StringArray => BoundOutputRef::WholeStringArray(ArrayRef { name, slot }),
                };
                (name, value)
            }
            OutputRef::Element(element) => {
                let name = element.array;
                let (element, ty) = self.element(element)?;
                let value = match ty {
                    Type::Int => BoundOutputRef::Element(element),
                    Type::Char => BoundOutputRef::Char(element),
                    Type::String => BoundOutputRef::StringRow(element),
                    Type::StringArray => unreachable!(),
                };
                (name, value)
            }
        };
        Ok(Located {
            value,
            position: name.position,
        })
    }
}

// Check every nested reference, including array indices, without making whole
// arrays legal in ordinary integer expressions.
fn reject_self_exclusion(node: &ExprNode<'_>, target: Name<'_>) -> Result<(), Error> {
    let check = |name: Name<'_>| {
        if name.value == target.value {
            Err(Error::at(
                name.position,
                "unique initialization cannot exclude its own target array",
            ))
        } else {
            Ok(())
        }
    };
    match node {
        ExprNode::Variable(name) => check(*name)?,
        ExprNode::Element(element) => {
            check(element.array)?;
            reject_self_exclusion(&element.index.node, target)?;
            if let Some(index) = &element.second_index {
                reject_self_exclusion(&index.node, target)?;
            }
        }
        ExprNode::Unary { operand, .. } => reject_self_exclusion(operand, target)?,
        ExprNode::Binary { first, rest } => {
            reject_self_exclusion(first, target)?;
            for (_, operand) in rest {
                reject_self_exclusion(operand, target)?;
            }
        }
        ExprNode::Literal(_) | ExprNode::CharLiteral(_) => {}
    }
    Ok(())
}

fn require_type(actual: Type, expected: Type, position: Position) -> Result<(), Error> {
    if actual == expected {
        Ok(())
    } else {
        Err(Error::at(
            position,
            format!("type mismatch: expected {expected:?}, got {actual:?}"),
        ))
    }
}
