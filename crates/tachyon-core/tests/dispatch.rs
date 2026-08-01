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
        r"export class Handler {
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
async fn a_project_without_handlers_still_serves_generated_output() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"Only\"><h1>Only</h1></main>",
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

    // Editing a source rebuilds and advances the generation an open page polls.
    let before = get(port, "/.tachyon/live").await;
    write(&source, "<main aria-label=\"W\"><h1>Second</h1></main>");
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
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    let survived = get(port, "/").await;
    assert!(
        survived.contains("Second"),
        "a failed rebuild broke the site"
    );

    let _stopped = stop.send(());
    let _joined = running.await;
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
async fn handlers_in_any_language_serve_routes_without_an_adapter() {
    // The point of the direct protocol: a language Tachyon ships no adapter for
    // serves a route, either through a registered interpreter or by being
    // executable. Nothing language-specific exists in the implementation.
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"P\"><h1>Polyglot</h1></main>",
    );

    let ruby = available("ruby");
    if ruby {
        write(
            &project.path().join(".tachyonrc"),
            r#"{"interpreters":{"rb":["ruby"]}}"#,
        );
        write(
            &project.path().join("server/routes/greet/_name/yon.rb"),
            "require 'json'\n\
             request = JSON.parse($stdin.read)\n\
             name = request.dig('parameters', 'name')\n\
             puts JSON.generate({ status: 200, body: JSON.generate({ hello: name }) })\n",
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let script = project.path().join("server/routes/shell/_word/yon.sh");
        write(
            &script,
            "#!/bin/sh\n\
             word=$(cat | sed -n 's/.*\"word\":\"\\([^\"]*\\)\".*/\\1/p')\n\
             printf '{\"status\":200,\"body\":\"%s\"}' \"$word\"\n",
        );
        let mut permissions = fs::metadata(&script).expect("script").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).expect("executable");
    }

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

    if ruby {
        let greeted = get(port, "/greet/Ada").await;
        assert!(greeted.contains(r#"{"hello":"Ada"}"#), "{greeted}");
    }

    #[cfg(unix)]
    {
        let echoed = get(port, "/shell/hi").await;
        assert!(echoed.contains("hi"), "{echoed}");
    }

    let _stopped = stop.send(());
    let _joined = running.await;
}

#[tokio::test]
async fn an_unrunnable_handler_extension_fails_closed() {
    // Without a registered interpreter and without the executable bit, a
    // handler must be refused with a diagnostic that names both remedies.
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
    assert!(
        text.contains(".tachyonrc") || text.contains("executable"),
        "{text}"
    );
}

#[tokio::test]
async fn middleware_can_refuse_a_request_before_it_reaches_a_route() {
    // Middleware speaks the same protocol a handler does, so it may be written
    // in any language the project can run. 204 means continue; any other
    // status answers the request without reaching the route.
    if !available("ruby") {
        return;
    }
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"M\"><h1>Open</h1></main>",
    );
    write(
        &project.path().join(".tachyonrc"),
        r#"{"interpreters":{"rb":["ruby"]}}"#,
    );
    write(
        &project.path().join("server/routes/private/yon.rb"),
        "require 'json'\n\
         $stdin.read\n\
         puts JSON.generate({ status: 200, body: 'secret data' })\n",
    );
    write(
        &project.path().join("middleware.rb"),
        r"require 'json'
request = JSON.parse($stdin.read)
token = (request.dig('headers', 'authorization') || []).first
if request['route'].start_with?('/private') && token != 'let-me-in'
  puts JSON.generate({ status: 401, body: 'unauthorized' })
else
  puts JSON.generate({ status: 204 })
end
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
    if !available("ruby") {
        return;
    }
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"A\"><h1>After</h1></main>",
    );
    write(
        &project.path().join(".tachyonrc"),
        r#"{"interpreters":{"rb":["ruby"]}}"#,
    );
    write(
        &project.path().join("server/routes/api/yon.rb"),
        "require 'json'\n$stdin.read\nputs JSON.generate({ status: 200, body: '{\"ok\":true}' })\n",
    );
    write(
        &project.path().join("middleware.rb"),
        r"require 'json'
request = JSON.parse($stdin.read)
if request['operation'] == 'middleware.after'
  status = (request.dig('headers', 'x-tachyon-status') || []).first
  puts JSON.generate({ status: 204,
                       headers: { 'x-served-by' => ['tachyon'],
                                  'x-observed-status' => [status.to_s] } })
else
  puts JSON.generate({ status: 204 })
end
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

#[tokio::test]
async fn scheduled_workers_run_on_their_interval() {
    // A worker is a handler invoked on a schedule instead of by a request, so
    // it reuses the same protocol, supervision, and bounds.
    if !available("ruby") {
        return;
    }
    let project = tempfile::tempdir().expect("project");
    let beats = project.path().join("beats.log");
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
        &format!(
            "require 'json'\n\
             request = JSON.parse($stdin.read)\n\
             File.open({:?}, 'a') {{ |f| f.puts(request['operation']) }}\n\
             puts JSON.generate({{ status: 200 }})\n",
            beats.to_string_lossy()
        ),
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

    let mut runs = 0;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        runs = fs::read_to_string(&beats).map_or(0, |log| log.lines().count());
        if runs >= 2 {
            break;
        }
    }
    assert!(runs >= 2, "worker ran {runs} times");
    let log = fs::read_to_string(&beats).expect("worker log");
    assert!(log.contains("worker.run"), "{log}");

    let _stopped = stop.send(());
    let _joined = running.await;
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
