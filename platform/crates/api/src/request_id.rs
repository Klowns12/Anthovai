//! Request ids.
//!
//! A caller-supplied `X-Request-Id` is echoed so their logs and ours line up,
//! but only when it looks sane: it ends up in log lines and error bodies, so an
//! arbitrary string from the network is not welcome.

use anthovai_core::RequestId;

const MAX_LEN: usize = 64;

/// Use the caller's id when it is safe, otherwise mint one.
pub fn resolve(header: Option<&str>) -> String {
    match header {
        Some(value) if is_acceptable(value) => value.to_owned(),
        _ => RequestId::new().to_string(),
    }
}

fn is_acceptable(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mints_an_id_when_the_caller_sends_none() {
        let id = resolve(None);
        assert!(id.starts_with("req_"));
    }

    #[test]
    fn echoes_a_reasonable_caller_id() {
        assert_eq!(resolve(Some("trace-abc_123")), "trace-abc_123");
    }

    #[test]
    fn rejects_ids_that_could_poison_logs_or_headers() {
        for hostile in [
            "",
            "has spaces",
            "new\nline",
            "semi;colon",
            &"x".repeat(MAX_LEN + 1),
        ] {
            let id = resolve(Some(hostile));
            assert!(
                id.starts_with("req_"),
                "should have been replaced: {hostile:?}"
            );
        }
    }
}
