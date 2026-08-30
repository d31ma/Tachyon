//! Runtime HTTP dispatch to supervised Yon handlers.
//!
//! These tests drive the real development server over a real socket. Before
//! this capability existed the server could only serve generated files; a
//! request could never reach a handler.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::time::Duration;
use tachyon_core::{DevServer, DevServerOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    fs::write(path, contents).expect("source");
}

/// Writes a project with one static page and one dynamic handler route.
fn write_project(root: &Path) {
    write(
        &root.join("client/pages/tac.html"),
        "<main aria-label=\"Home\"><h1>Home</h1></main>",
    );
    write(
        &root.join("server/routes/items/_id/yon.js"),
        r"@Controller
export class ItemsController {
  static GET(request) {
    return { id: request.parameters.id, method: request.method, requestId: request.request_id }
  }
  static POST(request) {
    return { created: request.parameters.id, body: request.body?.data ?? null }
  }
}
",
    );
}

/// Sends one raw HTTP/1.1 request and returns the whole response.
async fn request(port: u16, raw: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    stream.write_all(raw.as_bytes()).await.expect("write");
    stream.flush().await.expect("flush");
    let mut response = Vec::new();
    // The server closes the connection, so a read to end is bounded.
    tokio::time::timeout(Duration::from_secs(30), stream.read_to_end(&mut response))
        .await
        .expect("response within the deadline")
        .expect("read");
    String::from_utf8_lossy(&response).into_owned()
}

async fn get(port: u16, path: &str) -> String {
    request(
        port,
        &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"),
    )
    .await
}

async fn read_until(stream: &mut TcpStream, needle: &str) -> String {
    let mut received = Vec::new();
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut chunk = [0_u8; 4_096];
        loop {
            let count = stream.read(&mut chunk).await.expect("stream read");
            assert_ne!(count, 0, "stream closed before {needle}");
            received.extend_from_slice(&chunk[..count]);
            if String::from_utf8_lossy(&received).contains(needle) {
                return;
            }
        }
    })
    .await
    .expect("stream event within the deadline");
    String::from_utf8_lossy(&received).into_owned()
}

#[cfg(unix)]
async fn wait_for_stream_close(stream: &mut TcpStream) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = [0_u8; 1_024];
        loop {
            match stream.read(&mut bytes).await {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
        }
    })
    .await
    .expect("stream closes within the server shutdown bound");
}

#[cfg(unix)]
fn process_exists(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[tokio::test]
async fn a_generator_handler_is_served_as_server_sent_events() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Stream</main>",
    );
    write(
        &project.path().join("server/routes/ticks/yon.py"),
        "@Controller\nclass TicksController:\n    @staticmethod\n    @Stream\n    \
         def GET(request):\n        for tick in range(1, 4):\n            \
         yield {\"tick\": tick}\n",
    );
    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(async move {
        let _served = server
            .run_until(async {
                let _stopped = shutdown.await;
            })
            .await;
    });

    let streamed = get(port, "/ticks").await;
    assert!(streamed.contains("text/event-stream"), "{streamed}");
    for tick in 1..=3 {
        assert!(
            streamed.contains(&format!("data: {{\"tick\":{tick}}}")),
            "{streamed}"
        );
    }

    let _stopped = stop.send(());
    let _finished = running.await;
}

#[tokio::test]
async fn a_fast_finite_stream_delivers_every_event_in_exact_order() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Stream</main>",
    );
    write(
        &project.path().join("server/routes/finite/yon.py"),
        "@Controller\nclass FiniteController:\n    @staticmethod\n    @Stream\n    \
         def GET(_request):\n        for sequence in range(64):\n            \
         yield {\"sequence\": sequence}\n",
    );
    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(async move {
        let _served = server
            .run_until(async {
                let _stopped = shutdown.await;
            })
            .await;
    });

    let streamed = get(port, "/finite").await;
    let events = streamed
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| data.starts_with("{\"sequence\":"))
        .collect::<Vec<_>>();
    let expected = (0..64)
        .map(|sequence| format!("{{\"sequence\":{sequence}}}"))
        .collect::<Vec<_>>();
    assert_eq!(events, expected, "finite stream lost or reordered an event");
    assert!(!streamed.contains("event: error"), "{streamed}");

    let _stopped = stop.send(());
    let _finished = running.await;
}

