//! Utility functions for the API Gateway

use anyhow::{Result, anyhow};

/// Parse a string value to fixed-point representation
/// 
/// Converts a decimal string to a fixed-point integer representation
/// with 4 decimal places (10000 = 1.0)
pub fn parse_fixed_point(value: &str) -> Result<i64> {
    let parsed = value.parse::<f64>()
        .map_err(|e| anyhow!("Failed to parse '{}' as number: {}", value, e))?;
    
    // SAFETY: Conversion from f64 to i64 for fixed-point representation
    #[allow(clippy::cast_possible_truncation)]
    Ok((parsed * 10000.0) as i64)
}

/// Convert fixed-point value back to string representation
#[allow(clippy::cast_precision_loss)]
pub fn fixed_point_to_string(value: i64) -> String {
    format!("{:.4}", value as f64 / 10000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fixed_point() {
        assert_eq!(parse_fixed_point("1.0").unwrap(), 10000);
        assert_eq!(parse_fixed_point("0.5").unwrap(), 5000);
        assert_eq!(parse_fixed_point("123.456").unwrap(), 1234560);
        assert!(parse_fixed_point("invalid").is_err());
    }

    #[test]
    fn test_fixed_point_to_string() {
        assert_eq!(fixed_point_to_string(10000), "1.0000");
        assert_eq!(fixed_point_to_string(5000), "0.5000");
        assert_eq!(fixed_point_to_string(1234560), "123.4560");
    }
}