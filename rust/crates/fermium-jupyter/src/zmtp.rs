//! The part of ZeroMQ's wire protocol (ZMTP 3.1, rfc.zeromq.org/spec/37) that a Jupyter kernel needs: TCP,
//! the NULL security mechanism, and the socket types of Jupyter's channels. The kernel binds; the client
//! (jupyter_client, with libzmq) connects:
//!
//! | channel          | kernel | client |
//! |------------------|--------|--------|
//! | shell, control, stdin | ROUTER | DEALER |
//! | iopub            | PUB    | SUB    |
//! | heartbeat        | REP    | REQ    |
//!
//! Each accepted connection gets a thread that reads its messages: a ROUTER forwards them to the kernel
//! (tagged with the connection, which is how replies are routed), a REP echoes them (the heartbeat), a PUB
//! discards what subscribers send (subscriptions: every message is sent to every subscriber, which is what
//! Jupyter clients subscribe to).
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Router,
    Pub,
    Rep,
    Dealer,
    Sub,
    Req,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Router => "ROUTER",
            Kind::Pub => "PUB",
            Kind::Rep => "REP",
            Kind::Dealer => "DEALER",
            Kind::Sub => "SUB",
            Kind::Req => "REQ",
        }
    }
}

/// What arrives on a connection.
#[derive(Debug)]
pub enum Incoming {
    Message(Vec<Vec<u8>>),
    Command(String, Vec<u8>),
}

fn greeting() -> [u8; 64] {
    let mut g = [0u8; 64];
    g[0] = 0xFF;
    g[9] = 0x7F;
    g[10] = 3;
    g[11] = 1;
    g[12..16].copy_from_slice(b"NULL");
    g
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// The greeting and the NULL mechanism's READY exchange. Returns the peer's properties (Socket-Type, Identity).
pub fn handshake(s: &mut TcpStream, kind: Kind) -> io::Result<HashMap<String, Vec<u8>>> {
    s.set_nodelay(true)?;
    s.write_all(&greeting())?;
    let mut peer = [0u8; 64];
    s.read_exact(&mut peer[..10])?;
    if peer[0] != 0xFF || peer[9] & 1 != 1 {
        return Err(bad("not a ZMTP peer"));
    }
    s.read_exact(&mut peer[10..])?;
    if peer[10] < 3 {
        return Err(bad("ZMTP 3 or later is needed"));
    }
    if &peer[12..16] != b"NULL" {
        return Err(bad("only the NULL security mechanism is supported"));
    }
    let mut props = vec![];
    prop(&mut props, "Socket-Type", kind.name().as_bytes());
    if matches!(kind, Kind::Dealer | Kind::Req) {
        prop(&mut props, "Identity", b"");
    }
    write_command(s, "READY", &props)?;
    match read(s)? {
        Incoming::Command(name, body) if name == "READY" => parse_props(&body),
        Incoming::Command(name, _) if name == "ERROR" => Err(bad("the peer refused the connection")),
        _ => Err(bad("expected READY")),
    }
}

fn prop(out: &mut Vec<u8>, name: &str, value: &[u8]) {
    out.push(name.len() as u8);
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value);
}

fn parse_props(b: &[u8]) -> io::Result<HashMap<String, Vec<u8>>> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < b.len() {
        let n = b[i] as usize;
        let name = b.get(i + 1..i + 1 + n).ok_or_else(|| bad("bad property"))?;
        i += 1 + n;
        let len = b.get(i..i + 4).ok_or_else(|| bad("bad property"))?;
        let len = u32::from_be_bytes([len[0], len[1], len[2], len[3]]) as usize;
        let value = b.get(i + 4..i + 4 + len).ok_or_else(|| bad("bad property"))?;
        i += 4 + len;
        m.insert(String::from_utf8_lossy(name).into_owned(), value.to_vec());
    }
    Ok(m)
}

fn write_frame(w: &mut dyn Write, flags: u8, body: &[u8], out: &mut Vec<u8>) {
    let _ = w;
    if body.len() > 255 {
        out.push(flags | 0x02);
        out.extend_from_slice(&(body.len() as u64).to_be_bytes());
    } else {
        out.push(flags);
        out.push(body.len() as u8);
    }
    out.extend_from_slice(body);
}

/// A command frame (READY, PING, PONG …).
pub fn write_command(w: &mut dyn Write, name: &str, data: &[u8]) -> io::Result<()> {
    let mut body = vec![name.len() as u8];
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(data);
    let mut out = vec![];
    write_frame(w, 0x04, &body, &mut out);
    w.write_all(&out)?;
    w.flush()
}

/// A message of one or more frames, written in one go.
pub fn write_message(w: &mut dyn Write, frames: &[Vec<u8>]) -> io::Result<()> {
    let mut out = vec![];
    for (i, f) in frames.iter().enumerate() {
        let more = if i + 1 < frames.len() { 0x01 } else { 0 };
        write_frame(w, more, f, &mut out);
    }
    w.write_all(&out)?;
    w.flush()
}

