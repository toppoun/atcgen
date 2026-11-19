//! Fallible reservations for bounded, potentially large runtime containers.
use crate::Error;
use crate::error::Position;

fn allocation_bytes<T>(count: usize, position: Position, area: &str) -> Result<usize, Error> {
    count
        .checked_mul(size_of::<T>())
        .filter(|&bytes| bytes <= isize::MAX as usize)
        .ok_or_else(|| Error::at(position, format!("{area} allocation size overflow")))
}

pub(crate) fn reserve_vec<T>(
    values: &mut Vec<T>,
    additional: usize,
    position: Position,
    area: &str,
) -> Result<(), Error> {
    let count = values
        .len()
        .checked_add(additional)
        .ok_or_else(|| Error::at(position, format!("{area} allocation size overflow")))?;
    let bytes = allocation_bytes::<T>(count, position, area)?;
    if count > values.capacity() {
        #[cfg(test)]
        crate::faults::allocation(area, position)?;
        values.try_reserve_exact(additional).map_err(|error| {
            Error::at(
                position,
                format!("unable to allocate {area} ({count} elements, {bytes} bytes): {error}"),
            )
        })?;
    }
    Ok(())
}

pub(crate) fn output_string(bytes: usize, position: Position) -> Result<String, Error> {
    allocation_bytes::<u8>(bytes, position, "output buffer")?;
    #[cfg(test)]
    crate::faults::allocation("output buffer", position)?;
    let mut output = String::new();
    output.try_reserve_exact(bytes).map_err(|error| {
        Error::at(
            position,
            format!("unable to allocate output buffer ({bytes} bytes): {error}"),
        )
    })?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_multiplication_address_space_and_length_before_reserving() {
        let position = Position { line: 2, column: 3 };
        for count in [usize::MAX, isize::MAX as usize] {
            assert!(allocation_bytes::<i64>(count, position, "array").is_err());
        }
        assert!(output_string(usize::MAX, position).is_err());
        let mut values = vec![1_u8];
        let error = reserve_vec(&mut values, usize::MAX, position, "values").unwrap_err();
        assert!(error.to_string().contains("allocation size overflow"));
        assert_eq!(values, [1]);
    }
}
