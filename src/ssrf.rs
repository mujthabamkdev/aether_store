use std::net::{Ipv4Addr, Ipv6Addr};

/// Result of an SSRF guard check.
#[derive(Debug, PartialEq, Eq)]
pub enum SsrfVerdict {
    Allow,
    Deny(&'static str),
}

/// Block requests to private/loopback/link-local IPs and metadata services.
/// Sovereignty law (sensitivity >= 2) additionally pins traffic to .my /
/// localhost.
pub fn check_endpoint(url: &str, sensitivity: u8) -> SsrfVerdict {
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return SsrfVerdict::Deny("invalid url"),
    };
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return SsrfVerdict::Deny("scheme not allowed");
    }
    let host = match parsed.host_str() {
        Some(h) => h,
        None => return SsrfVerdict::Deny("no host"),
    };
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "localhost" {
        // loopback OK
    } else {
        match parsed.host() {
            Some(url::Host::Ipv4(v4)) => {
                // Sovereignty law: sensitivity >= 2 pins data to localhost/.my,
                // so loopback is the *allowed* zone for sovereign fetches.
                if sensitivity < 2 || !v4.is_loopback() {
                    if let Some(v) = check_v4(v4) {
                        return SsrfVerdict::Deny(v);
                    }
                }
            }
            Some(url::Host::Ipv6(v6)) => {
                if sensitivity < 2 || !v6.is_loopback() {
                    if let Some(v) = check_v6(v6) {
                        return SsrfVerdict::Deny(v);
                    }
                }
            }
            Some(url::Host::Domain(_)) => {
                // Sovereign data must stay in .my
                if sensitivity >= 2 && !host_lower.ends_with(".my") {
                    return SsrfVerdict::Deny("sovereign data must stay on .my domain");
                }
            }
            None => return SsrfVerdict::Deny("no host"),
        }
    }
    // Block well-known metadata hostnames even if they resolve publicly.
    if host_lower == "metadata.google.internal" || host_lower.ends_with(".amazonaws.com") && host_lower.starts_with("169.254.") {
        return SsrfVerdict::Deny("metadata endpoint blocked");
    }
    SsrfVerdict::Allow
}

fn check_v4(ip: Ipv4Addr) -> Option<&'static str> {
    if ip.is_loopback() {
        Some("loopback blocked unless sovereignty=2 + localhost host")
    } else if ip.is_private() {
        Some("private ipv4 blocked")
    } else if ip.is_link_local() {
        Some("link-local ipv4 blocked")
    } else if ip.is_unspecified() {
        Some("unspecified ipv4 blocked")
    } else if ip.is_multicast() {
        Some("multicast blocked")
    } else if ip.octets()[0] == 169 && ip.octets()[1] == 254 {
        Some("link-local 169.254.x.x blocked")
    } else {
        None
    }
}

fn check_v6(ip: Ipv6Addr) -> Option<&'static str> {
    if ip.is_loopback() {
        Some("loopback ipv6 blocked")
    } else if ip.is_unspecified() {
        Some("unspecified ipv6 blocked")
    } else if ip.is_multicast() {
        Some("multicast ipv6 blocked")
    } else {
        None
    }
}
