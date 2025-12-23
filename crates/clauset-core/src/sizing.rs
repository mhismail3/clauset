//! Terminal dimension validation and adjustment.
//!
//! Validates terminal dimensions from clients to prevent:
//! - Implausible dimensions (too small/large)
//! - Suspicious aspect ratios
//! - Device-specific sizing issues

use serde::{Deserialize, Serialize};
use tracing::warn;

/// Minimum acceptable column count
const MIN_COLS: u16 = 20;
/// Maximum acceptable column count
const MAX_COLS: u16 = 300;
/// Minimum acceptable row count
const MIN_ROWS: u16 = 5;
/// Maximum acceptable row count
const MAX_ROWS: u16 = 100;

/// Minimum acceptable aspect ratio (cols/rows)
const MIN_ASPECT_RATIO: f64 = 0.3;
/// Maximum acceptable aspect ratio (cols/rows)
const MAX_ASPECT_RATIO: f64 = 8.0;

/// Device hint from client for dimension validation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceHint {
    Iphone,
    Ipad,
    Desktop,
    Unknown,
}

impl Default for DeviceHint {
    fn default() -> Self {
        DeviceHint::Unknown
    }
}

/// Dimension source from client (indicates confidence level)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DimensionSource {
    Fitaddon,
    Container,
    Estimation,
    Defaults,
}

impl Default for DimensionSource {
    fn default() -> Self {
        DimensionSource::Estimation
    }
}

/// Confidence level for dimensions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

impl Default for ConfidenceLevel {
    fn default() -> Self {
        ConfidenceLevel::Low
    }
}

/// Validated dimensions result
#[derive(Debug, Clone)]
pub struct ValidatedDimensions {
    /// Final columns (may be adjusted from original)
    pub cols: u16,
    /// Final rows (may be adjusted from original)
    pub rows: u16,
    /// Whether dimensions were adjusted
    pub adjusted: bool,
    /// Reason for adjustment (if any)
    pub adjustment_reason: Option<String>,
}

/// Validation error for completely rejected dimensions
#[derive(Debug, Clone)]
pub struct DimensionError {
    /// Error message
    pub reason: String,
    /// Suggested columns
    pub suggested_cols: u16,
    /// Suggested rows
    pub suggested_rows: u16,
}

