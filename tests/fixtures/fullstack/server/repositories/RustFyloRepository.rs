use std::env;
use std::io::Write;
use std::process::{Command, Stdio};

pub struct RustFyloRepository {
    root: String,
    executable: Option<String>,
}

impl RustFyloRepository {
    pub fn new() -> RustFyloRepository {
        RustFyloRepository {
            root: env::var("FYLO_ROOT").unwrap_or_else(|_| "db".to_string()),
            executable: env::var("FYLO_EXEC_PATH")
                .or_else(|_| env::var("FYLO_BINARY"))
                .ok()
                .filter(|value| !value.is_empty())
                .or_else(|| Some("fylo".to_string())),
        }
    }

    pub fn disposable_sample(&self) -> YonJson {
        let collection = "fylo-rust-disposable";
        self.machine(YonJson::object(vec![("op", "createCollection".into()), ("collection", collection.into())]));
        self.machine(YonJson::object(vec![("op", "inspectCollection".into()), ("collection", collection.into())]));
        self.machine(YonJson::object(vec![("op", "dropCollection".into()), ("collection", collection.into())]));
        YonJson::object(vec![
            ("collection", collection.into()),
            ("operations", YonJson::array(vec!["createCollection".into(), "inspectCollection".into(), "dropCollection".into()])),
            ("resultCount", "3".into()),
        ])
    }

    fn machine(&self, request: YonJson) -> YonJson {
        let mut command = Command::new(self.executable.as_deref().unwrap_or("fylo"));
        command.arg("exec");
        let mut child = command
            .args(["--request", "-", "--root", &self.root])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Unable to start fylo exec");
        child.stdin.as_mut().expect("stdin").write_all(request.stringify().as_bytes()).expect("write fylo request");
        let output = child.wait_with_output().expect("wait for fylo exec");
        if !output.status.success() {
            panic!("{}", String::from_utf8_lossy(&output.stderr));
        }
        let response = YonJson::parse(&String::from_utf8_lossy(&output.stdout)).expect("parse fylo response");
        let ok = response.get("ok").map(|value| value.as_string()).unwrap_or_default();
        if ok != "true" {
            panic!("fylo exec returned an error");
        }
        response.get("result").cloned().unwrap_or(YonJson::Null)
    }
}
