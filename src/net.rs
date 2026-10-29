//! HTTP agents that start no helper thread.
//!
//! ureq's default resolver starts a thread for each request when a timeout is set, even for
//! 127.0.0.1. The NVIDIA OpenGL driver keeps about 80 KB of memory for each thread that ends
//! while the app draws, and never gives it back. With the League Client API asked about three
//! times a second, Hexsight grew by about 9 MB a minute in a game (measured on 2026-10-29). This
//! resolver looks the address up in the calling thread instead. Every agent of the app comes
//! from `agent`.

use std::net::ToSocketAddrs;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

/// The room in a `ResolvedSocketAddrs` (ureq keeps 16 addresses at most).
const MAX_ADDRS: usize = 16;

/// Resolves in the calling thread, with no timeout of its own. An IP address resolves at once.
/// A host name blocks the calling thread for as long as Windows takes, which is fine for the
/// background threads that do the web requests.
#[derive(Debug, Default)]
pub struct InlineResolver;

impl Resolver for InlineResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        _timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let (Some(scheme), Some(authority)) = (uri.scheme(), uri.authority()) else {
            return Err(ureq::Error::HostNotFound);
        };
        let addr =
            DefaultResolver::host_and_port(scheme, authority).ok_or(ureq::Error::HostNotFound)?;
        let mut out = self.empty();
        for a in config
            .ip_family()
            .keep_wanted(addr.to_socket_addrs()?)
            .take(MAX_ADDRS)
        {
            out.push(a);
        }
        if out.is_empty() {
            Err(ureq::Error::HostNotFound)
        } else {
            Ok(out)
        }
    }
}

/// An agent with this config and the `InlineResolver`.
pub fn agent(config: ureq::config::Config) -> ureq::Agent {
    ureq::Agent::with_parts(config, DefaultConnector::new(), InlineResolver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ureq::unversioned::transport::time::Duration as UreqDuration;

    fn resolve(url: &str) -> Result<Vec<std::net::SocketAddr>, ureq::Error> {
        let uri: ureq::http::Uri = url.parse().unwrap();
        let config = ureq::Agent::config_builder().build();
        let timeout = NextTimeout {
            after: UreqDuration::NotHappening,
            reason: ureq::Timeout::Global,
            per_read: None,
            per_write: None,
        };
        InlineResolver
            .resolve(&uri, &config, timeout)
            .map(|a| a.iter().copied().collect())
    }

    #[test]
    fn an_ip_address_resolves_to_itself_with_its_port() {
        assert_eq!(
            resolve("https://127.0.0.1:2999/liveclientdata/gamestats").unwrap(),
            vec!["127.0.0.1:2999".parse().unwrap()]
        );
    }

    #[test]
    fn the_scheme_gives_the_port_when_the_url_has_none() {
        assert_eq!(
            resolve("https://127.0.0.1/x").unwrap(),
            vec!["127.0.0.1:443".parse().unwrap()]
        );
        assert_eq!(
            resolve("http://127.0.0.1/x").unwrap(),
            vec!["127.0.0.1:80".parse().unwrap()]
        );
    }

    #[test]
    fn a_url_without_a_host_is_not_found() {
        assert!(matches!(
            resolve("/only/a/path"),
            Err(ureq::Error::HostNotFound)
        ));
    }

    #[test]
    fn an_agent_with_the_resolver_reaches_a_local_server() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut s, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let _ =
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        });
        let agent = agent(
            ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(5)))
                .build(),
        );
        let body = agent
            .get(&format!("http://127.0.0.1:{port}/"))
            .call()
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap();
        assert_eq!(body, "ok");
    }
}
