//! A one-connection HTTP/1.1 client, which is all a loopback load generator
//! needs.
//!
//! It is deliberately not a client library. A load generator that shares a
//! pool measures the pool; this owns exactly one socket, keeps it alive across
//! requests, and hands back the wall time the server took to answer on it. The
//! only two body framings a `127.0.0.1` axum server produces are
//! `Content-Length` and `Transfer-Encoding: chunked`, and both are read here;
//! anything else is an error rather than a guess.

use std::io;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// What the server said.
pub struct Reply {
    pub status: u16,
    /// Every `set-cookie` line, unparsed.
    pub cookies: Vec<String>,
    pub body: String,
    /// The change-feed offset the reply landed at, from `x-rn-offset`. A
    /// redirect has no body to say it in, which is why it is a header.
    pub offset: Option<u64>,
    /// How long the request took, measured around the write and the read.
    pub took: Duration,
}

impl Reply {
    /// The reply's body as JSON, or an error naming what came back instead.
    pub fn json(&self) -> Result<serde_json::Value, String> {
        serde_json::from_str(&self.body)
            .map_err(|error| format!("{error} — status {} body {}", self.status, self.body))
    }

    /// The `rn_session` token this reply minted, if it minted one.
    pub fn session(&self) -> Option<String> {
        self.cookies.iter().find_map(|line| {
            line.strip_prefix("rn_session=")
                .map(|rest| rest.split(';').next().unwrap_or_default().to_owned())
        })
    }
}

/// One keep-alive connection to one node.
///
/// It remembers the highest offset the server has told it about and sends it
/// back on every subsequent request (`x-rn-after`), which is what stops a
/// follower answering out of a world this connection has already seen past.
/// The seeder needed a retry loop for exactly that (`seed::command`); with
/// the offset threaded it does not.
pub struct Conn {
    stream: BufReader<TcpStream>,
    host: String,
    after: u64,
}

impl Conn {
    /// Open a connection to `host` (`127.0.0.1:3161`), with Nagle off so a
    /// small request is not held back for an ack.
    pub async fn connect(host: &str) -> io::Result<Self> {
        let stream = TcpStream::connect(host).await?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream: BufReader::new(stream),
            host: host.to_owned(),
            after: 0,
        })
    }

    /// GET a path with an optional session cookie.
    pub async fn get(&mut self, path: &str, cookie: Option<&str>) -> io::Result<Reply> {
        self.request("GET", path, cookie, None).await
    }

    /// The highest offset this connection has been told about.
    pub fn after(&self) -> u64 {
        self.after
    }

    /// POST a body. `body` is `(content_type, bytes)`.
    pub async fn post(
        &mut self,
        path: &str,
        cookie: Option<&str>,
        body: (&str, &str),
    ) -> io::Result<Reply> {
        self.request("POST", path, cookie, Some(body)).await
    }

    async fn request(
        &mut self,
        method: &str,
        path: &str,
        cookie: Option<&str>,
        body: Option<(&str, &str)>,
    ) -> io::Result<Reply> {
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nhost: {}\r\nconnection: keep-alive\r\n\
             sec-fetch-site: same-origin\r\naccept: application/json\r\n",
            self.host
        );
        if let Some(cookie) = cookie {
            head.push_str(&format!("cookie: rn_session={cookie}\r\n"));
        }
        if self.after > 0 {
            head.push_str(&format!("x-rn-after: {}\r\n", self.after));
        }
        match body {
            Some((kind, payload)) => head.push_str(&format!(
                "content-type: {kind}\r\ncontent-length: {}\r\n\r\n{payload}",
                payload.len()
            )),
            None => head.push_str("\r\n"),
        }

        // Keep-alive is a courtesy, not a promise: a connection the seeder
        // left idle while other connections did the bulk comes back closed,
        // and the first sign of it is an empty read where a status line
        // should be. Reconnecting and sending again is what a client library
        // would do; doing it here keeps a thirteen-second pause from ending a
        // run. Once only — a second failure is the server, not the socket.
        match self.attempt(&head).await {
            Ok(reply) => {
                if let Some(offset) = reply.offset {
                    self.after = self.after.max(offset);
                }
                Ok(reply)
            }
            Err(error) if reusable(&error) => {
                let stream = TcpStream::connect(&self.host).await?;
                stream.set_nodelay(true)?;
                self.stream = BufReader::new(stream);
                let reply = self.attempt(&head).await?;
                if let Some(offset) = reply.offset {
                    self.after = self.after.max(offset);
                }
                Ok(reply)
            }
            Err(error) => Err(error),
        }
    }

    async fn attempt(&mut self, head: &str) -> io::Result<Reply> {
        let started = Instant::now();
        self.stream.get_mut().write_all(head.as_bytes()).await?;
        self.stream.get_mut().flush().await?;
        self.read_reply(started).await
    }

    async fn read_reply(&mut self, started: Instant) -> io::Result<Reply> {
        let mut line = String::new();
        self.stream.read_line(&mut line).await?;
        if line.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the server closed the connection before answering",
            ));
        }
        let status = line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);

        let mut cookies = Vec::new();
        let mut offset: Option<u64> = None;
        let mut length: Option<usize> = None;
        let mut chunked = false;
        loop {
            let mut header = String::new();
            self.stream.read_line(&mut header).await?;
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            let (name, value) = header.split_once(':').unwrap_or((header, ""));
            let value = value.trim();
            match name.to_ascii_lowercase().as_str() {
                "set-cookie" => cookies.push(value.to_owned()),
                "x-rn-offset" => offset = value.parse().ok(),
                "content-length" => length = value.parse().ok(),
                "transfer-encoding" if value.eq_ignore_ascii_case("chunked") => chunked = true,
                _ => {}
            }
        }

        let body = if chunked {
            self.read_chunked().await?
        } else {
            let mut buffer = vec![0u8; length.unwrap_or(0)];
            self.stream.read_exact(&mut buffer).await?;
            String::from_utf8_lossy(&buffer).into_owned()
        };

        Ok(Reply {
            status,
            cookies,
            body,
            offset,
            took: started.elapsed(),
        })
    }

    async fn read_chunked(&mut self) -> io::Result<String> {
        let mut body = Vec::new();
        loop {
            let mut size = String::new();
            self.stream.read_line(&mut size).await?;
            let size = usize::from_str_radix(size.trim(), 16)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "a chunk size"))?;
            if size == 0 {
                let mut trailer = String::new();
                self.stream.read_line(&mut trailer).await?;
                break;
            }
            let mut chunk = vec![0u8; size + 2];
            self.stream.read_exact(&mut chunk).await?;
            chunk.truncate(size);
            body.extend_from_slice(&chunk);
        }
        Ok(String::from_utf8_lossy(&body).into_owned())
    }
}

/// Whether an error is the socket rather than the server — the kinds a fresh
/// connection makes go away.
fn reusable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
    )
}
