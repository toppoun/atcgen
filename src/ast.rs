use crate::error::Position;

pub(crate) struct Program<'a> {
    pub generate: Vec<GenerateStmt<'a>>,
    pub input: Vec<InputStmt<'a>>,
}

pub(crate) enum GenerateStmt<'a> {
    GenerateScalar {
        name: Name<'a>,
        range: RangeExpr<'a>,
    },
    DeclareArray {
        name: Name<'a>,
        length: Expr<'a>,
    },
    DeclareString {
        name: Name<'a>,
        count: Option<Expr<'a>>,
        length: Expr<'a>,
    },
    InitElement(ElementInit<'a>),
    If {
        position: Position,
        condition: Condition<'a>,
        then_body: Vec<GenerateStmt<'a>>,
        else_body: Option<Vec<GenerateStmt<'a>>>,
    },
    For {
        position: Position,
        variable: Name<'a>,
        range: RangeExpr<'a>,
        body: Vec<GenerateStmt<'a>>,
    },
}

pub(crate) struct ElementInit<'a> {
    pub target: ElementRef<'a>,
    pub value: InitValue<'a>,
}

pub(crate) enum InitValue<'a> {
    Range(RangeExpr<'a>),
    Alphabet(Located<Vec<u8>>),
    Fixed(Expr<'a>),
}

pub(crate) enum InputStmt<'a> {
    Row(OutputRow<'a>),
    If {
        position: Position,
        condition: Condition<'a>,
        then_body: Vec<InputStmt<'a>>,
        else_body: Option<Vec<InputStmt<'a>>>,
    },
    RowBlock {
        position: Position,
        body: Vec<InputStmt<'a>>,
    },
    For {
        position: Position,
        variable: Name<'a>,
        range: RangeExpr<'a>,
        body: Vec<InputStmt<'a>>,
    },
}

pub(crate) struct OutputRow<'a> {
    pub position: Position,
    pub items: Vec<OutputRef<'a>>,
}

pub(crate) enum OutputRef<'a> {
    Name(Name<'a>),
    Element(ElementRef<'a>),
}

pub(crate) struct ElementRef<'a, Scalar = Name<'a>, Array = Name<'a>> {
    pub array: Array,
    pub index: Expr<'a, Scalar, Array>,
    pub second_index: Option<Expr<'a, Scalar, Array>>,
}

pub(crate) struct RangeExpr<'a, Scalar = Name<'a>, Array = Name<'a>> {
    pub lower: Expr<'a, Scalar, Array>,
    pub upper: Expr<'a, Scalar, Array>,
    pub end: RangeEnd,
    pub source: &'a str,
}

pub(crate) struct Condition<'a, Scalar = Name<'a>, Array = Name<'a>> {
    pub left: Expr<'a, Scalar, Array>,
    pub op: ComparisonOp,
    pub op_position: Position,
    pub right: Expr<'a, Scalar, Array>,
}

#[derive(Clone, Copy)]
pub(crate) enum ComparisonOp {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
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

pub(crate) struct Expr<'a, Scalar = Name<'a>, Array = Name<'a>> {
    pub position: Position,
    pub source: &'a str,
    pub node: ExprNode<'a, Scalar, Array>,
}

pub(crate) enum ExprNode<'a, Scalar = Name<'a>, Array = Name<'a>> {
    Literal(i64),
    CharLiteral(u8),
    Variable(Scalar),
    Element(Box<ElementRef<'a, Scalar, Array>>),
    Unary {
        op: Located<UnaryOp>,
        operand: Box<Self>,
    },
    Binary {
        first: Box<Self>,
        // Operators of the same precedence are evaluated left to right.
        rest: Vec<(Located<BinaryOp>, Self)>,
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
