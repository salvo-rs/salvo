use http::uri::Scheme;

use crate::Request;

/// Policy for deciding whether response cookies should include the `Secure` attribute.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SecureCookiePolicy {
    /// Always include the `Secure` attribute.
    Always,
    /// Never include the `Secure` attribute.
    Never,
    /// Include the `Secure` attribute when the request scheme is HTTPS.
    #[default]
    AutoFromScheme,
}

impl SecureCookiePolicy {
    /// Create a fixed secure-cookie policy from a boolean.
    #[inline]
    #[must_use]
    pub fn from_bool(secure: bool) -> Self {
        if secure { Self::Always } else { Self::Never }
    }

    /// Resolve this policy for a request.
    #[inline]
    #[must_use]
    pub fn is_secure(self, req: &Request) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::AutoFromScheme => req.scheme() == &Scheme::HTTPS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::TestClient;

    #[test]
    fn secure_cookie_policy_from_bool_uses_fixed_policy() {
        assert_eq!(
            SecureCookiePolicy::from_bool(true),
            SecureCookiePolicy::Always
        );
        assert_eq!(
            SecureCookiePolicy::from_bool(false),
            SecureCookiePolicy::Never
        );
    }

    #[test]
    fn secure_cookie_policy_resolves_from_scheme() {
        let http_req = TestClient::get("http://example.com/").build();
        let https_req = TestClient::get("https://example.com/").build();

        assert!(!SecureCookiePolicy::AutoFromScheme.is_secure(&http_req));
        assert!(SecureCookiePolicy::AutoFromScheme.is_secure(&https_req));
    }

    #[test]
    fn secure_cookie_policy_uses_recorded_scheme_when_uri_has_no_scheme() {
        let hyper_req = http::Request::builder()
            .uri("/")
            .body(crate::http::ReqBody::None)
            .expect("build request");
        let req = Request::from_hyper(hyper_req, Scheme::HTTPS);

        assert_eq!(req.uri().scheme(), None);
        assert!(SecureCookiePolicy::AutoFromScheme.is_secure(&req));
    }
}
