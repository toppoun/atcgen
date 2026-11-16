mod ast;
mod error;
mod generator;
mod parser;
mod validation;

pub use error::Error;

/// Generates one input from a DSL source and an explicit seed.
pub fn generate(source: &str, seed: u64) -> Result<String, Error> {
    let input = parser::parse(source)?;
    let input = validation::validate(input)?;
    generator::generate(&input, seed)
}

#[cfg(test)]
mod tests;
