# ADR 0060: Sockets Are a Route and a Request

- Status: Accepted
- Date: 2026-09-26
- Extends: [ADR 0013](0013-semantic-hot-updates.md) (server-sent events stay
  the push channel for hot updates; WebSockets are added for application
  messages that go both ways) and
  [ADR 0058](0058-streams-are-a-route-and-a-request.md) (the same route and
  request pair, now for a two-way connection)
- Preserves: [ADR 0055](0055-annotation-arguments-use-types.md) (typed
  events) and [ADR 0056](0056-every-annotation-checks-its-target.md) (the rule
  table)

## Context

`@StreamRoute` and `@StreamRequest` send messages from the server to the
browser. A chat, a shared editor or a game also sends messages from the
browser to the server on the same connection. Jakarta WebSocket gives this a
small, well-known shape: an `@ServerEndpoint` class with `@OnOpen`,
`@OnMessage`, `@OnClose` and `@OnError` methods, and one endpoint object for
each connection.

## Decision

### SocketRoute (Yon)

- `@SocketRoute(path?)` marks a class in `server/controllers/`, like
  `@RequestRoute`. The class name ends with `Controller`. The path has no
  `{variables}`.
- The class has one `@OnMessage` method and may have one `@OnOpen`, one
  `@OnClose` and one `@OnError` method. Method names are free, as in Jakarta.
  - `@OnOpen` method: `(session)`, or no parameter.
  - `@OnMessage` method: `(session, message)`. The message parameter has a
    model type (JSDoc `@param {ChatMessage} message` or
    `message: ChatMessage`), and Tachyon checks each message against it.
  - `@OnClose` method: `(session)`, or no parameter.
  - `@OnError` method: `(session, error)`, `(session)`, or no parameter.
- One object for each connection, as in Jakarta: Tachyon starts one handler
  process when the browser connects and keeps it until the connection ends.
  Fields of the class keep state for that one connection. `@Needs` fields are
  built for each connection.
- The `session` has `id`, `send(message)`, `close()` and `openSessions()`.
  `openSessions()` is Jakarta's `getOpenSessions()`: it returns every open
  session of the same path (with `id` and `send`), so one message can go to
  every connected browser.
- Messages are text frames that hold JSON. `@OnClose` always runs last. When
  `@OnOpen` or `@OnMessage` fails, `@OnError` runs and the connection stays
  open, as in Jakarta: the next message goes to the same object. The error
  text goes to the server log only, never to the browser.
- Guards (fail closed):
  - A message that is not JSON, or does not fit the model, closes the
    connection with code 1007. A binary frame closes it with code 1003.
  - A message larger than 64 KiB closes it with code 1009.
  - At most 32 connections are open at once for the whole server; a new one
    gets `503`.
  - Five minutes without a message in either direction closes the connection
    with code 1001. A connection lasts at most one hour.
  - The `Origin` header must be the page's own origin; a different origin
    gets `403`. A request with no `Origin` (not a browser) is allowed.
  - A plain HTTP request to the path gets `426 Upgrade Required`.
- The rule table puts `@SocketRoute` on a class, the four `On…` markers on
  methods inside a `@SocketRoute` class, and refuses `@SocketRoute` together
  with `@RequestRoute`. Route markers such as `@GetRoute` need
  `@RequestRoute`, so they are refused in a socket class.
- A socket keeps one handler running for the whole connection and reads each
  message while it runs. The runner or prelude of every Yon language reads
  the socket frames below, so `@SocketRoute` is written in any Yon language
  (see "Socket languages").
- `/api.json` marks the path with `"socket": true` and describes the message
  model.

### Handler protocol frames

The request frame has `operation: "socket.open"`. After it, the server writes
`message` frames (one JSON text each), a `sessions` frame (the answer to
`openSessions()`) and a `close` frame on stdin. The handler writes `event`
frames (a message for this browser, or for another session with `to`),
`sessions` frames (a question) and `close` frames on stdout. The frames use
the same four-byte length prefix as every Handler Protocol v1 frame.

### SocketRequest (Tac)

- `@SocketRequest(path?)` goes on a method of an `@HTTPClient` class, like
  `@StreamRequest`. The method takes one parameter whose model type says what
  each incoming message holds. Tac checks each message against the model,
  passes it to the method and publishes it as that model's typed event, so
  `@OnEvent` listeners in any Tac language receive it.
