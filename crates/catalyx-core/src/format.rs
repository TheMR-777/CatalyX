//! Formatting every front-end shares: one way to print a size, whether the reader is the activity
//! log, the command line or a skin's row. Presentation, not measurement, so it lives outside the
//! sampler.

/// Bytes as a person reads them: "640 MB", "5.2 GB".
pub fn mem(b: u64) -> String {
    let mb = b as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{:.0} MB", mb)
    }
}

#[cfg(test)]
mod tests {
    use super::mem;

    #[test]
    fn sizes_read_in_megabytes_and_gigabytes() {
        assert_eq!(mem(0), "0 MB");
        assert_eq!(mem(640 * 1024 * 1024), "640 MB");
        assert_eq!(mem(5 * 1024 * 1024 * 1024), "5.0 GB");
    }
}
