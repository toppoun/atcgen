use std::collections::HashMap;
use std::ops::Range;

use crate::Error;
use crate::ast::{Expr, ExprNode, Input, Name, RangeExpr, Row, Statement};
use crate::error::Position;

pub(crate) struct ValidatedInput<'a> {
    pub statements: Vec<ValidatedStatement<'a>>,
    pub slot_count: usize,
}

pub(crate) enum ValidatedStatement<'a> {
    Row(ValidatedRow<'a>),
    For(ValidatedFor<'a>),
}

pub(crate) struct ValidatedRow<'a> {
    pub position: Position,
    pub variables: Vec<Variable<'a>>,
}

pub(crate) struct ValidatedFor<'a> {
    // The index is the first slot, followed by this loop's declarations.
    pub slots: Range<usize>,
    pub range: BoundRange<'a>,
    pub rows: Vec<ValidatedRow<'a>>,
}

pub(crate) struct Variable<'a> {
    pub name: Name<'a>,
    pub slot: usize,
    pub length: Option<BoundExpr<'a>>,
    pub range: BoundRange<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct Reference<'a> {
    pub name: Name<'a>,
    pub slot: usize,
}

pub(crate) type BoundExpr<'a> = Expr<'a, Reference<'a>>;
pub(crate) type BoundRange<'a> = RangeExpr<'a, Reference<'a>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NameKind {
    Outer,
    Local,
    LoopIndex,
}

struct Binding {
    slot: usize,
    array: bool,
}

type Scope<'a> = HashMap<&'a str, Binding>;

#[derive(Default)]
struct Registry<'a> {
    names: HashMap<&'a str, NameKind>,
    slot_count: usize,
}

impl<'a> Registry<'a> {
    fn declare(
        &mut self,
        scope: &mut Scope<'a>,
        name: Name<'a>,
        kind: NameKind,
        array: bool,
    ) -> Result<(), Error> {
        if self
            .names
            .get(name.value)
            .is_some_and(|&previous| previous != kind)
        {
            return Err(Error::at(
                name.position,
                format!("name collision for `{}`", name.value),
            ));
        }
        if scope.contains_key(name.value) {
            return Err(Error::at(
                name.position,
                format!("duplicate declaration of `{}`", name.value),
            ));
        }
        self.names.insert(name.value, kind);
        scope.insert(
            name.value,
            Binding {
                slot: self.slot_count,
                array,
            },
        );
        self.slot_count += 1;
        Ok(())
    }
}

