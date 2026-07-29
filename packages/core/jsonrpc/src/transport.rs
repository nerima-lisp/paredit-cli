//! Reading and writing JSON-RPC messages over a byte stream.
//!
//! Two framings, one reader. See the crate README for why they differ; the
//! interesting part here is that the reader is written against a stream that
//! misbehaves. An editor being killed mid-message is not an exceptional case in
//! a language server's life, it is how most sessions end.

use std::io::{BufRead, Write};

use serde_json::Value;

use crate::message::{
    Handler, Outcome, Request, ResponseError, error_codes, notification, response,
};

/// How messages are delimited on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// `Content-Length: N\r\n\r\n` then N bytes. The Language Server Protocol.
    Header,
    /// One compact JSON object per line. MCP's stdio transport.
    Line,
}

/// A cap on one message's declared length.
///
/// Without it a `Content-Length` header of `999999999999` makes the server
/// allocate until it is killed, on input that any client can send. 64 MiB is
/// far past any real document and far short of a denial of service.
const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// Reads framed messages from a stream.
#[derive(Debug)]
pub struct Reader<R> {
    input: R,
    framing: Framing,
}

/// Why a read stopped.
#[derive(Debug)]
pub enum ReadError {
    /// The stream ended. Normal.
    Eof,
    /// The stream is still open but this message was unusable.
    Malformed(String),
    Io(std::io::Error),
}

impl<R: BufRead> Reader<R> {
    pub const fn new(input: R, framing: Framing) -> Self {
        Self { input, framing }
    }

    /// Reads the next message.
    pub fn read(&mut self) -> Result<Value, ReadError> {
        let body = match self.framing {
            Framing::Header => self.read_header_framed()?,
            Framing::Line => self.read_line_framed()?,
        };
        serde_json::from_str(&body)
            .map_err(|error| ReadError::Malformed(format!("body is not JSON: {error}")))
    }

    fn read_header_framed(&mut self) -> Result<String, ReadError> {
        let mut length: Option<usize> = None;
        let mut saw_a_header = false;

        loop {
            let mut line = String::new();
            let read = self.input.read_line(&mut line).map_err(ReadError::Io)?;
            if read == 0 {
                return Err(ReadError::Eof);
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                break;
            }
            saw_a_header = true;
            // Header names are case-insensitive per the specification, and
            // clients do disagree about the capitalisation of `Content-Length`.
            let Some((name, value)) = line.split_once(':') else {
                return Err(ReadError::Malformed(format!(
                    "header without a colon: {line}"
                )));
            };
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }

        if !saw_a_header {
            return Err(ReadError::Malformed(
                "empty header block; expected Content-Length".to_owned(),
            ));
        }
        let Some(length) = length else {
            return Err(ReadError::Malformed(
                "header block declares no Content-Length".to_owned(),
            ));
        };
        if length > MAX_MESSAGE_BYTES {
            return Err(ReadError::Malformed(format!(
                "Content-Length {length} exceeds the {MAX_MESSAGE_BYTES}-byte limit"
            )));
        }

        let mut body = vec![0_u8; length];
        // `read_exact` is what makes a truncated stream an EOF rather than a
        // silently short message that then fails to parse as JSON.
        match self.input.read_exact(&mut body) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(ReadError::Eof);
            }
            Err(error) => return Err(ReadError::Io(error)),
        }
        String::from_utf8(body)
            .map_err(|error| ReadError::Malformed(format!("body is not UTF-8: {error}")))
    }

    fn read_line_framed(&mut self) -> Result<String, ReadError> {
        loop {
            let mut line = String::new();
            let read = self.input.read_line(&mut line).map_err(ReadError::Io)?;
            if read == 0 {
                return Err(ReadError::Eof);
            }
            let trimmed = line.trim();
            // A blank line between messages is not an error and not a message.
            if !trimmed.is_empty() {
                return Ok(trimmed.to_owned());
            }
        }
    }
}

/// Writes framed messages to a stream.
#[derive(Debug)]
pub struct Writer<W> {
    output: W,
    framing: Framing,
}

impl<W: Write> Writer<W> {
    pub const fn new(output: W, framing: Framing) -> Self {
        Self { output, framing }
    }

