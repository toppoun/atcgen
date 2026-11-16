use crate::error::Position;

pub(crate) struct Input<'a> {
    pub rows: Vec<Vec<Declaration<'a>>>,
    pub constraints: Vec<Constraint<'a>>,
}

pub(crate) struct Declaration<'a> {
    pub name: Name<'a>,
    pub length: Option<Expr<'a>>,
}

pub(crate) struct Constraint<'a> {
    pub target: Name<'a>,
    pub array_elements: bool,
    pub range: RangeExpr<'a>,
}

pub(crate) struct RangeExpr<'a> {
    pub lower: Expr<'a>,
    pub upper: Expr<'a>,
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

pub(crate) struct Expr<'a> {
    pub position: Position,
    pub source: &'a str,
    pub node: ExprNode<'a>,
}

pub(crate) enum ExprNode<'a> {
    Literal(i64),
    Variable(Name<'a>),
    Unary {
        op: Located<UnaryOp>,
        operand: Box<ExprNode<'a>>,
    },
    Binary {
        first: Box<ExprNode<'a>>,
        // Operators of the same precedence are evaluated left to right.
        rest: Vec<(Located<BinaryOp>, ExprNode<'a>)>,
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
