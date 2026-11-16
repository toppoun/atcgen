use crate::error::Position;

pub(crate) struct Input<'a> {
    pub rows: Vec<Vec<Declaration<'a>>>,
    pub constraints: Vec<Constraint<'a>>,
}

pub(crate) struct Declaration<'a> {
    pub name: Name<'a>,
    pub length: Option<Endpoint<'a>>,
}

pub(crate) struct Constraint<'a> {
    pub target: Name<'a>,
    pub array_elements: bool,
    pub lower: Endpoint<'a>,
    pub upper: Endpoint<'a>,
}

#[derive(Clone, Copy)]
pub(crate) struct Located<T> {
    pub value: T,
    pub position: Position,
}

pub(crate) type Name<'a> = Located<&'a str>;
pub(crate) type Endpoint<'a> = Located<Value<'a>>;

#[derive(Clone, Copy)]
pub(crate) enum Value<'a> {
    Integer(i64),
    Scalar(&'a str),
}
