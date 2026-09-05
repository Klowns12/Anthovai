//! Deciding whether we are willing to fetch a URL the customer gave us.
//!
//! This is the one place in the platform where a customer decides what address
//! our servers connect to, which makes it the one place server-side request
//! forgery is possible. A URL that resolves to `169.254.169.254` reaches the
//! cloud metadata service and its credentials; one that resolves to `127.0.0.1`
//! reaches our own admin ports. Neither is reachable from the customer's
//! network — which is exactly why they would ask us to fetch it.
//!
//! So the address is checked, not the name: DNS is resolved here and every
//! resolved IP is tested. A guard that inspected only the hostname would be
//! defeated by a DNS record pointing at a private address.
//!
//! It lives here rather than beside the HTTP client because both sides need it
//! — the upload endpoint, to refuse a URL while the customer is still looking
//! at the screen, and the worker, to re-check every redirect it is asked to
//! follow.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

use anthovai_core::{DomainError, Result};
use url::{Host, Url};

/// What a refused URL is reported as, everywhere.
pub const URL_NOT_ALLOWED: &str = "url_not_allowed";

/// The URL, if we are willing to fetch it.
///
/// Every rejection is the same error code. Which check failed is useful to us
/// and to the customer, but telling them *which* private address their name
/// resolved to would turn this endpoint into a port scanner.
pub fn allowed(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| refused("that is not a valid URL"))?;

    match url.scheme() {
        "http" | "https" => {}
        other => return Err(refused(&format!("`{other}` addresses are not fetched"))),
    }

    // Credentials in a URL are a signal that it was meant for something other
    // than public reading, and we would be the one sending them.
    if !url.username().is_empty() || url.password().is_some() {
        return Err(refused("a URL with credentials in it is not fetched"));
    }

    let host = url.host().ok_or_else(|| refused("that URL has no host"))?;

    match host {
        // A literal IP skips DNS, so it is checked directly.
        Host::Ipv4(ip) => guard(IpAddr::V4(ip))?,
        Host::Ipv6(ip) => guard(IpAddr::V6(ip))?,
        Host::Domain(name) => {
            // Resolve here and check every answer. A name with two A records,
            // one public and one private, must not be fetched at all.
            let port = url.port_or_known_default().unwrap_or(80);
            let addresses = (name, port)
                .to_socket_addrs()
                .map_err(|_| refused("that host could not be resolved"))?;

            let mut any = false;
            for address in addresses {
                any = true;
                guard(address.ip())?;
            }
            if !any {
                return Err(refused("that host could not be resolved"));
            }
        }
    }

    Ok(url)
}

/// Whether one resolved address is somewhere we are willing to connect.
///
/// The list is written out rather than left to `is_global`, which is still
/// unstable, and each range is here because it reaches something the customer
/// could not reach themselves.
fn guard(ip: IpAddr) -> Result<()> {
    let private = match ip {
        IpAddr::V4(ip) => is_private_v4(ip),
        IpAddr::V6(ip) => is_private_v6(ip),
    };

    if private {
        return Err(refused("that address is not reachable from the internet"));
    }
    Ok(())
}

fn is_private_v4(ip: Ipv4Addr) -> bool {
    ip.is_private()            // 10/8, 172.16/12, 192.168/16
        || ip.is_loopback()    // 127/8
        || ip.is_link_local()  // 169.254/16 — cloud metadata lives here
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified() // 0.0.0.0, which several stacks route to localhost
        || ip.octets()[0] == 0
        // 100.64/10, the carrier-grade NAT range: a container network's
        // addresses on more than one cloud.
        || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
        // 192.0.0/24 and 198.18/15, both reserved for protocol assignment
        || (ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 0)
        || (ip.octets()[0] == 198 && (18..=19).contains(&ip.octets()[1]))
        || ip.is_multicast()
}

fn is_private_v6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }

    let segments = ip.segments();

    // An IPv4-mapped address is a v4 address wearing a v6 hat; checking it as
    // v6 would let `::ffff:127.0.0.1` straight through.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_private_v4(v4);
    }

    // fc00::/7, unique local addresses
    if segments[0] & 0xfe00 == 0xfc00 {
        return true;
    }
    // fe80::/10, link local
    if segments[0] & 0xffc0 == 0xfe80 {
        return true;
    }
    // 2001:db8::/32, documentation
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return true;
    }

    false
}

