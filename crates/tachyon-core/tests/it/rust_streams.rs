#![allow(clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use tachyon_core::{DevServer, DevServerOptions, HandlerSource};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct RustStreams;

impl RustStreams {
    const FILES: [(&str, &str); 6] = [
        ("client/pages/tac.html", "<test-ticks />"),
        (
            "client/components/test/ticks/tac.html",
            "<main aria-label=\"Ticks\"><h1>Ticks</h1></main>",
        ),
        (
            "server/models/Tick.rs",
            "pub struct Tick {\n    pub sku: String,\n    pub level: i64,\n}\n",
        ),
        (
            "server/controllers/TicksController.rs",
            "#[RequestRoute(\"/ticks\")]\n#[Public]\nstruct TicksController;\nimpl TicksController {\n    fn new() -> Self { Self }\n    #[StreamRoute]\n    fn stream_ticks(&self) -> impl Iterator<Item = crate::models::Tick> + '_ {\n        self.ticks_for(vec![1, 2, 3])\n    }\n    fn ticks_for(&self, levels: Vec<i64>) -> impl Iterator<Item = crate::models::Tick> + '_ {\n        levels.into_iter().map(|level| crate::models::Tick { sku: String::from(\"anvil\"), level })\n    }\n}\n",
        ),
        (
            "server/controllers/ReadingsController.rs",
            "#[RequestRoute(\"/readings\")]\n#[Public]\nstruct ReadingsController;\nimpl ReadingsController {\n    fn new() -> Self { Self }\n    #[StreamRoute]\n    fn stream_readings(&self) -> Box<dyn Iterator<Item = Result<crate::models::Tick, String>> + '_> {\n        Box::new(self.readings())\n    }\n    fn readings(&self) -> impl Iterator<Item = Result<crate::models::Tick, String>> {\n        vec![Ok(crate::models::Tick { sku: String::from(\"gauge\"), level: 1 }), Err(String::from(\"secret-canary\"))].into_iter()\n    }\n}\n",
        ),
        (
            "server/controllers/AlarmsController.rs",
            "#[RequestRoute(\"/alarms\")]\n#[Public]\nstruct AlarmsController;\nimpl AlarmsController {\n    fn new() -> Self { Self }\n    #[StreamRoute]\n    fn stream_alarms(&self) -> impl Iterator<Item = crate::models::Tick> + '_ {\n        self.alarms()\n    }\n    fn alarms(&self) -> impl Iterator<Item = crate::models::Tick> + '_ {\n        let bell = crate::models::Tick { sku: String::from(\"bell\"), level: 1 };\n        std::iter::once(bell).chain(std::iter::repeat_with(|| self.broken()))\n    }\n    fn broken(&self) -> crate::models::Tick {\n        panic!(\"secret-canary\")\n    }\n}\n",
        ),
    ];

    fn cargo_available() -> bool {
        Command::new("cargo")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    fn project() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("project");
        for (relative, text) in Self::FILES {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
            std::fs::write(path, text).expect("source");
        }
        root
    }

    async fn serve(root: &Path) -> (u16, tokio::sync::oneshot::Sender<()>) {
        let server = DevServer::bind(
            root,
            &DevServerOptions {
                port: 0,
                watch: false,
                ..DevServerOptions::default()
            },
        )
        .await
        .unwrap_or_else(|failure| panic!("{failure}"));
        let port = server.address().port();
        let (stop, shutdown) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _served = server
                .run_until(async {
                    let _stopped = shutdown.await;
                })
                .await;
        });
        (port, stop)
    }

    async fn get(port: u16, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.expect("request");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_mins(5), stream.read_to_end(&mut response))
            .await
            .expect("stream ends within the deadline")
            .expect("read");
        String::from_utf8_lossy(&response).into_owned()
    }
}

#[tokio::test]
async fn a_rust_stream_sends_each_typed_item_and_then_the_end_event() {
    if !RustStreams::cargo_available() {
        eprintln!("skipping: cargo is unavailable");
        return;
    }
    let root = RustStreams::project();
    let (port, stop) = RustStreams::serve(root.path()).await;
    let streamed = RustStreams::get(port, "/ticks").await;
    assert!(streamed.contains("text/event-stream"), "{streamed}");
    let events: Vec<&str> = streamed
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .collect();
    assert_eq!(
        events,
        [
            "{\"sku\":\"anvil\",\"level\":1}",
            "{\"sku\":\"anvil\",\"level\":2}",
            "{\"sku\":\"anvil\",\"level\":3}",
            "{}"
        ],
        "{streamed}"
    );
    assert!(streamed.contains("event: end\ndata: {}\n\n"), "{streamed}");
    assert!(!streamed.contains("event: error"), "{streamed}");
    let _stopped = stop.send(());
}

#[tokio::test]
async fn a_rust_stream_error_or_panic_ends_the_stream_with_an_error_event() {
    if !RustStreams::cargo_available() {
        eprintln!("skipping: cargo is unavailable");
        return;
    }
    let root = RustStreams::project();
    let (port, stop) = RustStreams::serve(root.path()).await;
    for (path, first) in [
        ("/readings", "data: {\"sku\":\"gauge\",\"level\":1}"),
        ("/alarms", "data: {\"sku\":\"bell\",\"level\":1}"),
    ] {
        let streamed = RustStreams::get(port, path).await;
        assert!(streamed.contains(first), "{path}: {streamed}");
        assert!(streamed.contains("event: error"), "{path}: {streamed}");
        assert!(!streamed.contains("event: end"), "{path}: {streamed}");
        assert!(!streamed.contains("secret-canary"), "{path}: {streamed}");
    }
    let _stopped = stop.send(());
}

#[test]
fn a_rust_stream_route_that_returns_a_list_is_refused_with_the_shape() {
    let root = tempfile::tempdir().expect("project");
    let relative = "server/controllers/ListController.rs";
    let path = root.path().join(relative);
    std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
    std::fs::write(
        &path,
        "#[RequestRoute(\"/list\")]\n#[Public]\nstruct ListController;\nimpl ListController {\n    fn new() -> Self { Self }\n    #[StreamRoute]\n    fn stream_levels(&self) -> Vec<i64> {\n        vec![1, 2]\n    }\n}\n",
    )
    .expect("source");
    let Err(refused) = HandlerSource::discover(root.path(), relative) else {
        panic!("a list is not a stream");
    };
    let refused = refused.to_string();
    assert!(refused.contains("impl Iterator<Item = Model>"), "{refused}");
}
