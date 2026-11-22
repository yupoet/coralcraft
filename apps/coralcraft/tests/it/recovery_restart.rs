//! CoralCraft (P5 Task 4): the real crash/restart acceptance — a test application process
//! started from this binary with its preferences and recovery folder in a temporary directory,
//! a representative document made and modified over the control channel, a recovery copy
//! written, the child **killed**, and a fresh process started that lists the copy left behind,
//! restores it (with the post-crash edits), and saves it natively. The developer's own app and
//! the normal preferences folder are never touched: everything lives under a temp
//! `CORALCRAFT_CONFIG_DIR` that the test removes.

use std::io::{BufRead, BufReader, Write as _};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_coralcraft");

/// A temporary folder removed when the guard goes (its `Drop`), like the engine tests' `Folder`.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("p5-recovery-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One app process with its control channel, preferences redirected into `config`.
struct App {
    child: Child,
    stream: TcpStream,
    next_id: u64,
}

impl App {
    fn start(config: &std::path::Path) -> Self {
        // A free port: bind, read it, let the socket go so the child can take it.
        let port = {
            let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
            l.local_addr().unwrap().port()
        };
        let mut child = Command::new(BIN)
            .args(["--new-instance", "--control", &port.to_string()])
            .env("CORALCRAFT_CONFIG_DIR", config)
            .env_remove("CORALCRAFT_NO_PREFS")
            .env_remove("CORALCRAFT_CONTROL_PORT")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("the app exited before its control channel answered: {status:?}");
            }
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
                stream.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
                stream.set_write_timeout(Some(Duration::from_secs(60))).unwrap();
                return Self { child, stream, next_id: 0 };
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                panic!("the app's control channel never answered on port {port}");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// One request over the JSON-lines channel; `ok: false` fails the test with the error.
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let line = json!({"method": method, "params": params, "id": self.next_id});
        self.stream.write_all(format!("{line}\n").as_bytes()).unwrap();
        self.stream.flush().unwrap();
        let mut reader = BufReader::new(&mut self.stream);
        let mut text = String::new();
        let n = reader.read_line(&mut text).unwrap();
        assert!(n > 0, "{method}: the control channel closed");
        let reply: Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(reply["id"], json!(self.next_id), "{method}: {reply}");
        assert_eq!(reply["ok"], json!(true), "{method}: {reply}");
        reply["result"].clone()
    }

    /// `engine.execute` of one command.
    fn exec(&mut self, command: &str, params: Value) -> Value {
        self.rpc("engine.execute", json!({"command": command, "params": params}))
    }

    /// The crash: a real kill of the live process (SIGKILL, not the graceful close that would
    /// take the recovery copy with it), consuming the app.
    fn crash(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn a_killed_app_recovers_its_modified_document_in_a_fresh_one() {
    let tmp = TempDir::new("restart");
    let config = tmp.0.join("config");
    std::fs::create_dir_all(&config).unwrap();
    let doc = tmp.0.join("poster.vectorcraft");
    let doc_str = doc.to_string_lossy().into_owned();

    // A live app: make the representative document, save it, edit it (modified), and leave a
    // recovery copy behind.
    let mut first = App::start(&config);
    first.exec("file.new", json!({"width": 420, "height": 300}));
    first.exec("shape.rectangle", json!({"x": 20.0, "y": 20.0, "width": 120.0, "height": 80.0}));
    first.exec("paint.setFill", json!({"color": "#ff5500"}));
    first.exec("text.create", json!({"x": 20.0, "y": 12.0, "text": "Before the crash"}));
    first.exec("file.save", json!({"path": doc_str}));
    // The modification the crash must not lose.
    first.exec("text.create", json!({"x": 20.0, "y": 285.0, "text": "Edited before the kill"}));
    let saved = first.exec("file.recovery.save", json!({}));
    assert_eq!(saved["saved"].as_array().unwrap().len(), 1, "one modified document copied: {saved}");
    assert!(saved["location"].as_str().is_some_and(|l| l.contains("Data Recovery")), "{}", saved["location"]);
    // The copy is on disk, in this temp config only.
    let copies: Vec<std::path::PathBuf> = std::fs::read_dir(config.join("Data Recovery"))
        .unwrap()
        .filter_map(|e| e.ok())
        .flat_map(|e| {
            let area = e.path();
            std::fs::read_dir(&area).ok().into_iter().flatten().filter_map(Result::ok).map(move |f| f.path())
        })
        .collect();
    assert!(copies.iter().any(|p| p.extension().is_some_and(|e| e == "vectorcraft")), "the copy file: {copies:?}");

    // The crash: a real kill of the live process.
    first.crash();
    assert!(doc.metadata().is_ok_and(|m| m.len() > 0), "the saved file still exists");

    // A fresh process in the same (temp) preferences: the copy is a leftover of a crash, it is
    // restored with the edits made before the kill, and it saves natively over the original.
    let mut second = App::start(&config);
    let listed = second.exec("file.recovery.list", json!({}));
    let copies = listed["copies"].as_array().unwrap();
    assert_eq!(copies.len(), 1, "one copy left behind: {copies:?}");
    assert_eq!(copies[0]["path"].as_str(), Some(doc_str.as_str()), "the copy knows its document's file");
    assert_eq!(copies[0]["open"], json!(false), "no app holds it open");
    assert_eq!(copies[0]["running"], json!(false), "the killed app no longer runs");

    let restored = second.exec("file.recovery.restore", json!({}));
    let entries = restored["restored"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "one document restored: {restored}");
    assert!(entries[0]["title"].as_str().is_some_and(|t| t.contains("[Recovered]")), "{}", entries[0]["title"]);
    assert!(entries[0]["path"].as_str() == Some(doc_str.as_str()), "Save will suggest the original file: {entries:?}");

    // The restored document still holds the pre-kill edit.
    let inspected = second.exec("document.json", json!({}));
    let all_text = serde_json::to_string(&inspected).unwrap();
    assert!(all_text.contains("Edited before the kill"), "the pre-kill edit survived: {inspected:?}");

    // And it saves natively over the original file, which then holds the edit too: reopen it and
    // look (the native container may be compressed, so the check reads it back through the app).
    second.exec("file.save", json!({"path": doc_str}));
    assert!(doc.metadata().is_ok_and(|m| m.len() > 0), "the saved file is non-empty");
    second.exec("file.open", json!({"path": doc_str}));
    let reopened = second.exec("document.json", json!({}));
    let all = serde_json::to_string(&reopened).unwrap();
    assert!(all.contains("Edited before the kill"), "the reopened native file holds the pre-kill edit");

    // The copy leaves the store with the saved document.
    let listed = second.exec("file.recovery.list", json!({}));
    assert_eq!(listed["copies"].as_array().unwrap().len(), 0, "saving takes the copy back: {listed}");
}
