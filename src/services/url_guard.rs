//! SSRF guard for outbound image fetches.
//!
//! The `{{image}}` helper takes its URL from caller-supplied render data, so
//! an unguarded fetch lets a client aim the server at anything it can reach —
//! link-local cloud metadata endpoints (`169.254.169.254`), services bound to
//! loopback, or hosts inside a private VPC. Even when the response never makes
//! it into the document, the difference between "connection refused", a
//! timeout, and "invalid image format" is enough to enumerate an internal
//! network.
//!
//! This module enforces two rules before any request goes out:
//!
//! 1. The scheme must be `http` or `https`.
//! 2. Every address the host resolves to must be a global unicast address.
//!
//! Rule 2 is applied to *resolved addresses*, not to the hostname, so
//! `http://internal.example.com/` pointing at `10.0.0.5` is rejected the same
//! as `http://10.0.0.5/`. Redirects are followed manually by the caller so
//! each hop passes through this check too — a permissive redirect policy would
//! otherwise let a public URL bounce to a private one.
//!
//! Operators who genuinely serve images from a private network can opt out
//! with `IMAGE_FETCH_ALLOW_PRIVATE_NETWORKS=true`.
//!
//! Note the residual DNS-rebinding window: the name is resolved here and again
//! by the HTTP client. Closing it entirely requires pinning the connection to
//! the validated address, which `reqwest`'s high-level client does not expose.
//! The exposure is limited to an attacker who controls a DNS server and can
//! win the race, which is a materially higher bar than pasting a URL.

use crate::errors::RenderError;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// True when the operator has opted out of private-network blocking.
pub fn allow_private_networks() -> bool {
    std::env::var("IMAGE_FETCH_ALLOW_PRIVATE_NETWORKS")
        .map(|v| {
            let v = v.trim().to_lowercase();
            v == "true" || v == "1"
        })
        .unwrap_or(false)
}

/// Addresses that must never be reachable from a caller-supplied URL.
///
/// Deliberately broader than "private": loopback, link-local (which covers the
/// cloud metadata endpoint), carrier-grade NAT, multicast, broadcast, and the
/// reserved ranges are all rejected, because none of them are legitimate
/// sources for a document image on the public internet.
pub fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => {
            // Treat IPv4-mapped/compatible addresses as the IPv4 they encode,
            // so `::ffff:169.254.169.254` can't slip past the v4 rules.
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_blocked_ipv4(&mapped);
            }
            if let Some(compat) = v6.to_ipv4() {
                return is_blocked_ipv4(&compat);
            }
            is_blocked_ipv6(v6)
        }
    }
}

fn is_blocked_ipv4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()            // 127.0.0.0/8
        || ip.is_private()      // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local()   // 169.254.0.0/16 — cloud metadata lives here
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_unspecified()  // 0.0.0.0
        || o[0] == 0            // 0.0.0.0/8
        || (o[0] == 100 && (64..128).contains(&o[1]))  // 100.64.0.0/10 CGNAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)     // 192.0.0.0/24
        || (o[0] == 192 && o[1] == 0 && o[2] == 2)     // 192.0.2.0/24 TEST-NET-1
        || (o[0] == 198 && (18..20).contains(&o[1]))   // 198.18.0.0/15 benchmarking
        || (o[0] == 198 && o[1] == 51 && o[2] == 100)  // 198.51.100.0/24 TEST-NET-2
        || (o[0] == 203 && o[1] == 0 && o[2] == 113)   // 203.0.113.0/24 TEST-NET-3
        || o[0] >= 240 // 240.0.0.0/4 reserved
}

fn is_blocked_ipv6(ip: &Ipv6Addr) -> bool {
    let seg = ip.segments();
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (seg[0] & 0xfe00) == 0xfc00  // fc00::/7 unique local
        || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 link local
}

