use crate::AppError;

pub const USD_NANOS: i64 = 1_000_000_000;

pub fn format_usd_nanos(nanos: i64) -> String {
    let whole = nanos / USD_NANOS;
    let fraction = nanos.rem_euclid(USD_NANOS);
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:09}")
            .trim_end_matches('0')
            .to_owned()
    }
}

pub fn parse_usd_nanos(value: &str) -> Result<i64, AppError> {
    parse_positive_decimal_nanos(value, "amount_usd", USD_NANOS, 10_000 * USD_NANOS)
}

pub fn parse_model_price_multiplier_nanos(value: &str) -> Result<i64, AppError> {
    parse_positive_decimal_nanos(value, "model_price_multiplier", 1, 1_000 * USD_NANOS)
}

fn parse_positive_decimal_nanos(
    value: &str,
    field: &str,
    minimum: i64,
    maximum: i64,
) -> Result<i64, AppError> {
    let value = value.trim();
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, ""), |(whole, fraction)| (whole, fraction));
    if whole.is_empty()
        || whole.len() > 6
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 9
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(AppError::bad_request(format!(
            "{field} must be a positive amount with at most 9 decimal places"
        )));
    }
    let whole = whole
        .parse::<i64>()
        .map_err(|_| AppError::bad_request(format!("{field} is outside the allowed range")))?;
    let fraction = format!("{fraction:0<9}")
        .parse::<i64>()
        .map_err(|_| AppError::bad_request(format!("{field} is outside the allowed range")))?;
    let nanos = whole
        .checked_mul(USD_NANOS)
        .and_then(|whole| whole.checked_add(fraction))
        .ok_or_else(|| AppError::bad_request(format!("{field} is outside the allowed range")))?;
    if !(minimum..=maximum).contains(&nanos) {
        return Err(AppError::bad_request(format!(
            "{field} is outside the allowed range"
        )));
    }
    Ok(nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_usd_amounts() {
        assert_eq!(parse_usd_nanos("1.000000001").unwrap(), 1_000_000_001);
        assert_eq!(format_usd_nanos(12_340_000_000), "12.34");
        assert!(parse_usd_nanos("0.99").is_err());
        assert!(parse_usd_nanos("10000.000000001").is_err());
        assert!(parse_usd_nanos("1.0000000001").is_err());
    }
}
