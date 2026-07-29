//! The smallest HTTP/1.1 server that can carry JSON-RPC.
//!
//! Not a web server. It reads a request line, a header block, and a
//! `Content-Length` body; it writes a status line, three headers, and a body.
//! Everything HTTP offers beyond that — chunked encoding, keep-alive
//! pipelining, compression, ranges — is absent because a JSON-RPC call over
//! loopback needs none of it, and each one would be surface to get wrong.
//!
//! What it *does* take seriously is the two ways a local HTTP server is
//! attacked. Both are handled by [`Request::authorized`] and the bind check in
//! the parent module; see there for why a header is the defence.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

/// A cap on the request body.
///
/// A local server still faces a local process sending `Content-Length:
/// 999999999999`. 16 MiB is far past any source file this tool reads in one
/// call.
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub(super) struct Request {
    pub method: String,
    pub target: String,
    pub body: String,
    bearer: Option<String>,
}

#[derive(Debug)]
pub(super) enum RequestError {
    /// The connection ended before a request arrived. Normal: a health checker
    /// opening and closing a socket does this.
    Closed,
    Malformed(String),
    Io(std::io::Error),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("the connection closed before a request arrived"),
            Self::Malformed(detail) => write!(formatter, "malformed request: {detail}"),
            Self::Io(error) => write!(formatter, "read failed: {error}"),
        }
    }
}

impl Request {
    pub(super) fn read(stream: &mut BufReader<&TcpStream>) -> Result<Self, RequestError> {
        let mut line = String::new();
        if stream.read_line(&mut line).map_err(RequestError::Io)? == 0 {
            return Err(RequestError::Closed);
        }
        let mut parts = line.split_whitespace();
        let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
            return Err(RequestError::Malformed("malformed request line".to_owned()));
        };
        let (method, target) = (method.to_owned(), target.to_owned());

        let mut length = 0_usize;
        let mut bearer = None;
        loop {
            let mut header = String::new();
            if stream.read_line(&mut header).map_err(RequestError::Io)? == 0 {
                return Err(RequestError::Closed);
            }
            let header = header.trim_end_matches(['\r', '\n']);
            if header.is_empty() {
                break;
            }
            let Some((name, value)) = header.split_once(':') else {
                return Err(RequestError::Malformed(format!("bad header: {header}")));
            };
            let value = value.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => {
                    length = value.parse().map_err(|_| {
                        RequestError::Malformed(format!("bad Content-Length: {value}"))
                    })?;
                }
                "authorization" => {
                    bearer = value.strip_prefix("Bearer ").map(str::to_owned);
                }
                _ => {}
            }
        }

        if length > MAX_BODY_BYTES {
            return Err(RequestError::Malformed(format!(
                "body of {length} bytes exceeds the {MAX_BODY_BYTES}-byte limit"
            )));
        }
        let mut body = vec![0_u8; length];
        stream.read_exact(&mut body).map_err(RequestError::Io)?;
        let body = String::from_utf8(body)
            .map_err(|error| RequestError::Malformed(format!("body is not UTF-8: {error}")))?;

        Ok(Self {
            method,
            target,
            body,
            bearer,
        })
    }

    /// Whether this request carries the session token.
    ///
    /// A bearer *header* rather than a query parameter or a cookie, and this is
    /// the whole defence against the two attacks a loopback HTTP server faces.
    /// A page in the user's browser can POST to `127.0.0.1` across origins, and
    /// with a DNS rebind it can read the reply — but it cannot set an
    /// `Authorization` header without a CORS preflight this server never
    /// approves. A token in the URL would travel in the browser's own request
    /// and defeat that.
    ///
    /// The comparison is constant-time in the length it shares, so a caller
    /// cannot recover the token a byte at a time from response timing.
    pub(super) fn authorized(&self, token: &str) -> bool {
        let Some(bearer) = &self.bearer else {
            return false;
        };
        if bearer.len() != token.len() {
            return false;
        }
        bearer
            .bytes()
            .zip(token.bytes())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
    }
}

/// Writes one response and closes.
pub(super) fn respond(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         X-Content-Type-Options: nosniff\r\n\
         \r\n",
        body.len(),
    )?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A token in a header cannot be forged by a browser; the check has to be
    /// exact, and absent must not pass.
    #[test]
    fn authorization_requires_the_exact_token() {
        let request = |bearer: Option<&str>| Request {
            method: "POST".to_owned(),
            target: "/".to_owned(),
            body: String::new(),
            bearer: bearer.map(str::to_owned),
        };
        assert!(request(Some("s3cret")).authorized("s3cret"));
        assert!(!request(Some("s3cre")).authorized("s3cret"));
        assert!(!request(Some("s3crea")).authorized("s3cret"));
        assert!(!request(None).authorized("s3cret"));
    }
}
