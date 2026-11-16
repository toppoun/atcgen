use std::collections::HashMap;

use crate::Error;
use crate::ast::{Constraint, Declaration, Endpoint, Input, Value};

pub(crate) struct ValidatedInput<'a> {
    pub rows: Vec<Vec<Variable<'a>>>,
}

pub(crate) struct Variable<'a> {
    pub declaration: Declaration<'a>,
    pub constraint: Constraint<'a>,
}

struct DeclarationInfo {
    index: usize,
    array: bool,
}

pub(crate) fn validate(input: Input<'_>) -> Result<ValidatedInput<'_>, Error> {
    let mut declarations = HashMap::new();
    for (index, declaration) in input.rows.iter().flatten().enumerate() {
        let name = declaration.name;
        let info = DeclarationInfo {
            index,
            array: declaration.length.is_some(),
        };
        if declarations.insert(name.value, info).is_some() {
            return Err(Error::at(
                name.position,
                format!("duplicate declaration of `{}`", name.value),
            ));
        }
    }

    let mut constraints = HashMap::new();
    for constraint in input.constraints {
        let target = constraint.target;
        let Some(info) = declarations.get(target.value) else {
            return Err(Error::at(
                target.position,
                format!("undefined variable `{}`", target.value),
            ));
        };
        if info.array != constraint.array_elements {
            let message = if info.array {
                format!("use `{}[*]` to constrain array elements", target.value)
            } else {
                format!("`{}` is a scalar and cannot be indexed", target.value)
            };
            return Err(Error::at(target.position, message));
        }
        if constraints.insert(target.value, constraint).is_some() {
            return Err(Error::at(
                target.position,
                format!("duplicate constraint for `{}`", target.value),
            ));
        }
    }

    let mut rows = Vec::new();
    for row in input.rows {
        let mut variables = Vec::new();
        for declaration in row {
            let name = declaration.name;
            let Some(constraint) = constraints.remove(name.value) else {
                return Err(Error::at(
                    name.position,
                    format!("missing constraint for `{}`", name.value),
                ));
            };
            let index = declarations[name.value].index;
            if let Some(length) = declaration.length {
                validate_reference(length, index, &declarations)?;
            }
            validate_reference(constraint.lower, index, &declarations)?;
            validate_reference(constraint.upper, index, &declarations)?;
            variables.push(Variable {
                declaration,
                constraint,
            });
        }
        rows.push(variables);
    }
    Ok(ValidatedInput { rows })
}

fn validate_reference(
    endpoint: Endpoint<'_>,
    current: usize,
    declarations: &HashMap<&str, DeclarationInfo>,
) -> Result<(), Error> {
    let Value::Scalar(name) = endpoint.value else {
        return Ok(());
    };
    let message = match declarations.get(name) {
        None => format!("undefined variable `{name}`"),
        Some(info) if info.array => format!("`{name}` is an array; expected a scalar"),
        Some(info) if info.index >= current => {
            format!("`{name}` is not available before this declaration")
        }
        Some(_) => return Ok(()),
    };
    Err(Error::at(endpoint.position, message))
}
