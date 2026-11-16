use crate::error::Position;

pub(crate) struct Input<'a> {
    pub statements: Vec<Statement<'a>>,
    pub constraints: Vec<Constraint<'a>>,
}

pub(crate) enum Statement<'a> {
    Row(Row<'a>),
    For(For<'a>),
}

pub(crate) struct Row<'a> {
    pub position: Position,
    pub declarations: Vec<Declaration<'a>>,
}

pub(crate) struct For<'a> {
    pub index: Name<'a>,
    pub range: RangeExpr<'a>,
    pub rows: Vec<Row<'a>>,
}

pub(crate) struct Declaration<'a> {
    pub name: Name<'a>,
    pub length: Option<Expr<'a>>,
}

pub(crate) struct Constraint<'a> {
    pub target: Name<'a>,
    pub range: RangeExpr<'a>,
}

pub(crate) struct RangeExpr<'a, Ref = Name<'a>> {
    pub lower: Expr<'a, Ref>,
    pub upper: Expr<'a, Ref>,
    pub end: RangeEnd,
    pub source: &'a str,
}

#[derive(Clone, Copy)]
pub(crate) enum RangeEnd {
    Exclusive,
    Inclusive,
}

#[derive(Clone, Copy)]
pub(crate) struct Located<T> {
    pub value: T,
    pub position: Position,
}

pub(crate) type Name<'a> = Located<&'a str>;

pub(crate) struct Expr<'a, Ref = Name<'a>> {
    pub position: Position,
    pub source: &'a str,
    pub node: ExprNode<Ref>,
}

pub(crate) enum ExprNode<Ref> {
    Literal(i64),
    Variable(Ref),
    Unary {
        op: Located<UnaryOp>,
        operand: Box<ExprNode<Ref>>,
    },
    Binary {
        first: Box<ExprNode<Ref>>,
        // Operators of the same precedence are evaluated left to right.
        rest: Vec<(Located<BinaryOp>, ExprNode<Ref>)>,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum UnaryOp {
    Plus,
    Minus,
}

#[derive(Clone, Copy)]
pub(crate) enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}