pub(crate) fn validate(input: Input<'_>) -> Result<ValidatedInput<'_>, Error> {
    let mut registry = Registry::default();
    let mut outer_scope = Scope::new();
    let mut loop_scopes = Vec::new();
    for statement in &input.statements {
        match statement {
            Statement::Row(row) => {
                for declaration in &row.declarations {
                    registry.declare(
                        &mut outer_scope,
                        declaration.name,
                        NameKind::Outer,
                        declaration.length.is_some(),
                    )?;
                }
            }
            Statement::For(loop_) => {
                let mut scope = Scope::new();
                registry.declare(&mut scope, loop_.index, NameKind::LoopIndex, false)?;
                for declaration in loop_.rows.iter().flat_map(|row| &row.declarations) {
                    registry.declare(
                        &mut scope,
                        declaration.name,
                        NameKind::Local,
                        declaration.length.is_some(),
                    )?;
                }
                loop_scopes.push(scope);
            }
        }
    }

    let mut constraints = HashMap::new();
    for constraint in input.constraints {
        let target = constraint.target;
        match registry.names.get(target.value) {
            None => {
                return Err(Error::at(
                    target.position,
                    format!("undefined variable `{}`", target.value),
                ));
            }
            Some(NameKind::LoopIndex) => {
                return Err(Error::at(
                    target.position,
                    format!("loop variable `{}` cannot have a constraint", target.value),
                ));
            }
            Some(NameKind::Outer | NameKind::Local) => {}
        }
        if constraints.insert(target.value, constraint.range).is_some() {
            return Err(Error::at(
                target.position,
                format!("duplicate constraint for `{}`", target.value),
            ));
        }
    }

    let outer = Resolver {
        outer: &outer_scope,
        local: None,
        names: &registry.names,
    };
    let mut loop_scopes = loop_scopes.iter();
    let mut statements = Vec::new();
    for statement in input.statements {
        let statement = match statement {
            Statement::Row(row) => ValidatedStatement::Row(outer.row(row, &constraints)?),
            Statement::For(loop_) => {
                let local = loop_scopes
                    .next()
                    .expect("scope was registered for every loop");
                let index = local[loop_.index.value].slot;
                let range = outer.range(&loop_.range, index)?;
                let resolver = Resolver {
                    local: Some(local),
                    ..outer
                };
                let rows = loop_
                    .rows
                    .into_iter()
                    .map(|row| resolver.row(row, &constraints))
                    .collect::<Result<_, _>>()?;
                ValidatedStatement::For(ValidatedFor {
                    slots: index..index + local.len(),
                    range,
                    rows,
                })
            }
        };
        statements.push(statement);
    }
    Ok(ValidatedInput {
        statements,
        slot_count: registry.slot_count,
    })
}

struct Resolver<'scope, 'a> {
    outer: &'scope Scope<'a>,
    local: Option<&'scope Scope<'a>>,
    names: &'scope HashMap<&'a str, NameKind>,
}

impl<'a> Resolver<'_, 'a> {
    fn binding(&self, name: &str) -> Option<&Binding> {
        self.local
            .and_then(|scope| scope.get(name))
            .or_else(|| self.outer.get(name))
    }

    fn row(
        &self,
        row: Row<'a>,
        constraints: &HashMap<&str, RangeExpr<'a>>,
    ) -> Result<ValidatedRow<'a>, Error> {
        let mut variables = Vec::new();
        for declaration in row.declarations {
            let name = declaration.name;
            let Some(range) = constraints.get(name.value) else {
                return Err(Error::at(
                    name.position,
                    format!("missing constraint for `{}`", name.value),
                ));
            };
            let slot = self
                .binding(name.value)
                .expect("declaration was registered")
                .slot;
            let length = declaration
                .length
                .as_ref()
                .map(|expr| self.expression(expr, slot))
                .transpose()?;
            let range = self.range(range, slot)?;
            variables.push(Variable {
                name,
                slot,
                length,
                range,
            });
        }
        Ok(ValidatedRow {
            position: row.position,
            variables,
        })
    }

    fn range(&self, range: &RangeExpr<'a>, current: usize) -> Result<BoundRange<'a>, Error> {
        Ok(RangeExpr {
            lower: self.expression(&range.lower, current)?,
            upper: self.expression(&range.upper, current)?,
            end: range.end,
            source: range.source,
        })
    }

    fn expression(&self, expr: &Expr<'a>, current: usize) -> Result<BoundExpr<'a>, Error> {
        Ok(Expr {
            node: self.node(&expr.node, current)?,
            position: expr.position,
            source: expr.source,
        })
    }

    fn node(
        &self,
        node: &ExprNode<Name<'a>>,
        current: usize,
    ) -> Result<ExprNode<Reference<'a>>, Error> {
        Ok(match node {
            ExprNode::Literal(value) => ExprNode::Literal(*value),
            ExprNode::Variable(name) => ExprNode::Variable(self.reference(*name, current)?),
            ExprNode::Unary { op, operand } => ExprNode::Unary {
                op: *op,
                operand: Box::new(self.node(operand, current)?),
            },
            ExprNode::Binary { first, rest } => ExprNode::Binary {
                first: Box::new(self.node(first, current)?),
                rest: rest
                    .iter()
                    .map(|(op, operand)| Ok((*op, self.node(operand, current)?)))
                    .collect::<Result<_, Error>>()?,
            },
        })
    }

    fn reference(&self, name: Name<'a>, current: usize) -> Result<Reference<'a>, Error> {
        let variable = name.value;
        let message = match self.binding(variable) {
            None => match self.names.get(variable) {
                Some(NameKind::Local) => format!(
                    "`{variable}` is outside this scope; a loop collection cannot be used as a scalar here"
                ),
                Some(NameKind::LoopIndex) => {
                    format!("loop variable `{variable}` is outside this scope")
                }
                _ => format!("undefined variable `{variable}`"),
            },
            Some(binding) if binding.array => {
                format!("`{variable}` is an array; expected a scalar")
            }
            Some(binding) if binding.slot >= current => {
                format!("`{variable}` is not available before this declaration")
            }
            Some(binding) => {
                return Ok(Reference {
                    name,
                    slot: binding.slot,
                });
            }
        };
        Err(Error::at(name.position, message))
    }
}
