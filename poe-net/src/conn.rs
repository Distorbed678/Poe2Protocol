//! Buffered TCP connection with per-direction stream ciphers.
//!
//! Because messages carry no length prefix, the protocol layer drives decoding: it inspects
//! [`Connection::buffer`], consumes what it could decode and calls [`Connection::read_more`]
//! when a message is incomplete.

use std::net::SocketAddr;

use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::trace;

use crate::{
    Plain,
    cipher::StreamCipher,
    codec::{DecodeError, Reader},
    hexdump::hexdump,
};

pub struct Connection {
    stream: TcpStream,
    peer: SocketAddr,
    inbound: BytesMut,
    /// Largest number of unconsumed bytes `read_more` accepts (`usize::MAX`: no limit).
    max_buffer: usize,
    recv_cipher: Box<dyn StreamCipher>,
    send_cipher: Box<dyn StreamCipher>,
}

impl Connection {
    pub fn new(stream: TcpStream) -> std::io::Result<Self> {
        let _ = stream.set_nodelay(true);
        let peer = stream.peer_addr()?;
        Ok(Self {
            stream,
            peer,
            inbound: BytesMut::with_capacity(16 * 1024),
            max_buffer: usize::MAX,
            recv_cipher: Box::new(Plain),
            send_cipher: Box::new(Plain),
        })
    }

    pub fn peer(&self) -> SocketAddr {
        self.peer
    }

    /// Limits how many unconsumed bytes may pile up while a message is incomplete (`None`: no
    /// limit, the default). A peer that announces a huge message and trickles bytes, or sends
    /// data that never decodes, makes [`Connection::read_more`] fail with
    /// [`std::io::ErrorKind::InvalidData`] once the buffer passes the limit (by at most one
    /// socket read, about 8 KiB). Use it for connections that are not authenticated yet.
    pub fn set_max_buffer(&mut self, max: Option<usize>) {
        self.max_buffer = max.unwrap_or(usize::MAX);
    }

    /// Switches ciphers. Bytes already buffered stay as they were decrypted; only data
    /// received from now on uses the new cipher.
    pub fn set_ciphers(&mut self, recv: Box<dyn StreamCipher>, send: Box<dyn StreamCipher>) {
        self.recv_cipher = recv;
        self.send_cipher = send;
    }

    /// Decrypted bytes received but not yet consumed.
    pub fn buffer(&self) -> &[u8] {
        &self.inbound
    }

    pub fn consume(&mut self, len: usize) {
        self.inbound.advance(len);
    }

    /// Reads at least one more byte from the socket. Returns `Ok(0)` when the peer closed.
    pub async fn read_more(&mut self) -> std::io::Result<usize> {
        let start = self.inbound.len();
        self.inbound.reserve(8 * 1024);
        let n = self.stream.read_buf(&mut self.inbound).await?;
        if n > 0 {
            crate::stats::add_in(n);
            self.recv_cipher.apply(&mut self.inbound[start..]);
            trace!(peer = %self.peer, len = n, "recv\n{}", hexdump(&self.inbound[start..]));
            if self.inbound.len() > self.max_buffer {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("inbound buffer over its limit of {} bytes", self.max_buffer),
                ));
            }
        }
        Ok(n)
    }

    /// Decodes one message with `decode`, reading from the socket until it has enough bytes.
    ///
    /// `decode` must return [`DecodeError::Incomplete`] when the buffer ends early. Returns
    /// `Ok(None)` if the peer closed the connection before a full message arrived.
    pub async fn decode<T>(
        &mut self,
        mut decode: impl FnMut(&mut Reader<'_>) -> Result<T, DecodeError>,
    ) -> std::io::Result<Option<T>> {
        loop {
            let mut reader = Reader::new(&self.inbound);
            match decode(&mut reader) {
                Ok(value) => {
                    let used = reader.position();
                    self.consume(used);
                    return Ok(Some(value));
                }
                Err(DecodeError::Incomplete { .. }) => {
                    if self.read_more().await? == 0 {
                        return Ok(None);
                    }
                }
                Err(e) => {
                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e));
                }
            }
        }
    }

    /// Encrypts (if a cipher is set) and writes a complete message.
    pub async fn send(&mut self, data: &[u8]) -> std::io::Result<()> {
        trace!(peer = %self.peer, len = data.len(), "send\n{}", hexdump(data));
        let mut out = data.to_vec();
        self.send_cipher.apply(&mut out);
        self.stream.write_all(&out).await?;
        crate::stats::add_out(out.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{TcpListener, TcpStream};

    async fn pair() -> (Connection, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (server, _) = listener.accept().await.unwrap();
        (Connection::new(server).unwrap(), client)
    }

    /// A decoder that never completes: the buffer must stop growing at the limit.
    #[tokio::test]
    async fn buffer_limit_stops_a_message_that_never_completes() {
        let (mut conn, mut client) = pair().await;
        conn.set_max_buffer(Some(1024));
        let writer = tokio::spawn(async move {
            let chunk = [7u8; 512];
            // Writes fail once the server closes; that is the expected end.
            while client.write_all(&chunk).await.is_ok() {}
        });
        let err = conn
            .decode(|r| -> Result<(), DecodeError> {
                Err(DecodeError::Incomplete {
                    offset: r.position(),
                    needed: 1,
                })
            })
            .await
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("limit"), "{err}");
        assert!(conn.buffer().len() <= 1024 + 16 * 1024);
        drop(conn);
        writer.abort();
    }

    #[tokio::test]
    async fn no_limit_by_default_and_small_messages_pass() {
        let (mut conn, mut client) = pair().await;
        conn.set_max_buffer(Some(1024));
        client.write_all(&[1, 2, 3]).await.unwrap();
        let got = conn
            .decode(|r| r.u8())
            .await
            .unwrap()
            .expect("one byte decoded");
        assert_eq!(got, 1);
        assert_eq!(conn.buffer(), &[2, 3]);
    }
}
