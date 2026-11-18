mod ast;
mod error;
mod generator;
mod output;
mod parser;
mod validation;

pub use error::Error;

/// Generates one input from a DSL source and an explicit seed.
pub fn generate(source: &str, seed: u64) -> Result<String, Error> {
    let program = parser::parse(source)?;
    let program = validation::validate(program)?;
    let values = generator::generate(&program.generate, program.slot_count, seed)?;
    let output = output::validate(&program.input, &values)?;
    Ok(output.format())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod string_tests;