#[cfg(unix)]
#[tokio::test]
async fn shutdown_closes_active_handler_hot_and_topic_streams_and_reaps_the_handler() {
    let project = tempfile::tempdir().expect("project");
    let pid_path = project.path().join("stream.pid");
    let effects_path = project.path().join("stream.effects");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Streams</main>",
    );
    write(
        &project.path().join("server/routes/infinite/yon.py"),
        &format!(
            "from pathlib import Path\nimport os\nimport time\n@Controller\nclass InfiniteController:\n    @staticmethod\n    @Stream\n    def GET(_request):\n        Path({pid_path:?}).write_text(str(os.getpid()))\n        while True:\n            with Path({effects_path:?}).open('a') as effects:\n                effects.write('event\\n')\n            yield {{\"event\": \"owned\"}}\n            time.sleep(0.03)\n"
        ),
    );
    write(
        &project.path().join(".tachyon/topics/chat.jsonl"),
        "{\"message\":\"owned\"}\n",
    );
    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: true,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = shutdown.await;
            })
            .await
    });

    let mut handler = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("handler connect");
    handler
        .write_all(b"GET /infinite HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .expect("handler request");
    assert!(
        read_until(&mut handler, "data: {\"event\":\"owned\"}")
            .await
            .contains("200 OK")
    );

    let mut hot = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("hot connect");
    hot.write_all(b"GET /.tachyon/hot HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .expect("hot request");
    assert!(
        read_until(&mut hot, ": connected")
            .await
            .contains("text/event-stream")
    );

    let mut topic = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("topic connect");
    topic
        .write_all(b"GET /.tachyon/topics/chat HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .expect("topic request");
    assert!(
        read_until(&mut topic, "data: {\"message\":\"owned\"}")
            .await
            .contains("200 OK")
    );

    let pid = fs::read_to_string(&pid_path).expect("stream handler pid");
    stop.send(()).expect("shutdown signal");
    tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .expect("bounded server shutdown")
        .expect("server task")
        .expect("clean shutdown");
    wait_for_stream_close(&mut handler).await;
    wait_for_stream_close(&mut hot).await;
    wait_for_stream_close(&mut topic).await;
    let settled = fs::read_to_string(&effects_path).expect("settled stream effects");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        fs::read_to_string(&effects_path).expect("settled stream effects"),
        settled,
        "stream handler produced effects after shutdown"
    );
    assert!(
        !process_exists(&pid),
        "stream handler process {pid} survived"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn disconnecting_a_stream_client_reaps_its_handler_without_stopping_the_server() {
    let project = tempfile::tempdir().expect("project");
    let pid_path = project.path().join("disconnect.pid");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Disconnect</main>",
    );
    write(
        &project.path().join("server/routes/infinite/yon.py"),
        &format!(
            "from pathlib import Path\nimport os\nimport time\n@Controller\nclass InfiniteController:\n    @staticmethod\n    @Stream\n    def GET(_request):\n        Path({pid_path:?}).write_text(str(os.getpid()))\n        while True:\n            yield {{\"event\": \"active\"}}\n            time.sleep(0.03)\n"
        ),
    );
    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = shutdown.await;
            })
            .await
    });
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    stream
        .write_all(b"GET /infinite HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .expect("request");
    let _first = read_until(&mut stream, "data: {\"event\":\"active\"}").await;
    let pid = fs::read_to_string(&pid_path).expect("handler pid");
    drop(stream);
    tokio::time::timeout(Duration::from_secs(3), async {
        while process_exists(&pid) {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("disconnected handler is reaped");
    assert!(get(port, "/").await.contains("200 OK"));
    stop.send(()).expect("shutdown signal");
    running.await.expect("server task").expect("clean shutdown");
}

#[tokio::test]
async fn a_php_generator_streams_through_the_direct_protocol() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Stream</main>",
    );
    write(
        &project.path().join("server/routes/ticks/yon.php"),
        "<?php\n#[Controller]\nclass TicksController\n{\n    #[Stream]\n    \
         public static function GET(YonRequest $request)\n    {\n        \
         foreach ([1, 2, 3] as $tick) {\n            yield ['tick' => $tick];\n        \
         }\n    }\n}\n",
    );

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(async move {
        let _served = server
            .run_until(async {
                let _stopped = shutdown.await;
            })
            .await;
    });

    let streamed = get(port, "/ticks").await;
    assert!(streamed.contains("text/event-stream"), "{streamed}");
    for tick in 1..=3 {
        assert!(
            streamed.contains(&format!("data: {{\"tick\":{tick}}}")),
            "{streamed}"
        );
    }

    let _stopped = stop.send(());
    let _finished = running.await;
}

#[tokio::test]
async fn a_stream_failure_does_not_expose_handler_error_text() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Stream</main>",
    );
    write(
        &project.path().join("server/routes/ticks/yon.py"),
        "@Controller\nclass TicksController:\n    @staticmethod\n    @Stream\n    \
         def GET(request):\n        yield {\"tick\": 1}\n        \
         raise RuntimeError(\"secret-canary\")\n",
    );
    write(
        &project.path().join("server/routes/fail/yon.py"),
        "@Controller\nclass FailController:\n    @staticmethod\n    @Stream\n    def GET(request):\n        if False:\n            yield None\n        raise RuntimeError(\"before-first-secret\")\n",
    );

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(async move {
        let _served = server
            .run_until(async {
                let _stopped = shutdown.await;
            })
            .await;
    });

    let streamed = get(port, "/ticks").await;
    assert!(streamed.contains("data: {\"tick\":1}"), "{streamed}");
    assert!(streamed.contains("event: error"), "{streamed}");
    assert!(
        streamed.find("data: {\"tick\":1}") < streamed.find("event: error"),
        "data must precede the terminal error: {streamed}"
    );
    assert_eq!(streamed.matches("event: error").count(), 1, "{streamed}");
    assert!(streamed.contains("\"code\":\"TY2107\""), "{streamed}");
    let request_id = streamed
        .lines()
        .find_map(|line| {
            const HEADER: &str = "x-tachyon-request-id: ";
            line.get(..HEADER.len())
                .is_some_and(|name| name.eq_ignore_ascii_case(HEADER))
                .then(|| String::from(&line[HEADER.len()..]))
        })
        .expect("request id header");
    assert!(
        streamed.contains(&format!("\"request_id\":\"{}\"", request_id.trim())),
        "{streamed}"
    );
    assert!(!streamed.contains("secret-canary"), "{streamed}");

    let before_first = get(port, "/fail").await;
    assert!(before_first.contains("event: error"), "{before_first}");
    assert_eq!(
        before_first.matches("event: error").count(),
        1,
        "{before_first}"
    );
    assert!(
        before_first.contains("\"code\":\"TY2107\""),
        "{before_first}"
    );
    assert!(!before_first.contains("data: null"), "{before_first}");
    assert!(
        !before_first.contains("before-first-secret"),
        "{before_first}"
    );
    let before_request_id = before_first
        .lines()
        .find_map(|line| {
            const HEADER: &str = "x-tachyon-request-id: ";
            line.get(..HEADER.len())
                .is_some_and(|name| name.eq_ignore_ascii_case(HEADER))
                .then(|| String::from(&line[HEADER.len()..]))
        })
        .expect("request id header");
    assert!(
        before_first.contains(&format!("\"request_id\":\"{}\"", before_request_id.trim())),
        "{before_first}"
    );

    let _stopped = stop.send(());
    let _finished = running.await;
}