/// Read the next message or command.
pub fn read(r: &mut dyn Read) -> io::Result<Incoming> {
    let mut frames = vec![];
    loop {
        let mut f = [0u8; 1];
        r.read_exact(&mut f)?;
        let flags = f[0];
        let len = if flags & 0x02 != 0 {
            let mut b = [0u8; 8];
            r.read_exact(&mut b)?;
            u64::from_be_bytes(b) as usize
        } else {
            let mut b = [0u8; 1];
            r.read_exact(&mut b)?;
            b[0] as usize
        };
        if len > 1 << 31 {
            return Err(bad("frame too large"));
        }
        let mut body = vec![0u8; len];
        r.read_exact(&mut body)?;
        if flags & 0x04 != 0 {
            let n = *body.first().ok_or_else(|| bad("empty command"))? as usize;
            let name = String::from_utf8_lossy(body.get(1..1 + n).ok_or_else(|| bad("bad command"))?).into_owned();
            return Ok(Incoming::Command(name, body[1 + n..].to_vec()));
        }
        frames.push(body);
        if flags & 0x01 == 0 {
            return Ok(Incoming::Message(frames));
        }
    }
}

/// A message that arrived on a ROUTER socket: which channel, which connection, the frames.
#[derive(Debug)]
pub struct Event {
    pub channel: &'static str,
    pub conn: u64,
    pub frames: Vec<Vec<u8>>,
}

/// A bound socket: its connections, for sending.
#[derive(Clone)]
pub struct Bound {
    pub port: u16,
    conns: Arc<Mutex<HashMap<u64, TcpStream>>>,
}

impl Bound {
    /// Send to one connection (ROUTER: the one a request came from).
    pub fn send(&self, conn: u64, frames: &[Vec<u8>]) {
        let mut m = self.conns.lock().unwrap();
        if let Some(s) = m.get_mut(&conn) {
            if write_message(s, frames).is_err() {
                m.remove(&conn);
            }
        }
    }

    /// Send to every connection (PUB).
    pub fn broadcast(&self, frames: &[Vec<u8>]) {
        let mut m = self.conns.lock().unwrap();
        let dead: Vec<u64> = m.iter_mut().filter_map(|(k, s)| write_message(s, frames).err().map(|_| *k)).collect();
        for k in dead {
            m.remove(&k);
        }
    }

    pub fn connections(&self) -> usize {
        self.conns.lock().unwrap().len()
    }
}

static NEXT_CONN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Bind `ip:port` as a socket of this kind; ROUTER messages go to `tx` tagged with `channel`.
pub fn bind(ip: &str, port: u16, kind: Kind, channel: &'static str, tx: Option<Sender<Event>>) -> io::Result<Bound> {
    let listener = TcpListener::bind((ip, port))?;
    let port = listener.local_addr()?.port();
    let conns: Arc<Mutex<HashMap<u64, TcpStream>>> = Arc::new(Mutex::new(HashMap::new()));
    let b = Bound { port, conns: conns.clone() };
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let Ok(mut s) = s else { continue };
            let (conns, tx) = (conns.clone(), tx.clone());
            std::thread::spawn(move || {
                if handshake(&mut s, kind).is_err() {
                    return;
                }
                let id = NEXT_CONN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let Ok(w) = s.try_clone() {
                    conns.lock().unwrap().insert(id, w);
                }
                loop {
                    match read(&mut s) {
                        Ok(Incoming::Message(frames)) => match kind {
                            Kind::Rep => {
                                // the heartbeat: send the envelope and the payload back
                                let mut m = conns.lock().unwrap();
                                if let Some(w) = m.get_mut(&id) {
                                    let _ = write_message(w, &frames);
                                }
                            }
                            Kind::Router => {
                                if let Some(tx) = &tx {
                                    if tx.send(Event { channel, conn: id, frames }).is_err() {
                                        break;
                                    }
                                }
                            }
                            _ => {} // subscriptions on a PUB socket: everything goes to everyone
                        },
                        Ok(Incoming::Command(name, ctx)) => {
                            if name == "PING" {
                                // PING: ttl (2 bytes) then context, sent back in PONG
                                let mut m = conns.lock().unwrap();
                                if let Some(w) = m.get_mut(&id) {
                                    let _ = write_command(w, "PONG", ctx.get(2..).unwrap_or(&[]));
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
                conns.lock().unwrap().remove(&id);
            });
        }
    });
    Ok(b)
}

/// Connect as a client (tests play a Jupyter client with this).
pub fn connect(ip: &str, port: u16, kind: Kind) -> io::Result<TcpStream> {
    let mut s = TcpStream::connect((ip, port))?;
    handshake(&mut s, kind)?;
    Ok(s)
}
