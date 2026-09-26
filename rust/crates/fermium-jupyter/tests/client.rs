//! A Jupyter client played against the kernel over real TCP sockets (ZMTP + signed messages), so the kernel is
//! tested without Jupyter installed. rust/tools/jupyter_e2e.py runs the same checks with jupyter_client.
use std::net::TcpStream;

use fermium_jupyter::kernel::{decode, encode, new_id, now_iso, Connection, Kernel, Message};
use fermium_jupyter::zmtp::{connect, read, write_message, Incoming, Kind};
use fermium_lsp::json::Json;

const KEY: &[u8] = b"a-test-key";

struct Client {
    shell: TcpStream,
    iopub: TcpStream,
    session: String,
}

impl Client {
    fn send(&mut self, msg_type: &str, content: Json) -> String {
        let id = new_id();
        let header = Json::obj(vec![("msg_id", Json::from(id.clone())), ("session", Json::from(self.session.clone())),
                                    ("username", Json::from("test")), ("date", Json::from(now_iso())),
                                    ("msg_type", Json::from(msg_type)), ("version", Json::from("5.3"))]);
        let e = Json::obj::<&str>(vec![]);
        write_message(&mut self.shell, &encode(&[], &header, &e, &e, &content, KEY)).unwrap();
        id
    }

    fn reply(&mut self) -> Message {
        match read(&mut self.shell).unwrap() {
            Incoming::Message(f) => decode(&f, KEY).expect("a signed reply"),
            _ => panic!("expected a message"),
        }
    }

    /// The iopub messages for a request, up to its idle status.
    fn outputs(&mut self, id: &str) -> Vec<Message> {
        let mut out = vec![];
        loop {
            let Incoming::Message(f) = read(&mut self.iopub).unwrap() else { continue };
            let m = decode(&f, KEY).expect("signed iopub message");
            if m.parent.get("msg_id").str() != Some(id) {
                continue;
            }
            let idle = m.msg_type() == "status" && m.content.get("execution_state").str() == Some("idle");
            out.push(m);
            if idle {
                return out;
            }
        }
    }

    fn execute(&mut self, code: &str) -> (String, Vec<(String, String)>) {
        let id = self.send("execute_request", Json::obj(vec![("code", Json::from(code)), ("silent", Json::from(false))]));
        let outs = self.outputs(&id);
        let r = self.reply();
        let streams = outs
            .iter()
            .filter(|m| m.msg_type() == "stream")
            .map(|m| (m.content.get("name").str().unwrap().to_string(), m.content.get("text").str().unwrap().to_string()))
            .collect();
        (r.content.get("status").str().unwrap().to_string(), streams)
    }
}

#[test]
fn a_client_session() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let c = Connection { ip: "127.0.0.1".into(), key: KEY.to_vec(), shell_port: 0, iopub_port: 0,
                                 stdin_port: 0, control_port: 0, hb_port: 0 };
            let mut k = Kernel::bind(&c).unwrap();
            tx.send(k.ports()).unwrap();
            k.run();
        })
        .unwrap();
    let (shell, iopub, _stdin, _control, hb) = rx.recv().unwrap();
    let mut sub = connect("127.0.0.1", iopub, Kind::Sub).unwrap();
    write_message(&mut sub, &[vec![1u8]]).unwrap(); // subscribe to everything
    let mut c = Client { shell: connect("127.0.0.1", shell, Kind::Dealer).unwrap(), iopub: sub, session: new_id() };

    // the heartbeat echoes
    let mut h = connect("127.0.0.1", hb, Kind::Req).unwrap();
    write_message(&mut h, &[vec![], b"ping".to_vec()]).unwrap();
    assert!(matches!(read(&mut h).unwrap(), Incoming::Message(f) if f == vec![vec![], b"ping".to_vec()]));

    // kernel_info
    let id = c.send("kernel_info_request", Json::obj::<&str>(vec![]));
    let r = c.reply();
    assert_eq!(r.msg_type(), "kernel_info_reply");
    assert_eq!(r.parent.get("msg_id").str(), Some(id.as_str()));
    assert_eq!(r.content.at(&["language_info", "name"]).str(), Some("fermium"));

    assert_eq!(c.execute("L = 1.20 m\nT = 2.21 s\ng = 4π² L / T²\nprint g"),
               ("ok".into(), vec![("stdout".into(), "9.70 m/s²\n".into())]));
    assert_eq!(c.execute("print g in ft/s²"), ("ok".into(), vec![("stdout".into(), "31.8 ft/s²\n".into())]));
    let (status, outs) = c.execute("y = L + T");
    assert_eq!(status, "error");
    assert_eq!(outs[0].0, "stderr");
    assert!(outs[0].1.starts_with("line 1: can't add length [m] to time [s]"));

    c.send("complete_request", Json::obj(vec![("code", Json::from("x = \\ome")), ("cursor_pos", Json::from(8usize))]));
    let r = c.reply();
    assert_eq!(r.content.get("matches").arr(), &[Json::from("ω")]);
    assert_eq!(r.content.get("cursor_start").int(), Some(4));

    c.send("is_complete_request", Json::obj(vec![("code", Json::from("if 1 > 0"))]));
    assert_eq!(c.reply().content.get("status").str(), Some("incomplete"));

    c.send("shutdown_request", Json::obj(vec![("restart", Json::from(false))]));
    let r = c.reply();
    assert_eq!(r.msg_type(), "shutdown_reply");
    assert_eq!(r.content.get("status").str(), Some("ok"));
}
