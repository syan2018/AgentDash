use regex::Regex;

const REDACTED: &str = "***";

pub fn redact_secret(value: &str) -> String {
    let mut redacted = value.to_string();
    let patterns = [
        r#"(?i)(\bBearer\s+)[^\s,;"]+"#,
        r#"(?i)(\b(?:access_token|refresh_token|auth_token|relay_token|registration_token|token)\s*=\s*)[^\s&;,]+"#,
        r#"(?i)("(?:access_token|refresh_token|auth_token|relay_token|registration_token|token)"\s*:\s*")[^"]+"#,
    ];

    for pattern in patterns {
        let regex = Regex::new(pattern).expect("redaction regex must compile");
        redacted = regex
            .replace_all(&redacted, format!("$1{REDACTED}"))
            .to_string();
    }

    redacted
}

pub fn redact_optional(value: Option<&str>) -> Option<String> {
    value.map(redact_secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_token_markers_and_bearer_header() {
        let raw = "Authorization: Bearer adrt_secret auth_token=relay-secret&x=1";

        let redacted = redact_secret(raw);

        assert_eq!(redacted, "Authorization: Bearer *** auth_token=***&x=1");
    }

    #[test]
    fn redacts_json_token_fields() {
        let raw = r#"{"registration_token":"adrt_1_secret","auth_token":"relay"}"#;

        let redacted = redact_secret(raw);

        assert_eq!(
            redacted,
            r#"{"registration_token":"***","auth_token":"***"}"#
        );
    }

    #[test]
    fn redacts_url_query_token_variants() {
        let raw = "wss://example.test/ws?token=relay&relay_token=relay2&access_token=access&refresh_token=refresh";

        let redacted = redact_secret(raw);

        assert_eq!(
            redacted,
            "wss://example.test/ws?token=***&relay_token=***&access_token=***&refresh_token=***"
        );
    }
}