fn refused(why: &str) -> DomainError {
    DomainError::rejected(URL_NOT_ALLOWED, why)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The code a refused URL comes back with.
    fn refusal(url: &str) -> String {
        rejection(url).code()
    }

    fn rejection(url: &str) -> DomainError {
        allowed(url)
            .err()
            .unwrap_or_else(|| panic!("{url} should have been refused"))
    }

    #[test]
    fn a_public_address_is_fetched() {
        assert!(allowed("https://www.anthovai.com/pricing").is_ok());
        assert!(allowed("http://93.184.216.34/index.html").is_ok());
    }

    #[test]
    fn the_cloud_metadata_service_is_not_reachable() {
        // The single most valuable target of an SSRF: on AWS, GCP and Azure
        // alike this address hands out credentials to whatever asks.
        assert!(refusal("http://169.254.169.254/latest/meta-data/") == URL_NOT_ALLOWED);
    }

    #[test]
    fn our_own_ports_are_not_reachable() {
        for url in [
            "http://127.0.0.1:8080/internal/health",
            "http://localhost:5432/",
            "http://10.0.0.5/admin",
            "http://172.16.0.1/",
            "http://192.168.1.1/",
            "http://[::1]:8080/",
            "http://0.0.0.0:8080/",
        ] {
            assert!(refusal(url) == URL_NOT_ALLOWED, "{url} was allowed");
        }
    }

    #[test]
    fn a_container_network_address_is_not_reachable() {
        // 100.64/10 is where more than one cloud puts container networking.
        assert!(refusal("http://100.100.100.200/") == URL_NOT_ALLOWED);
    }

    #[test]
    fn an_ipv4_address_wearing_an_ipv6_hat_is_still_that_address() {
        // Checking this one as IPv6 would let loopback straight through.
        assert!(is_private_v6("::ffff:127.0.0.1".parse().unwrap()));
        assert!(is_private_v6("::ffff:169.254.169.254".parse().unwrap()));
        assert!(!is_private_v6("::ffff:93.184.216.34".parse().unwrap()));
    }

    #[test]
    fn only_http_and_https_are_fetched() {
        for url in [
            "file:///etc/passwd",
            "gopher://127.0.0.1:6379/_INFO",
            "ftp://example.com/x",
            "data:text/html,<h1>hi</h1>",
        ] {
            assert!(refusal(url) == URL_NOT_ALLOWED, "{url} was allowed");
        }
    }

    #[test]
    fn credentials_in_a_url_are_not_sent_on_the_customers_behalf() {
        assert!(refusal("http://admin:secret@example.com/") == URL_NOT_ALLOWED);
    }

    #[test]
    fn a_refusal_never_says_what_it_found() {
        // Different answers per address would make this endpoint a port
        // scanner for anyone with an API key.
        let metadata = rejection("http://169.254.169.254/").to_string();
        let loopback = rejection("http://127.0.0.1/").to_string();
        assert_eq!(metadata, loopback);
        assert!(!metadata.contains("169.254"), "{metadata}");
    }

    #[test]
    fn nonsense_is_refused_rather_than_resolved() {
        assert!(refusal("not a url") == URL_NOT_ALLOWED);
        assert!(refusal("https://") == URL_NOT_ALLOWED);
    }

    #[test]
    fn the_v4_ranges_are_what_the_specification_lists() {
        for ip in [
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "0.0.0.0",
        ] {
            assert!(is_private_v4(ip.parse().unwrap()), "{ip} was allowed");
        }

        for ip in ["8.8.8.8", "93.184.216.34", "172.32.0.1", "1.1.1.1"] {
            assert!(!is_private_v4(ip.parse().unwrap()), "{ip} was refused");
        }
    }

    #[test]
    fn the_v6_ranges_are_what_the_specification_lists() {
        for ip in ["::1", "fc00::1", "fd12:3456::1", "fe80::1"] {
            assert!(is_private_v6(ip.parse().unwrap()), "{ip} was allowed");
        }
        assert!(!is_private_v6("2606:4700:4700::1111".parse().unwrap()));
    }
}
