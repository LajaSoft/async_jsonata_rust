/// Numeric helper functions translated from the JSONata JavaScript implementation.
pub fn sum(args: Option<&[f64]>) -> Option<f64> {
    let slice = args?;
    let total: f64 = slice.iter().copied().sum();
    Some(total)
}

pub fn count<T>(args: Option<&[T]>) -> usize {
    match args {
        Some(slice) => slice.len(),
        None => 0,
    }
}

pub fn max(args: Option<&[f64]>) -> Option<f64> {
    let slice = args?;
    if slice.is_empty() {
        return None;
    }
    slice.iter().copied().reduce(f64::max)
}

pub fn min(args: Option<&[f64]>) -> Option<f64> {
    let slice = args?;
    if slice.is_empty() {
        return None;
    }
    slice.iter().copied().reduce(f64::min)
}

pub fn average(args: Option<&[f64]>) -> Option<f64> {
    let slice = args?;
    if slice.is_empty() {
        return None;
    }
    let total: f64 = slice.iter().copied().sum();
    Some(total / slice.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_handles_none() {
        assert_eq!(sum(None), None);
    }

    #[test]
    fn sum_handles_values() {
        let data = [1.0, 2.0, 3.0];
        assert_eq!(sum(Some(&data)), Some(6.0));
    }

    #[test]
    fn count_defaults_to_zero() {
        assert_eq!(count::<u8>(None), 0);
    }

    #[test]
    fn count_counts_items() {
        let data = [1, 2, 3, 4];
        assert_eq!(count(Some(&data)), 4);
    }

    #[test]
    fn max_respects_empty() {
        assert_eq!(max(Some(&[])), None);
    }

    #[test]
    fn max_finds_value() {
        let data = [1.0, 3.5, 2.0];
        assert_eq!(max(Some(&data)), Some(3.5));
    }

    #[test]
    fn min_finds_value() {
        let data = [1.0, 3.5, 2.0];
        assert_eq!(min(Some(&data)), Some(1.0));
    }

    #[test]
    fn average_requires_items() {
        let data = [2.0, 4.0];
        assert_eq!(average(Some(&data)), Some(3.0));
        assert_eq!(average(Some(&[])), None);
    }
}
