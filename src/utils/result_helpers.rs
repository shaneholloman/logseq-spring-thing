//! Safe error handling utilities to eliminate unsafe .unwrap() calls

use tracing::warn;

/// Safely converts f64 to serde_json::Number, replacing NaN/Infinity with 0.0
/// This is a common pattern when building JSON responses with numeric data.
/// # Examples
/// ```rust,ignore
/// use visionflow::utils::result_helpers::safe_json_number;
/// let num = safe_json_number(42.5);
/// assert!(num.is_some());
/// let nan_num = safe_json_number(f64::NAN);
/// assert_eq!(nan_num, serde_json::Number::from_f64(0.0));
/// ```
pub fn safe_json_number(value: f64) -> serde_json::Number {
    use serde_json::Number;

    // SAFETY: 0.0 is always a valid finite f64, so from_f64 never returns None for it
    let zero = Number::from_f64(0.0).expect("0.0 is always a valid JSON number");

    if value.is_finite() {
        Number::from_f64(value).unwrap_or(zero)
    } else {
        // NaN or Infinity - replace with 0.0
        warn!("safe_json_number: Replacing non-finite value ({}) with 0.0", value);
        zero
    }
}