#[tokio::test]
async fn requests_reach_supervised_handlers_with_bound_parameters() {
    let project = tempfile::tempdir().expect("project");
    write_project(project.path());

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    // A dynamic route binds its parameter and reaches the handler.
    let dynamic = get(port, "/items/42").await;
    assert!(dynamic.contains("200 OK"), "{dynamic}");
    assert!(dynamic.contains(r#""id":"42""#), "{dynamic}");
    assert!(dynamic.contains(r#""method":"GET""#), "{dynamic}");

    // The request id reaching the handler is a TTID: eleven uppercase base-36
    // characters, so it is correlatable and sorts chronologically. A per-process
    // counter would repeat itself after a restart.
    let id_start = dynamic.find(r#""requestId":""#).expect("request id");
    let request_id: String = dynamic[id_start + 13..]
        .chars()
        .take_while(|character| *character != '"')
        .collect();
    assert_eq!(request_id.len(), 11, "{request_id}");
    assert!(
        request_id
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()),
        "{request_id}"
    );

    // Parameters are percent-decoded before the handler sees them.
    let encoded = get(port, "/items/a%20b").await;
    assert!(encoded.contains(r#""id":"a b""#), "{encoded}");

    // A body reaches the handler, and a second method on one route works.
    let posted = request(
        port,
        "POST /items/7 HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 7\r\n\
         Content-Type: application/json\r\nConnection: close\r\n\r\n{\"x\":1}",
    )
    .await;
    assert!(posted.contains(r#""created":"7""#), "{posted}");

    // A method the handler does not implement is refused, not crashed.
    let refused = request(
        port,
        "DELETE /items/7 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        refused.contains("405") || refused.contains("404"),
        "{refused}"
    );

    // Static output is still served, and unknown paths are still 404.
    let index = get(port, "/").await;
    assert!(
        index.contains("200 OK") && index.contains("Home"),
        "{index}"
    );
    let missing = get(port, "/nothing-here").await;
    assert!(missing.contains("404"), "{missing}");

    // A traversal attempt never reaches a handler or escapes the output root.
    let traversal = get(port, "/items/%2e%2e%2fsecret").await;
    assert!(
        traversal.contains("404") || traversal.contains("400"),
        "{traversal}"
    );

    let _stopped = stop.send(());
    let _joined = running.await;
}

#[tokio::test]
async fn explicit_html_handler_responses_pass_through_without_rendering() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Client page</main>",
    );
    write(
        &project.path().join("server/routes/html/yon.js"),
        r"@Controller
export class HtmlController {
  static GET() {
    return {
      status: 202,
      headers: { 'Content-Type': 'text/html; charset=utf-8', 'X-Origin': 'handler' },
      body: '<article><h1>Unchanged HTML</h1><p>{not-a-template}</p></article>',
    }
  }
}",
    );

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    let response = get(port, "/html").await;
    assert!(response.contains("202 Accepted"), "{response}");
    assert!(
        response.contains("content-type: text/html; charset=utf-8"),
        "{response}"
    );
    assert!(response.contains("x-origin: handler"), "{response}");
    assert!(
        response.contains("<article><h1>Unchanged HTML</h1><p>{not-a-template}</p></article>"),
        "{response}"
    );
    assert!(!response.contains("tachyon-view"), "{response}");

    let _stopped = stop.send(());
    let _joined = running.await;
}

#[tokio::test]
async fn a_project_without_handlers_still_serves_generated_output() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"Only\"><h1>Only</h1></main>",
    );
    write(
        &project.path().join("client/pages/docs/tac.html"),
        "<main><h1>Documentation index</h1></main>",
    );
    write(
        &project.path().join("client/pages/docs/tac.js"),
        "document.title = 'STATIC_CLIENT_ASSET';\n",
    );
    write(
        &project.path().join("client/pages/docs/_topic/tac.html"),
        "<main><h1>Dynamic documentation template</h1></main>",
    );

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    let index = get(port, "/").await;
    assert!(
        index.contains("200 OK") && index.contains("Only"),
        "{index}"
    );
    let dynamic = get(port, "/docs/routing").await;
    assert!(
        dynamic.contains("200 OK") && dynamic.contains("Dynamic documentation template"),
        "{dynamic}"
    );
    let client_asset = get(port, "/docs/client.js").await;
    assert!(client_asset.contains("200 OK"), "{client_asset}");
    assert!(
        client_asset.contains("STATIC_CLIENT_ASSET"),
        "a generated asset lost precedence over a dynamic page: {client_asset}"
    );
    assert!(
        !client_asset.contains("Dynamic documentation template"),
        "{client_asset}"
    );

    let _stopped = stop.send(());
    let _joined = running.await;
}

#[tokio::test]
async fn watching_rebuilds_and_survives_a_broken_source() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join("client/pages/tac.html");
    write(&source, "<main aria-label=\"W\"><h1>First</h1></main>");

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let output = server
        .build()
        .expect("initial build")
        .output_directory()
        .join("index.html");
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    // A served document carries the reload client, which must be a same-origin
    // file: the server sends `default-src 'self'`, so inline script is refused.
    let first = get(port, "/").await;
    assert!(first.contains("First"), "{first}");
    assert!(first.contains("/.tachyon/live-reload.js"), "{first}");
    assert!(
        !first.contains("<script>let seen"),
        "inline script was injected"
    );
    let client = get(port, "/.tachyon/live-reload.js").await;
    assert!(client.contains("new EventSource(endpoint)"), "{client}");
    assert!(client.contains("tachyon:hot-update"), "{client}");
    assert!(client.contains("updateIslands"), "{client}");
    assert!(client.contains("replaceStyles"), "{client}");
    assert!(client.contains("tac.hotUpdate"), "{client}");
    assert!(!client.contains("DOMParser"), "{client}");
    assert!(!client.contains("__tachyonIslands"), "{client}");
    assert!(
        !client.contains("setInterval("),
        "polling client returned: {client}"
    );

    let mut updates = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect hot stream");
    updates
        .write_all(
            b"GET /.tachyon/hot HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: text/event-stream\r\n\r\n",
        )
        .await
        .expect("open hot stream");
    let connected = read_until(&mut updates, ": connected").await;
    assert!(
        connected.contains("content-type: text/event-stream"),
        "{connected}"
    );
    assert!(connected.contains("x-accel-buffering: no"), "{connected}");

    // Editing a template rebuilds and emits the explicit safe reload fallback.
    let before = get(port, "/.tachyon/live").await;
    write(&source, "<main aria-label=\"W\"><h1>Second</h1></main>");
    let reloaded = read_until(&mut updates, r#""kind":"reload""#).await;
    assert!(reloaded.contains("event: hot"), "{reloaded}");
    assert!(reloaded.contains("client/pages/tac.html"), "{reloaded}");
    let mut rebuilt = String::new();
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        rebuilt = get(port, "/").await;
        if rebuilt.contains("Second") {
            break;
        }
    }
    assert!(rebuilt.contains("Second"), "watch never rebuilt: {rebuilt}");
    assert_ne!(
        before,
        get(port, "/.tachyon/live").await,
        "generation stalled"
    );

    // A broken source must not take the running site down.
    write(&source, "<main><logic :else>orphan</logic></main>");
    let diagnostics = read_until(&mut updates, r#""kind":"diagnostics""#).await;
    assert!(diagnostics.contains(r#""contract_version":1"#));
    assert!(diagnostics.contains("TY"), "{diagnostics}");
    let survived = get(port, "/").await;
    assert!(
        survived.contains("Second"),
        "a failed rebuild broke the site"
    );

    drop(updates);
    let _stopped = stop.send(());
    running
        .await
        .expect("server task")
        .expect("graceful shutdown");

    assert_watcher_stopped(&output, &source).await;
}

async fn assert_watcher_stopped(output: &Path, source: &Path) {
    // Once run_until returns, a later source edit cannot publish another
    // bundle from the server-owned watcher.
    let settled = fs::read(output).expect("settled output");
    write(
        source,
        "<main aria-label=\"W\"><h1>After shutdown</h1></main>",
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        fs::read(output).expect("post-shutdown output"),
        settled,
        "watcher rebuilt after public server shutdown"
    );
}

#[tokio::test]
async fn no_watch_serves_documents_untouched() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"P\"><h1>Plain</h1></main>",
    );
    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    let document = get(port, "/").await;
    assert!(document.contains("Plain"), "{document}");
    assert!(!document.contains("live-reload"), "reload client leaked in");
    for endpoint in [
        "/.tachyon/hot",
        "/.tachyon/live",
        "/.tachyon/live-reload.js",
    ] {
        let response = get(port, endpoint).await;
        assert!(response.contains("404 Not Found"), "{endpoint}: {response}");
    }

    let _stopped = stop.send(());
    let _joined = running.await;
}

/// Returns whether a program is runnable on this machine.
fn available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[tokio::test]
async fn unsupported_route_languages_are_reached_through_relay() {
    // Yon routes are the eight languages that can declare their layer. A
    // program in another language remains reachable through an explicit
    // @Relay command on a method of an explicit delegate.
    if !available("ruby") {
        return;
    }
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"P\"><h1>Polyglot</h1></main>",
    );

    write(
        &project.path().join("server/routes/greet/_name/yon.py"),
        "@Controller\nclass GreetController:\n    @staticmethod\n    def GET(request):\n        \
         return GreetDelegate.GET(request)\n\n@Delegate\nclass GreetDelegate:\n    \
         @staticmethod\n    @Relay(\"ruby\", \"server/delegates/greet.rb\")\n    \
         def GET(request):\n        raise RuntimeError(\"relay body must not run\")\n",
    );
    write(
        &project.path().join("server/delegates/greet.rb"),
        "require 'json'\n\
         request = JSON.parse($stdin.read)\n\
         name = request.dig('parameters', 'name')\n\
         puts JSON.generate({ status: 200, body: JSON.generate({ hello: name }) })\n",
    );

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    let greeted = get(port, "/greet/Ada").await;
    assert!(greeted.contains(r#"{"hello":"Ada"}"#), "{greeted}");

    let _stopped = stop.send(());
    let _joined = running.await;
}

#[tokio::test]
async fn an_unrunnable_handler_extension_fails_closed() {
    // A route in a language without annotation syntax is refused and directed
    // to a delegate, even if an interpreter could run it.
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"P\"><h1>P</h1></main>",
    );
    write(
        &project.path().join("server/routes/thing/yon.zig"),
        "// no interpreter registered, not executable\n",
    );

    let error = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect_err("unrunnable handler");
    let text = error.to_string();
    assert!(text.contains("TY2003"), "{text}");
    assert!(text.contains("Yon runs yon.js"), "{text}");
    assert!(text.contains("delegate"), "{text}");
}

#[tokio::test]
async fn middleware_can_refuse_a_request_before_it_reaches_a_route() {
    // Middleware speaks the same protocol a handler does, so it may be written
    // in any language the project can run. 204 means continue; any other
    // status answers the request without reaching the route.
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"M\"><h1>Open</h1></main>",
    );
    write(
        &project.path().join("server/routes/private/yon.py"),
        "@Controller\nclass PrivateController:\n    @staticmethod\n    def GET(request):\n        \
         return {\"status\": 200, \"headers\": {}, \"body\": \"secret data\"}\n",
    );
    write(
        &project.path().join("middleware.php"),
        r"<?php
$request = json_decode(stream_get_contents(STDIN), true);
$token = $request['headers']['authorization'][0] ?? '';
if (str_starts_with($request['route'], '/private') && $token !== 'let-me-in') {
    echo json_encode(['status' => 401, 'body' => 'unauthorized']);
} else {
    echo json_encode(['status' => 204]);
}
",
    );

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    // Refused before the route runs, so the protected body never appears.
    let refused = get(port, "/private").await;
    assert!(refused.contains("401"), "{refused}");
    assert!(!refused.contains("secret data"), "{refused}");

    // Authorised requests reach the route.
    let allowed = request(
        port,
        "GET /private HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: let-me-in\r\n\
         Connection: close\r\n\r\n",
    )
    .await;
    assert!(allowed.contains("secret data"), "{allowed}");

    // Paths the middleware permits still serve generated output.
    let open = get(port, "/").await;
    assert!(open.contains("Open"), "{open}");

    let _stopped = stop.send(());
    let _joined = running.await;
}

#[tokio::test]
async fn middleware_can_adjust_a_response_after_it_is_produced() {
    // The after phase is overwhelmingly used to add headers, so it receives the
    // status and headers and returns 204 to merge its own. A failure there must
    // never discard a response the request already earned.
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"A\"><h1>After</h1></main>",
    );
    write(
        &project.path().join("server/routes/api/yon.py"),
        "@Controller\nclass PrivateController:\n    @staticmethod\n    def GET(request):\n        \
         return {\"status\": 200, \"headers\": {}, \"body\": '{\"ok\":true}'}\n",
    );
    write(
        &project.path().join("middleware.php"),
        r"<?php
$request = json_decode(stream_get_contents(STDIN), true);
if (($request['operation'] ?? '') === 'middleware.after') {
    $status = $request['headers']['x-tachyon-status'][0] ?? '';
    echo json_encode([
        'status' => 204,
        'headers' => [
            'x-served-by' => ['tachyon'],
            'x-observed-status' => [(string) $status],
        ],
    ]);
} else {
    echo json_encode(['status' => 204]);
}
",
    );

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    // A handler route keeps its body and gains the middleware's headers.
    let handled = get(port, "/api").await;
    assert!(handled.contains("x-served-by: tachyon"), "{handled}");
    assert!(handled.contains("x-observed-status: 200"), "{handled}");
    assert!(handled.contains(r#"{"ok":true}"#), "{handled}");

    // Generated output passes through the same phase.
    let document = get(port, "/").await;
    assert!(document.contains("x-served-by: tachyon"), "{document}");
    assert!(document.contains("After"), "{document}");

    let _stopped = stop.send(());
    let _joined = running.await;
}

#[cfg(unix)]
#[tokio::test]
async fn startup_uses_one_owned_snapshot_for_build_middleware_and_scheduled_workers() {
    let workspace = tempfile::tempdir().expect("workspace");
    let authored = workspace.path().join("project");
    let original = workspace.path().join("original");
    let output = workspace.path().join("web-output");
    let owned_marker = workspace.path().join("owned-worker");
    let planted_marker = workspace.path().join("planted-worker");
    fs::create_dir_all(&authored).expect("project");
    write(
        &authored.join("client/pages/tac.html"),
        "<main><h1>Owned startup page</h1></main>",
    );
    write(
        &authored.join("middleware.py"),
        "@Controller\nclass StartupController:\n    @staticmethod\n    def GET(request):\n        return {\"status\": 204, \"headers\": {\"x-startup-snapshot\": [\"owned\"]}, \"body\": \"\"}\n",
    );
    write(
        &authored.join("server/workers/heartbeat.py"),
        &format!(
            "from pathlib import Path\n@Controller\nclass HeartbeatController:\n    @staticmethod\n    def POST(request):\n        with Path({owned_marker:?}).open('a', encoding='utf-8') as marker:\n            marker.write('owned\\n')\n        return {{\"status\": 204, \"headers\": {{}}, \"body\": \"\"}}\n"
        ),
    );
    write(
        &authored.join(".tachyonrc"),
        r#"{"workers":{"server/workers/heartbeat.py":{"every_seconds":1}}}"#,
    );

    let project = tachyon_core::ProjectDiscovery::discover(&authored).expect("owned snapshot");
    fs::rename(&authored, &original).expect("move authored root");
    fs::create_dir_all(&authored).expect("planted root");
    write(
        &authored.join("client/pages/tac.html"),
        "<main><h1>PLANTED startup page</h1></main>",
    );
    write(
        &authored.join("middleware.py"),
        "@Controller\nclass StartupController:\n    @staticmethod\n    def GET(request):\n        return {\"status\": 204, \"headers\": {\"x-startup-snapshot\": [\"planted\"]}, \"body\": \"\"}\n",
    );
    write(
        &authored.join("server/workers/heartbeat.py"),
        &format!(
            "from pathlib import Path\n@Controller\nclass HeartbeatController:\n    @staticmethod\n    def POST(request):\n        Path({planted_marker:?}).write_text('planted')\n        return {{\"status\": 204, \"headers\": {{}}, \"body\": \"\"}}\n"
        ),
    );
    write(
        &authored.join(".tachyonrc"),
        r#"{"workers":{"server/workers/heartbeat.py":{"every_seconds":86400}}}"#,
    );

    let server = DevServer::bind_project(
        &project,
        &DevServerOptions {
            port: 0,
            watch: false,
            output_directory: output,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind from snapshot");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    let response = get(port, "/").await;
    assert!(response.contains("Owned startup page"), "{response}");
    assert!(response.contains("x-startup-snapshot: owned"), "{response}");
    assert!(!response.contains("PLANTED"), "{response}");
    assert!(
        !response.contains("x-startup-snapshot: planted"),
        "{response}"
    );

    tokio::time::timeout(Duration::from_secs(8), async {
        while !owned_marker.exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("captured one-second worker schedule executed");
    assert_eq!(
        fs::read_to_string(&owned_marker).expect("owned marker"),
        "owned\n"
    );
    assert!(!planted_marker.exists(), "planted worker source executed");

    let _stopped = stop.send(());
    running
        .await
        .expect("server task")
        .expect("graceful shutdown");
    let settled = fs::read_to_string(&owned_marker).expect("settled worker marker");
    tokio::time::sleep(Duration::from_millis(1_250)).await;
    assert_eq!(
        fs::read_to_string(&owned_marker).expect("post-shutdown marker"),
        settled,
        "scheduled worker continued after public server shutdown"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_a_bound_server_starts_neither_workers_nor_watcher_rebuilds() {
    if !available("python3") {
        return;
    }
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join("client/pages/tac.html");
    let marker = project.path().join("worker-marker");
    write(&source, "<main><h1>Before drop</h1></main>");
    write(
        &project.path().join("server/workers/drop.py"),
        &format!(
            "from pathlib import Path\n@Controller\nclass DropController:\n    @staticmethod\n    def POST(request):\n        Path({marker:?}).write_text('leaked')\n        return {{\"status\": 204, \"headers\": {{}}, \"body\": \"\"}}\n"
        ),
    );
    write(
        &project.path().join(".tachyonrc"),
        r#"{"workers":{"server/workers/drop.py":{"every_seconds":1}}}"#,
    );

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let output = server
        .build()
        .expect("initial build")
        .output_directory()
        .join("index.html");
    let published = fs::read(&output).expect("initial output");
    drop(server);
    write(&source, "<main><h1>PLANTED after drop</h1></main>");
    tokio::time::sleep(Duration::from_millis(1_500)).await;

    assert!(
        !marker.exists(),
        "a dropped bound server started its worker"
    );
    assert_eq!(
        fs::read(&output).expect("output after drop"),
        published,
        "a dropped bound server kept a watcher rebuild alive"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn shutdown_cancels_and_reaps_a_hung_worker_within_the_server_bound() {
    if !available("python3") {
        return;
    }
    let project = tempfile::tempdir().expect("project");
    let started = project.path().join("hung-worker-started");
    let finished = project.path().join("hung-worker-finished");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main><h1>Hung worker host</h1></main>",
    );
    write(
        &project.path().join("server/workers/hung.py"),
        &format!(
            "import os\nimport time\nfrom pathlib import Path\n@Controller\nclass HungController:\n    @staticmethod\n    def POST(request):\n        Path({started:?}).write_text(str(os.getpid()))\n        time.sleep(60)\n        Path({finished:?}).write_text('leaked')\n        return {{\"status\": 204, \"headers\": {{}}, \"body\": \"\"}}\n"
        ),
    );
    write(
        &project.path().join(".tachyonrc"),
        r#"{"workers":{"server/workers/hung.py":{"every_seconds":1}}}"#,
    );
    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(8), async {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("hung worker entered user code");
    let pid = fs::read_to_string(&started).expect("worker pid");

    let _stopped = stop.send(());
    tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .expect("bounded server shutdown")
        .expect("server task")
        .expect("graceful shutdown");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!finished.exists(), "hung worker completed after shutdown");
    assert!(
        !std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .status()
            .is_ok_and(|status| status.success()),
        "worker process {pid} survived server shutdown"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_failing_worker_does_not_stop_the_http_server() {
    if !available("python3") {
        return;
    }
    let project = tempfile::tempdir().expect("project");
    let marker = project.path().join("failed-worker-ran");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main><h1>Still serving</h1></main>",
    );
    write(
        &project.path().join("server/workers/fails.py"),
        &format!(
            "from pathlib import Path\n@Controller\nclass FailsController:\n    @staticmethod\n    def POST(request):\n        Path({marker:?}).write_text('ran')\n        return {{\"status\": 500, \"headers\": {{}}, \"body\": \"failed\"}}\n"
        ),
    );
    write(
        &project.path().join(".tachyonrc"),
        r#"{"workers":{"server/workers/fails.py":{"every_seconds":1}}}"#,
    );
    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(8), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("failing worker ran");
    assert!(get(port, "/").await.contains("Still serving"));
    let _stopped = stop.send(());
    running
        .await
        .expect("server task")
        .expect("graceful shutdown");
}

#[tokio::test]
async fn removed_arbitrary_language_workers_fail_closed() {
    // .tachyonrc no longer turns an arbitrary language into Yon. A stale
    // worker registration is refused before the server starts rather than
    // silently leaving a scheduled task inactive.
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"W\"><h1>W</h1></main>",
    );
    write(
        &project.path().join(".tachyonrc"),
        r#"{"interpreters":{"rb":["ruby"]},"workers":{"server/workers/beat.rb":{"every_seconds":1}}}"#,
    );
    write(
        &project.path().join("server/workers/beat.rb"),
        "puts '{\"status\":200}'\n",
    );

    let failure = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect_err("Ruby is not a Yon worker language");
    let rendered = failure.to_string();
    assert!(rendered.contains("TY1502"), "{rendered}");
    assert!(rendered.contains("interpreters"), "{rendered}");
    assert!(rendered.contains("@Relay"), "{rendered}");
}

#[tokio::test]
async fn topics_stream_as_server_sent_events_with_a_resumable_cursor() {
    // The legacy shape is an append-only NDJSON log per topic read by an
    // integer position. Serving it as server-sent events keeps that contract
    // and lets the browser own reconnection.
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"R\"><h1>R</h1></main>",
    );
    let log = project.path().join(".tachyon/topics/chat.jsonl");
    write(&log, "{\"seq\":1}\n{\"seq\":2}\n");

    let server = DevServer::bind(
        project.path(),
        &DevServerOptions {
            port: 0,
            watch: false,
            ..DevServerOptions::default()
        },
    )
    .await
    .expect("bind");
    let port = server.address().port();
    let (stop, wait) = tokio::sync::oneshot::channel::<()>();
    let running = tokio::spawn(async move {
        server
            .run_until(async {
                let _stopped = wait.await;
            })
            .await
    });

    // A stream is opened, not a finite document, so each read is bounded by a
    // deadline rather than by end of file.
    let read_stream = |raw: String| async move {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        stream.write_all(raw.as_bytes()).await.expect("write");
        let mut seen = Vec::new();
        let mut chunk = [0_u8; 4096];
        let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(600), stream.read(&mut chunk)).await {
                Ok(Ok(read)) if read > 0 => seen.extend_from_slice(&chunk[..read]),
                _ => break,
            }
        }
        String::from_utf8_lossy(&seen).into_owned()
    };

    let from_start = read_stream(String::from(
        "GET /.tachyon/topics/chat HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    ))
    .await;
    assert!(from_start.contains("text/event-stream"), "{from_start}");
    assert!(from_start.contains("id: 0"), "{from_start}");
    assert!(from_start.contains(r#"data: {"seq":1}"#), "{from_start}");
    assert!(from_start.contains(r#"data: {"seq":2}"#), "{from_start}");

    // Appending to the log publishes; a subscriber resuming past record 0 sees
    // only what follows its cursor.
    fs::write(&log, "{\"seq\":1}\n{\"seq\":2}\n{\"seq\":3}\n").expect("publish");
    let resumed = read_stream(String::from(
        "GET /.tachyon/topics/chat HTTP/1.1\r\nHost: 127.0.0.1\r\n\
         Last-Event-ID: 1\r\n\r\n",
    ))
    .await;
    assert!(resumed.contains(r#"data: {"seq":3}"#), "{resumed}");
    assert!(!resumed.contains(r#"data: {"seq":1}"#), "{resumed}");

    // A topic name outside the bounded slug shape is refused.
    let refused = read_stream(String::from(
        "GET /.tachyon/topics/BAD..name HTTP/1.1\r\nHost: 127.0.0.1\r\n\
         Connection: close\r\n\r\n",
    ))
    .await;
    assert!(refused.contains("400"), "{refused}");

    let _stopped = stop.send(());
    let _joined = running.await;
}