    /// Writes one message and flushes.
    ///
    /// Flushing every time rather than on a timer: an unflushed response is a
    /// hung editor, and these messages are small and infrequent enough that the
    /// syscall is not worth economising.
    pub fn write(&mut self, value: &Value) -> std::io::Result<()> {
        let body = serde_json::to_string(value)?;
        match self.framing {
            Framing::Header => {
                // The length is in *bytes*, not characters. A document holding
                // one non-ASCII character makes the difference, and the failure
                // is a desynchronised stream rather than a bad message.
                write!(self.output, "Content-Length: {}\r\n\r\n", body.len())?;
                self.output.write_all(body.as_bytes())?;
            }
            Framing::Line => {
                // `to_string` never emits a raw newline (it escapes them inside
                // strings), so one message stays one line.
                self.output.write_all(body.as_bytes())?;
                self.output.write_all(b"\n")?;
            }
        }
        self.output.flush()
    }
}

/// Runs a handler over stdin and stdout until the stream ends or the handler
/// stops.
///
/// Returns the process exit code the server should use.
pub fn serve_stdio<H: Handler>(handler: &mut H, framing: Framing) -> std::io::Result<u8> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(
        handler,
        framing,
        std::io::BufReader::new(stdin.lock()),
        stdout.lock(),
    )
}

