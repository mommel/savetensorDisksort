//! Base-1024 human-readable byte formatting and parsing utilities.

const KB: u64 = 1024;
const MB: u64 = 1024 * KB;
const GB: u64 = 1024 * MB;
const TB: u64 = 1024 * GB;

/// Format a byte count into a human-readable string according to DiskSort specification:
/// - < 1024: "X B"
/// - < 1 MB: "X.X KB" (1 decimal)
/// - < 1 GB: "X.X MB" (1 decimal)
/// - < 1 TB: "X.XX GB" (2 decimals)
/// - >= 1 TB: "X.XX TB" (2 decimals)
pub fn format_bytes(bytes: u64) -> String {
    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        let val = bytes as f64 / KB as f64;
        format!("{:.1} KB", val)
    } else if bytes < GB {
        let val = bytes as f64 / MB as f64;
        format!("{:.1} MB", val)
    } else if bytes < TB {
        let val = bytes as f64 / GB as f64;
        format!("{:.2} GB", val)
    } else {
        let val = bytes as f64 / TB as f64;
        format!("{:.2} TB", val)
    }
}

/// Parse a human-readable size string (e.g. "3.97 GB", "500MB", "1.2TB", "1024 B") into bytes.
pub fn parse_human_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Empty size string".to_string());
    }

    let upper = s.to_uppercase();
    let (num_part, unit_part) = if let Some(idx) = upper.find(|c: char| c.is_alphabetic()) {
        (upper[..idx].trim(), upper[idx..].trim())
    } else {
        (upper.as_str(), "B")
    };

    let value: f64 = num_part
        .parse()
        .map_err(|e| format!("Invalid number '{}': {}", num_part, e))?;

    if value < 0.0 {
        return Err("Size cannot be negative".to_string());
    }

    let multiplier = match unit_part {
        "B" | "BYTES" | "BYTE" => 1.0,
        "K" | "KB" | "KIB" => KB as f64,
        "M" | "MB" | "MIB" => MB as f64,
        "G" | "GB" | "GIB" => GB as f64,
        "T" | "TB" | "TIB" => TB as f64,
        other => return Err(format!("Unknown size unit '{}'", other)),
    };

    let total = value * multiplier;
    if total > u64::MAX as f64 {
        return Err("Size overflow".to_string());
    }

    Ok(total.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(847), "847 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(4300), "4.2 KB");
        assert_eq!(format_bytes(134_742_016), "128.5 MB");
        assert_eq!(format_bytes(4_265_380_864), "3.97 GB");
        assert_eq!(format_bytes(1_363_394_412_544), "1.24 TB");
    }

    #[test]
    fn test_parse_human_size() {
        assert_eq!(parse_human_size("847 B").unwrap(), 847);
        assert_eq!(parse_human_size("1.0 KB").unwrap(), 1024);
        assert_eq!(parse_human_size("4.2 KB").unwrap(), 4301);
        assert_eq!(parse_human_size("128.5 MB").unwrap(), 134_742_016);
        assert_eq!(parse_human_size("3.97 GB").unwrap(), 4_262_755_041);
        assert_eq!(parse_human_size("1 TB").unwrap(), TB);
    }
}