/// Validate a caller-supplied image URL and return the host and port to
/// resolve. Rejects anything that isn't plain HTTP(S).
pub fn validate_scheme(url: &str) -> Result<reqwest::Url, RenderError> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| RenderError::ImageProcessing(format!("Invalid image URL {}: {}", url, e)))?;

    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        other => Err(RenderError::ImageProcessing(format!(
            "Unsupported image URL scheme '{}' — only http and https are allowed",
            other
        ))),
    }
}

/// Resolve `url`'s host and reject the request if any resolved address is
/// non-global. All addresses are checked, not just the first: a hostname with
/// both a public and a loopback record must not be usable to reach loopback.
pub async fn assert_host_is_public(url: &reqwest::Url) -> Result<(), RenderError> {
    if allow_private_networks() {
        return Ok(());
    }

    let host = url.host_str().ok_or_else(|| {
        RenderError::ImageProcessing("Image URL has no host component".to_string())
    })?;

    // Address literals are checked directly rather than handed to the
    // resolver. `host_str()` keeps the brackets on an IPv6 literal (`[::1]`),
    // which no resolver accepts — so without this the address would fail DNS
    // and surface as "could not resolve" instead of being recognised and
    // reported as a blocked address.
    let literal = host.trim_start_matches('[').trim_end_matches(']');

    let addrs: Vec<IpAddr> = match literal.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => {
            let port = url.port_or_known_default().unwrap_or(80);
            tokio::net::lookup_host((host, port))
                .await
                .map_err(|e| {
                    RenderError::ImageProcessing(format!(
                        "Could not resolve image host '{}': {}",
                        host, e
                    ))
                })?
                .map(|sa| sa.ip())
                .collect()
        }
    };

    if addrs.is_empty() {
        return Err(RenderError::ImageProcessing(format!(
            "Image host '{}' did not resolve to any address",
            host
        )));
    }

    if let Some(blocked) = addrs.iter().find(|ip| is_blocked_ip(ip)) {
        return Err(RenderError::ImageProcessing(format!(
            "Refusing to fetch image from '{}': resolves to non-public address {}. \
             Set IMAGE_FETCH_ALLOW_PRIVATE_NETWORKS=true to allow this.",
            host, blocked
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse::<Ipv4Addr>().unwrap())
    }

    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse::<Ipv6Addr>().unwrap())
    }

    #[test]
    fn blocks_cloud_metadata_endpoint() {
        assert!(is_blocked_ip(&v4("169.254.169.254")));
    }

    #[test]
    fn blocks_loopback() {
        assert!(is_blocked_ip(&v4("127.0.0.1")));
        assert!(is_blocked_ip(&v4("127.1.2.3")));
        assert!(is_blocked_ip(&v6("::1")));
    }

    #[test]
    fn blocks_rfc1918_ranges() {
        assert!(is_blocked_ip(&v4("10.0.0.5")));
        assert!(is_blocked_ip(&v4("172.16.0.1")));
        assert!(is_blocked_ip(&v4("172.31.255.254")));
        assert!(is_blocked_ip(&v4("192.168.1.1")));
    }

    #[test]
    fn allows_public_addresses_adjacent_to_private_ranges() {
        assert!(!is_blocked_ip(&v4("172.15.0.1")));
        assert!(!is_blocked_ip(&v4("172.32.0.1")));
        assert!(!is_blocked_ip(&v4("11.0.0.1")));
        assert!(!is_blocked_ip(&v4("9.255.255.255")));
    }

    #[test]
    fn blocks_unspecified_and_zero_network() {
        assert!(is_blocked_ip(&v4("0.0.0.0")));
        assert!(is_blocked_ip(&v4("0.1.2.3")));
        assert!(is_blocked_ip(&v6("::")));
    }

    #[test]
    fn blocks_cgnat_range() {
        assert!(is_blocked_ip(&v4("100.64.0.1")));
        assert!(is_blocked_ip(&v4("100.127.255.255")));
        assert!(!is_blocked_ip(&v4("100.63.255.255")));
        assert!(!is_blocked_ip(&v4("100.128.0.0")));
    }

    #[test]
    fn blocks_multicast_and_reserved() {
        assert!(is_blocked_ip(&v4("224.0.0.1")));
        assert!(is_blocked_ip(&v4("240.0.0.1")));
        assert!(is_blocked_ip(&v4("255.255.255.255")));
    }

    #[test]
    fn blocks_ipv4_mapped_ipv6_bypass() {
        // The classic filter bypass: wrap a blocked v4 address in v6 syntax.
        assert!(is_blocked_ip(&v6("::ffff:169.254.169.254")));
        assert!(is_blocked_ip(&v6("::ffff:127.0.0.1")));
        assert!(is_blocked_ip(&v6("::ffff:10.0.0.1")));
    }

    #[test]
    fn blocks_ipv6_unique_local_and_link_local() {
        assert!(is_blocked_ip(&v6("fc00::1")));
        assert!(is_blocked_ip(&v6("fd12:3456::1")));
        assert!(is_blocked_ip(&v6("fe80::1")));
    }

    #[test]
    fn allows_public_addresses() {
        assert!(!is_blocked_ip(&v4("93.184.216.34")));
        assert!(!is_blocked_ip(&v4("8.8.8.8")));
        assert!(!is_blocked_ip(&v6("2606:2800:220:1:248:1893:25c8:1946")));
    }

    #[test]
    fn validate_scheme_accepts_http_and_https() {
        assert!(validate_scheme("http://example.com/a.png").is_ok());
        assert!(validate_scheme("https://example.com/a.png").is_ok());
    }

    #[test]
    fn validate_scheme_rejects_file_and_other_schemes() {
        for url in [
            "file:///etc/passwd",
            "ftp://example.com/a.png",
            "gopher://example.com/",
            "data:text/plain,hi",
        ] {
            let err = validate_scheme(url).unwrap_err().to_string();
            assert!(
                err.contains("scheme") || err.contains("Invalid image URL"),
                "unexpected error for {url}: {err}"
            );
        }
    }

    #[test]
    fn validate_scheme_rejects_garbage() {
        assert!(validate_scheme("not a url").is_err());
        assert!(validate_scheme("").is_err());
    }

    #[tokio::test]
    async fn assert_host_is_public_rejects_loopback_literal() {
        let url = validate_scheme("http://127.0.0.1:8080/x.png").unwrap();
        let err = assert_host_is_public(&url).await.unwrap_err().to_string();
        assert!(err.contains("non-public"), "unexpected: {err}");
    }

    #[tokio::test]
    async fn assert_host_is_public_rejects_localhost_name() {
        // Resolution-based, not string-based: the hostname is public-looking
        // but resolves to loopback.
        let url = validate_scheme("http://localhost/x.png").unwrap();
        let err = assert_host_is_public(&url).await.unwrap_err().to_string();
        assert!(err.contains("non-public"), "unexpected: {err}");
    }

    #[tokio::test]
    async fn assert_host_is_public_rejects_bracketed_ipv6_literal() {
        // An IPv6 literal host must be checked as an address, not handed to
        // the resolver — `host_str()` keeps the brackets and DNS would reject
        // it, masking a blocked address as a resolution failure.
        let url = validate_scheme("http://[::1]/x.png").unwrap();
        let err = assert_host_is_public(&url).await.unwrap_err().to_string();
        assert!(err.contains("non-public"), "unexpected: {err}");
    }

    #[tokio::test]
    async fn assert_host_is_public_rejects_ipv4_mapped_ipv6_literal() {
        let url = validate_scheme("http://[::ffff:127.0.0.1]/x.png").unwrap();
        let err = assert_host_is_public(&url).await.unwrap_err().to_string();
        assert!(err.contains("non-public"), "unexpected: {err}");
    }

    #[tokio::test]
    async fn assert_host_is_public_allows_public_literal() {
        let url = validate_scheme("http://93.184.216.34/x.png").unwrap();
        assert!(assert_host_is_public(&url).await.is_ok());
    }

    #[tokio::test]
    async fn assert_host_is_public_rejects_metadata_literal() {
        let url = validate_scheme("http://169.254.169.254/latest/meta-data/").unwrap();
        assert!(assert_host_is_public(&url).await.is_err());
    }
}