/// The stdio loop, over any streams. Separated so the tests can drive it.
pub fn serve<H: Handler, R: BufRead, W: Write>(
    handler: &mut H,
    framing: Framing,
    input: R,
    output: W,
) -> std::io::Result<u8> {
    let mut reader = Reader::new(input, framing);
    let mut writer = Writer::new(output, framing);

    loop {
        let message = match reader.read() {
            Ok(message) => message,
            Err(ReadError::Eof) => return Ok(0),
            Err(ReadError::Io(error)) => return Err(error),
            Err(ReadError::Malformed(detail)) => {
                // The stream is still framed correctly enough to continue, so
                // the connection survives one bad message. Dropping it instead
                // would lose every queued message behind it.
                let error = ResponseError::new(error_codes::PARSE_ERROR, detail);
                writer.write(&response(Value::Null, Err(error)))?;
                continue;
            }
        };

        let Some(request) = Request::from_value(&message) else {
            continue;
        };
        let id = request.id.clone();

        let mut pending: Vec<Value> = Vec::new();
        let outcome = {
            let mut notify = |method: &str, params: Value| {
                pending.push(notification(method, params));
            };
            handler.handle(request, &mut notify)
        };
        for message in pending {
            writer.write(&message)?;
        }

        match (outcome, id) {
            (Outcome::Reply(result), Some(id)) => writer.write(&response(id, Ok(result)))?,
            (Outcome::Fail(error), Some(id)) => writer.write(&response(id, Err(error)))?,
            // A notification takes no response, whatever the handler returned.
            // Some clients treat an unsolicited response as a fatal protocol
            // error, so this is a correctness rule and not tidiness.
            (Outcome::Reply(_) | Outcome::Fail(_) | Outcome::Silent, _) => {}
            (Outcome::Stop(result), id) => {
                if let (Some(id), Some(result)) = (id, result) {
                    writer.write(&response(id, Ok(result)))?;
                }
                return Ok(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Outcome;
    use serde_json::json;

    struct Echo;

    impl Handler for Echo {
        fn handle(&mut self, request: Request, notify: &mut dyn FnMut(&str, Value)) -> Outcome {
            match request.method.as_str() {
                "exit" => Outcome::Stop(None),
                "shout" => {
                    notify("event/shouted", json!({ "heard": true }));
                    Outcome::Reply(json!("ok"))
                }
                "boom" => Outcome::Fail(ResponseError::method_not_found("boom")),
                _ => Outcome::Reply(request.params),
            }
        }
    }

    fn header_framed(bodies: &[&str]) -> Vec<u8> {
        let mut input = Vec::new();
        for body in bodies {
            input.extend_from_slice(
                format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes(),
            );
        }
        input
    }

    fn run(input: Vec<u8>, framing: Framing) -> String {
        let mut output = Vec::new();
        serve(
            &mut Echo,
            framing,
            std::io::BufReader::new(std::io::Cursor::new(input)),
            &mut output,
        )
        .expect("the loop completes");
        String::from_utf8(output).expect("utf-8 output")
    }

    #[test]
    fn a_header_framed_request_is_answered_with_the_same_framing() {
        let output = run(
            header_framed(&[r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":{"a":1}}"#]),
            Framing::Header,
        );
        assert!(output.starts_with("Content-Length: "), "{output}");
        assert!(output.contains(r#""result":{"a":1}"#), "{output}");
    }

    /// The length is in bytes. A document with one non-ASCII character is where
    /// a character count desynchronises the stream.
    #[test]
    fn the_content_length_counts_bytes_and_not_characters() {
        let output = run(
            header_framed(&[r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":"é→"}"#]),
            Framing::Header,
        );
        let (header, body) = output.split_once("\r\n\r\n").expect("a framed message");
        let declared: usize = header
            .trim()
            .strip_prefix("Content-Length: ")
            .expect("a length header")
            .parse()
            .expect("a number");
        assert_eq!(declared, body.len());
        assert_ne!(declared, body.chars().count());
    }

    #[test]
    fn a_notification_gets_no_response_at_all() {
        let output = run(
            header_framed(&[r#"{"jsonrpc":"2.0","method":"echo","params":{}}"#]),
            Framing::Header,
        );
        assert!(output.is_empty(), "{output}");
    }

    /// A pushed notification must reach the client before the response to the
    /// request that caused it, which is the order a client's queue assumes.
    #[test]
    fn a_pushed_notification_precedes_the_response_that_produced_it() {
        let output = run(
            header_framed(&[r#"{"jsonrpc":"2.0","id":1,"method":"shout"}"#]),
            Framing::Header,
        );
        let shout = output.find("event/shouted").expect("the notification");
        let reply = output.find(r#""result""#).expect("the response");
        assert!(shout < reply, "{output}");
    }

    /// One unparseable message must not take the connection with it: the
    /// messages queued behind it are still owed answers.
    #[test]
    fn a_malformed_body_is_reported_and_the_connection_survives() {
        let mut input = header_framed(&["not json at all"]);
        input.extend(header_framed(&[
            r#"{"jsonrpc":"2.0","id":2,"method":"echo","params":"after"}"#,
        ]));
        let output = run(input, Framing::Header);
        assert!(output.contains("-32700"), "{output}");
        assert!(output.contains(r#""result":"after""#), "{output}");
    }

    /// A client that dies mid-message leaves a truncated body. That is an end
    /// of stream, not an error to report to a client that is no longer there.
    #[test]
    fn a_truncated_body_ends_the_loop_cleanly() {
        let input = b"Content-Length: 500\r\n\r\n{\"jsonrpc\":\"2.0\"".to_vec();
        let output = run(input, Framing::Header);
        assert!(output.is_empty(), "{output}");
    }

    /// A length any client can send must not be able to make the server
    /// allocate until it dies.
    #[test]
    fn an_absurd_content_length_is_refused_rather_than_allocated() {
        let input = b"Content-Length: 999999999999\r\n\r\n".to_vec();
        let output = run(input, Framing::Header);
        assert!(output.contains("-32700"), "{output}");
        assert!(output.contains("exceeds"), "{output}");
    }

    #[test]
    fn header_names_are_matched_without_regard_to_case() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":"ok"}"#;
        let input = format!("content-length: {}\r\n\r\n{body}", body.len()).into_bytes();
        assert!(run(input, Framing::Header).contains(r#""result":"ok""#));
    }

    #[test]
    fn line_framing_writes_one_message_per_line() {
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"echo\",\"params\":\"a\"}\n",
            "\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"echo\",\"params\":\"b\"}\n",
        )
        .as_bytes()
        .to_vec();
        let output = run(input, Framing::Line);
        assert_eq!(output.lines().count(), 2, "{output}");
        assert!(output.lines().all(|line| line.starts_with('{')), "{output}");
    }

    /// A line-framed message must never contain a raw newline, whatever the
    /// payload holds — that is the whole contract of the framing.
    #[test]
    fn line_framing_escapes_a_newline_inside_a_payload() {
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"echo\",\"params\":\"one\\ntwo\"}\n"
            .as_bytes()
            .to_vec();
        let output = run(input, Framing::Line);
        assert_eq!(output.lines().count(), 1, "{output}");
        assert!(output.contains("one\\ntwo"), "{output}");
    }

    #[test]
    fn stop_ends_the_loop_and_leaves_later_messages_unread() {
        let mut input = header_framed(&[r#"{"jsonrpc":"2.0","method":"exit"}"#]);
        input.extend(header_framed(&[
            r#"{"jsonrpc":"2.0","id":9,"method":"echo","params":"never"}"#,
        ]));
        let output = run(input, Framing::Header);
        assert!(!output.contains("never"), "{output}");
    }

    #[test]
    fn a_failed_request_answers_with_an_error_object() {
        let output = run(
            header_framed(&[r#"{"jsonrpc":"2.0","id":4,"method":"boom"}"#]),
            Framing::Header,
        );
        assert!(output.contains(r#""code":-32601"#), "{output}");
    }
}
