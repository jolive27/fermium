//! Scripted language-server sessions compared with Fermium 1.5's server (fermium/lsp.py over pygls). The
//! fixtures are written by `python3 rust/tools/lsp_session.py --write-fixtures`: the messages sent, and every
//! message v1 sent back after `initialize` (diagnostics, hover, completion, code actions, the shutdown reply).
//! Messages are compared as JSON values (key order aside). The process exit code is not compared: pygls exits
//! with 1 after shutdown + exit, the protocol (and this server) says 0.
use fermium_lsp::json::Json;
use fermium_lsp::server::Server;

fn canon(j: &Json) -> Json {
    match j {
        Json::Obj(v) => {
            let mut v: Vec<(String, Json)> = v.iter().map(|(k, x)| (k.clone(), canon(x))).collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Obj(v)
        }
        Json::Arr(v) => Json::Arr(v.iter().map(canon).collect()),
        x => x.clone(),
    }
}

fn messages(bytes: &[u8]) -> Vec<Json> {
    let mut r = std::io::BufReader::new(bytes);
    let mut out = vec![];
    while let Some(m) = fermium_lsp::server::read_message(&mut r) {
        out.push(m.unwrap());
    }
    out
}

#[test]
fn sessions_match_v1() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("sessions");
    let mut n = 0;
    for i in 0.. {
        let Ok(input) = std::fs::read_to_string(dir.join(format!("{i:03}.in.json"))) else { break };
        let want: Vec<Json> = std::fs::read_to_string(dir.join(format!("{i:03}.out.jsonl")))
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with("{\"exit code\""))
            .map(|l| canon(&Json::parse(l).unwrap()))
            .collect();
        let mut s = Server::new(Vec::new());
        let mut id = 1i64;
        s.handle(&Json::parse(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#).unwrap());
        let init_len = s.output().len();
        s.handle(&Json::parse(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#).unwrap());
        for m in Json::parse(&input).unwrap().arr() {
            let (method, params, notify) = (m.arr()[0].clone(), m.arr()[1].clone(), m.arr()[2].bool().unwrap());
            let mut msg = Json::obj(vec![("jsonrpc", Json::from("2.0")), ("method", method), ("params", params)]);
            if !notify {
                id += 1;
                msg.set("id", Json::from(id));
            }
            s.handle(&msg);
        }
        id += 1;
        s.handle(&Json::obj(vec![("jsonrpc", Json::from("2.0")), ("id", Json::from(id)),
                                 ("method", Json::from("shutdown")), ("params", Json::Null)]));
        let got: Vec<Json> = messages(&s.output()[init_len..]).iter().map(canon).collect();
        assert_eq!(got.len(), want.len(), "session {i}: {} messages, v1 sent {}", got.len(), want.len());
        for (k, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, w, "session {i}, message {k}:\n rust {}\n v1   {}", g.dump(), w.dump());
        }
        n += 1;
    }
    assert!(n >= 4, "the fixtures are missing");
}
