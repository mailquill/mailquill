use serde::Deserialize;

/// Explicit user response to a TLS certificate verification failure.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TlsDecision {
    Accept,
    AcceptAlways,
    Deny,
}

impl TlsDecision {
    pub fn permits_retry(self) -> bool {
        matches!(self, Self::Accept | Self::AcceptAlways)
    }

    pub fn persists_exception(self) -> bool {
        matches!(self, Self::AcceptAlways)
    }
}

#[cfg(test)]
mod tests {
    use super::TlsDecision;

    #[test]
    fn only_accept_always_persists_the_exception() {
        assert!(TlsDecision::Accept.permits_retry());
        assert!(!TlsDecision::Accept.persists_exception());
        assert!(TlsDecision::AcceptAlways.permits_retry());
        assert!(TlsDecision::AcceptAlways.persists_exception());
        assert!(!TlsDecision::Deny.permits_retry());
        assert!(!TlsDecision::Deny.persists_exception());
    }

    #[test]
    fn parses_the_public_api_values() {
        assert_eq!(
            serde_json::from_str::<TlsDecision>(r#""accept""#).unwrap(),
            TlsDecision::Accept
        );
        assert_eq!(
            serde_json::from_str::<TlsDecision>(r#""accept_always""#).unwrap(),
            TlsDecision::AcceptAlways
        );
        assert_eq!(
            serde_json::from_str::<TlsDecision>(r#""deny""#).unwrap(),
            TlsDecision::Deny
        );
    }
}