/// Validate and potentially adjust terminal dimensions.
///
/// # Arguments
/// * `cols` - Requested column count
/// * `rows` - Requested row count
/// * `device_hint` - Optional device type hint from client
/// * `confidence` - Client's confidence level in the dimensions
/// * `source` - How the client calculated the dimensions
///
/// # Returns
/// `Ok(ValidatedDimensions)` with final dimensions, or `Err(DimensionError)` if completely invalid
pub fn validate_dimensions(
    cols: u16,
    rows: u16,
    device_hint: Option<DeviceHint>,
    confidence: Option<ConfidenceLevel>,
    _source: Option<DimensionSource>,
) -> Result<ValidatedDimensions, DimensionError> {
    let device = device_hint.unwrap_or_default();
    let conf = confidence.unwrap_or_default();

    let mut adjusted_cols = cols;
    let mut adjusted_rows = rows;
    let mut adjustment_reasons: Vec<String> = Vec::new();

    // Check for zero dimensions (completely invalid)
    if cols == 0 || rows == 0 {
        let defaults = get_device_defaults(&device);
        return Err(DimensionError {
            reason: "Zero dimensions are invalid".to_string(),
            suggested_cols: defaults.0,
            suggested_rows: defaults.1,
        });
    }

    // Validate column bounds
    if cols < MIN_COLS {
        adjusted_cols = MIN_COLS;
        adjustment_reasons.push(format!("cols {} < min {}", cols, MIN_COLS));
    } else if cols > MAX_COLS {
        adjusted_cols = MAX_COLS;
        adjustment_reasons.push(format!("cols {} > max {}", cols, MAX_COLS));
    }

    // Validate row bounds
    if rows < MIN_ROWS {
        adjusted_rows = MIN_ROWS;
        adjustment_reasons.push(format!("rows {} < min {}", rows, MIN_ROWS));
    } else if rows > MAX_ROWS {
        adjusted_rows = MAX_ROWS;
        adjustment_reasons.push(format!("rows {} > max {}", rows, MAX_ROWS));
    }

    // Check aspect ratio
    let aspect_ratio = adjusted_cols as f64 / adjusted_rows as f64;
    if aspect_ratio < MIN_ASPECT_RATIO {
        // Too narrow - widen by increasing cols
        adjusted_cols = (adjusted_rows as f64 * MIN_ASPECT_RATIO).ceil() as u16;
        adjustment_reasons.push(format!("aspect ratio {} too narrow", aspect_ratio));
    } else if aspect_ratio > MAX_ASPECT_RATIO {
        // Too wide - narrow by increasing rows
        adjusted_rows = (adjusted_cols as f64 / MAX_ASPECT_RATIO).ceil() as u16;
        adjustment_reasons.push(format!("aspect ratio {} too wide", aspect_ratio));
    }

    // Device-specific validation and warnings
    match device {
        DeviceHint::Iphone => {
            // iPhones should typically have < 60 cols in portrait
            if cols > 60 {
                // Only warn if confidence is low - high confidence FitAddon knows better
                if conf == ConfidenceLevel::Low {
                    warn!(
                        target: "clauset::sizing",
                        "iPhone requesting {} cols with low confidence may indicate sizing issue",
                        cols
                    );
                    // Adjust to safe default for low confidence iPhone
                    adjusted_cols = 40;
                    adjusted_rows = 20;
                    adjustment_reasons.push("iPhone low-confidence adjustment".to_string());
                } else {
                    warn!(
                        target: "clauset::sizing",
                        "iPhone requesting {} cols - accepted with {} confidence",
                        cols,
                        match conf {
                            ConfidenceLevel::High => "high",
                            ConfidenceLevel::Medium => "medium",
                            ConfidenceLevel::Low => "low",
                        }
                    );
                }
            }
        }
        DeviceHint::Ipad => {
            // iPads can have larger dimensions, but warn if extreme
            if cols > 150 {
                warn!(
                    target: "clauset::sizing",
                    "iPad requesting {} cols is unusually large",
                    cols
                );
            }
        }
        DeviceHint::Desktop | DeviceHint::Unknown => {
            // Desktop/unknown - less restrictive
        }
    }

    let adjusted = !adjustment_reasons.is_empty();
    let adjustment_reason = if adjusted {
        Some(adjustment_reasons.join("; "))
    } else {
        None
    };

    Ok(ValidatedDimensions {
        cols: adjusted_cols,
        rows: adjusted_rows,
        adjusted,
        adjustment_reason,
    })
}

/// Get safe default dimensions for a device type.
fn get_device_defaults(device: &DeviceHint) -> (u16, u16) {
    match device {
        DeviceHint::Iphone => (40, 20),
        DeviceHint::Ipad => (80, 30),
        DeviceHint::Desktop | DeviceHint::Unknown => (80, 24),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_dimensions() {
        let result = validate_dimensions(80, 24, None, None, None);
        assert!(result.is_ok());
        let dims = result.unwrap();
        assert_eq!(dims.cols, 80);
        assert_eq!(dims.rows, 24);
        assert!(!dims.adjusted);
    }

    #[test]
    fn test_zero_dimensions() {
        let result = validate_dimensions(0, 24, None, None, None);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.suggested_cols, 80);
        assert_eq!(error.suggested_rows, 24);
    }

    #[test]
    fn test_min_cols() {
        let result = validate_dimensions(10, 24, None, None, None).unwrap();
        assert_eq!(result.cols, MIN_COLS);
        assert!(result.adjusted);
    }

    #[test]
    fn test_max_cols() {
        let result = validate_dimensions(500, 24, None, None, None).unwrap();
        assert_eq!(result.cols, MAX_COLS);
        assert!(result.adjusted);
    }

    #[test]
    fn test_iphone_low_confidence() {
        let result = validate_dimensions(
            80,
            24,
            Some(DeviceHint::Iphone),
            Some(ConfidenceLevel::Low),
            None,
        )
        .unwrap();
        // Should adjust to iPhone safe defaults
        assert_eq!(result.cols, 40);
        assert_eq!(result.rows, 20);
        assert!(result.adjusted);
    }

    #[test]
    fn test_iphone_high_confidence() {
        let result = validate_dimensions(
            80,
            24,
            Some(DeviceHint::Iphone),
            Some(ConfidenceLevel::High),
            None,
        )
        .unwrap();
        // High confidence - trust the client
        assert_eq!(result.cols, 80);
        assert_eq!(result.rows, 24);
        assert!(!result.adjusted);
    }
}