- Sending uses Spring's `WebSocketSession.sendMessage` shape: the client
  object gets a generated `send(message)` method. It opens the socket when it
  is not open yet and sends the value as JSON.
- The browser event bus owns the `WebSocket`: it opens when the client is
  first used, closes when its owner goes away, and reconnects after 1, 2, 4, 8
  and 16 seconds and then stops. A message resets the count.
- `@SocketRequest` is written in JavaScript or TypeScript; the other Tac
  languages receive the messages with `@OnEvent`. TypeScript declares the
  generated method with `declare send: SocketSend<ChatMessage>`; a `declare`
  field only names a type, so `@NeedsAll` does not require it.

### Socket languages

Each language keeps the same frames, limits and one object for each
connection. The session methods use the language's own naming:

| Language | Marker | Session |
| --- | --- | --- |
| JavaScript, TypeScript | `@SocketRoute` | `id`, `send`, `close`, `openSessions()` (promises) |
| Python | `@socket_route` | `id`, `send`, `close`, `open_sessions()` |
| Go | `// @SocketRoute` | `ID()`, `Send`, `Close`, `OpenSessions()`, `Request()` on `*SocketSession` |
| Java | `@SocketRoute` | `getId()`, `send`, `close`, `getOpenSessions()` (Jakarta names) |
| Kotlin | `@SocketRoute` | `id`, `send`, `close`, `openSessions()` |
| C# | `[SocketRoute]` | `Id`, `Send`, `Close`, `OpenSessions()`; a method may return `Task` |
| PHP | `#[SocketRoute]` | `$session->id`, `send`, `close`, `openSessions()` |
| Ruby | `#[SocketRoute]` | `id`, `send`, `close`, `open_sessions`; the message model comes from YARD `@param` |
| Rust | `#[SocketRoute]` | `id()`, `send`, `close`, `open_sessions()` on `&SocketSession`; fields keep state with `Cell` or a lock, because the object is shared as `Arc` |
| Dart | `@SocketRoute` | `id`, `send`, `close`, `openSessions()` (futures); `@OnError` takes `Object error` |

The generated handler calls the marked methods with the parameters they
declare, and turns each message into the model type first (for example a
Python dataclass or a Go struct). An `async def` method in Python runs on one
event loop for the connection. A method that returns an error (Go) or throws
counts as a failure and runs `@OnError`.

The compiled and prelude languages normally read the whole request and answer
once. For a socket, the supervisor sets `YON_SOCKET_FRAMES=1` (a private name
that an allowlist cannot pass on); the prelude then reads the request as a
framed Handler Protocol frame and keeps reading frames from stdin.

## Consequences

Two-way messages need no hand-written `WebSocket` code. Each connection costs
one handler process, so the session limit is small and fixed. Every Yon
language reads the same `message` frames, so a socket class can be written in
the language of the rest of the server.

## Amendment (2026-09-29)

The `@SocketRequest` method belongs to an `HTTPClient` contract (ADR 0048
amendment): a class with empty method bodies in JavaScript and TypeScript.
`declare send: SocketSend<Message>` stays allowed, because it declares a type
and holds no state.

## Amendment (2026-10-11)

The Go session has `Request()`. It returns the request that opened the
socket as a `*http.Request`: its URL with the query string, its headers and
the client address. The supervisor now passes the opening request's target,
so the query string reaches every language's socket request.

## Amendment (2026-10-11): Go socket clients

`// @SocketRequest` is also written on a Go HTTPClient interface method. The
page's event bus holds the `WebSocket` for the Go service that keeps the
client in a field; the Go service receives each message with `// @OnEvent`,
and the generated `Send(message any) error` writes to the socket and returns
an error code instead of a rejected promise. See ADR 0090, "Go streams and
sockets". The other compiled Tac languages still refuse `@SocketRequest`.

## Amendment (2026-10-17): Swift socket clients

`@SocketRequest` is also written on a Swift HTTPClient protocol method. The
page holds the `WebSocket` for the Swift web service that keeps the client in
a property, and the generated `send(_ message:) async throws` writes to it.
See ADR 0090, "Swift streams and sockets".
