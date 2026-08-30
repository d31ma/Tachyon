<?php
// Appended after a `yon.php` handler. Everything below is the protocol.
//
// PHP strips a shebang only from the entry script, and the handler is the
// entry script, so the runtime cannot be loaded from the top of the file — it
// is appended after it with `-d auto_append_file`, by which time the class is
// defined. That is a PHP fact rather than a Tachyon choice.
//
// The author writes a class and methods. Reading standard input, dispatching
// on the method and writing the response envelope is Tachyon's half.

/**
 * Marks a method as a proxy for a program Yon does not run.
 *
 * Declared, unlike the five layer stereotypes: PHP resolves an attribute
 * lazily, so an undeclared `#[Controller]` costs nothing — but this one is
 * read back through reflection, and reflection is where an undeclared
 * attribute finally becomes an error. PHP is one of the four languages here
 * whose annotations cannot do the work themselves, so the dispatch checks for
 * it and the annotated body is never called.
 */
#[Attribute(Attribute::TARGET_METHOD)]
final class Relay
{
    /** @var list<string> */
    public array $command;

    public function __construct(string ...$command)
    {
        $this->command = $command;
    }
}

/** One request, as the protocol delivers it. */
final class YonRequest
{
    public function __construct(private array $raw) {}

    /** The HTTP method, upper case. */
    public function method(): string
    {
        return (string) ($this->raw['method'] ?? '');
    }

    /** The route as matched, with its parameters still in it. */
    public function route(): string
    {
        return (string) ($this->raw['route'] ?? '');
    }

    /** One bound route parameter, or an empty string when there is none. */
    public function parameter(string $name): string
    {
        return (string) ($this->raw['parameters'][$name] ?? '');
    }

    /** The request body as text, empty when none was sent. */
    public function body(): string
    {
        return (string) ($this->raw['body']['data'] ?? '');
    }

    /** The whole request, for anything the accessors above do not cover. */
    public function raw(): array
    {
        return $this->raw;
    }
}

/** One response, built the way the other layers build their return values. */
final class YonResponse
{
    private function __construct(
        private int $status,
        private array $headers,
        private string $body,
    ) {}

    /** A JSON body with a 200. The common case, so it is the short one. */
    public static function json(string $body): self
    {
        return new self(200, ['content-type' => ['application/json']], $body);
    }

    /** A response with no body, for a 204 or a redirect. */
    public static function empty(int $status): self
    {
        return new self($status, [], '');
    }

    public function status(int $status): self
    {
        $this->status = $status;
        return $this;
    }

    /**
     * Replaces every header at once, for a delegate answering with its own.
     * Without this the assumed content type would survive alongside the one
     * the delegate actually set.
     *
     * @param array<string, list<string>> $headers
     */
    public function headers(array $headers): self
    {
        $this->headers = $headers;
        return $this;
    }

    public function header(string $name, string $value): self
    {
        // A header value is a list, because one header may repeat.
        $this->headers[$name][] = $value;
        return $this;
    }

    public function envelope(): string
    {
        return json_encode([
            'status' => $this->status,
            'headers' => (object) $this->headers,
            'body' => $this->body,
        ]);
    }
}

/** Work handed to a language Yon does not run. */
final class Yon
{
    private const MAX_RELAY_STDOUT_BYTES = 16 * 1024 * 1024;
    private const MAX_RELAY_STDERR_BYTES = 64 * 1024;

    /**
     * Runs a handler written in a language Yon does not run.
     *
     * Yon runs the eight languages that can declare a layer. Go, Ruby, Elixir
     * and the rest cannot, so they are not routes — but they are still
     * programs, and a program that speaks Handler Protocol v1 on standard
     * input and output is exactly what Yon spawns anyway.
     *
     * The command is explicit rather than inferred from the file name: a
     * compiled language has no interpreter to infer. The working directory is
     * the project root, so a project-relative path reads as written.
     *
     * @param list<string> $command
     */
    public static function relay(array $command, YonRequest $request): YonResponse
    {
        if ($command === []) {
            return self::failed('A delegate command cannot be empty.');
        }
        $windows = PHP_OS_FAMILY === 'Windows';
        // PHP cannot select or make proc_open pipes non-blocking on Windows.
        // Sending sideband output to the OS sink leaves one bounded response
        // pipe to read and cannot expose delegate diagnostics to the client.
        $descriptors = [
            0 => ['pipe', 'r'],
            1 => ['pipe', 'w'],
            2 => $windows ? ['file', 'NUL', 'a'] : ['pipe', 'w'],
        ];
        // The array form, so nothing is passed through a shell: a route
        // parameter that reached a command line would be an injection.
        $process = @proc_open($command, $descriptors, $pipes);
        if (!is_resource($process)) {
            return self::failed('Delegate could not be started.');
        }
        $payload = json_encode($request->raw()) ?: '{}';
        $written = 0;
        $stdout = '';
        $stderrBytes = 0;
        $stdoutOverflow = false;
        $timedOut = false;

        if ($windows) {
            // The Rust supervisor owns the deadline and the Windows process
            // group. With stderr detached, writing the bounded request before
            // reading bounded stdout cannot form a cross-pipe deadlock.
            while ($written < strlen($payload)) {
                $count = @fwrite($pipes[0], substr($payload, $written, 8192));
                if ($count === false || $count === 0) break;
                $written += $count;
            }
            fclose($pipes[0]);
            $pipes[0] = null;
            $captured = @stream_get_contents($pipes[1], self::MAX_RELAY_STDOUT_BYTES + 1);
            if ($captured !== false) $stdout = $captured;
            $stdoutOverflow = strlen($stdout) > self::MAX_RELAY_STDOUT_BYTES;
            if ($stdoutOverflow) $stdout = substr($stdout, 0, self::MAX_RELAY_STDOUT_BYTES);
            if ($stdoutOverflow) @proc_terminate($process, 9);
        } else {
            foreach ($pipes as $pipe) {
                stream_set_blocking($pipe, false);
            }
            $deadlineMs = max(1, min(300000, (int) ($request->raw()['deadline_ms'] ?? 30000)));
            $deadline = microtime(true) + ($deadlineMs / 1000);

            // All three pipes are serviced together. Sequential read-to-end can
            // deadlock when the delegate fills stderr while the shim reads stdout.
            while (true) {
                $state = proc_get_status($process);
                if (microtime(true) >= $deadline) {
                    $timedOut = true;
                    break;
                }
                $read = [];
                if (is_resource($pipes[1]) && !feof($pipes[1])) $read[] = $pipes[1];
                if (is_resource($pipes[2]) && !feof($pipes[2])) $read[] = $pipes[2];
                $write = [];
                if (is_resource($pipes[0]) && $written < strlen($payload)) $write[] = $pipes[0];
                if (!$state['running'] && $read === [] && $write === []) break;
                if ($read === [] && $write === []) {
                    usleep(1000);
                    continue;
                }
                $except = [];
                $selected = @stream_select($read, $write, $except, 0, 100000);
                if ($selected === false) {
                    $timedOut = true;
                    break;
                }
                foreach ($write as $pipe) {
                    $count = @fwrite($pipe, substr($payload, $written, 8192));
                    if ($count === false) {
                        fclose($pipes[0]);
                        $pipes[0] = null;
                        break;
                    }
                    $written += $count;
                    if ($written >= strlen($payload)) {
                        fclose($pipes[0]);
                        $pipes[0] = null;
                    }
                }
                foreach ($read as $pipe) {
                    $chunk = @fread($pipe, 8192);
                    if ($chunk === false || $chunk === '') continue;
                    if ($pipe === $pipes[1]) {
                        $remaining = self::MAX_RELAY_STDOUT_BYTES - strlen($stdout);
                        if (strlen($chunk) > $remaining) $stdoutOverflow = true;
                        if ($remaining > 0) $stdout .= substr($chunk, 0, $remaining);
                    } else {
                        // Stderr is drained but never reflected. Keep only a
                        // bounded count so a hostile delegate cannot grow memory.
                        $stderrBytes = min(
                            self::MAX_RELAY_STDERR_BYTES,
                            $stderrBytes + strlen($chunk)
                        );
                    }
                }
            }
        }
        if ($timedOut) @proc_terminate($process, 9);
        foreach ($pipes as $pipe) {
            if (is_resource($pipe)) fclose($pipe);
        }
        $status = @proc_close($process);
        if ($timedOut || $stdoutOverflow || $status !== 0) {
            return self::failed('Delegate invocation failed.');
        }
        $envelope = json_decode($stdout, true);
        if (!is_array($envelope)) {
            return self::failed('Delegate returned an invalid response.');
        }
        $response = YonResponse::json((string) ($envelope['body'] ?? ''))
            ->status((int) ($envelope['status'] ?? 200));
        // The headers the delegate set replace the assumed content type,
        // because a delegate that answered with a header meant that header.
        if (($envelope['headers'] ?? []) !== []) {
            $response = $response->headers([]);
        }
        foreach ($envelope['headers'] ?? [] as $name => $values) {
            foreach (is_array($values) ? $values : [$values] as $value) {
                $response = $response->header((string) $name, (string) $value);
            }
        }
        return $response;
    }

    /**
     * A delegate that could not be run answers 502, the same as any other
     * upstream that did not reply. Reasons are deliberately generic: process
     * errors and delegate stderr are diagnostics, never client response data.
     */
    private static function failed(string $reason): YonResponse
    {
        return YonResponse::json(json_encode(['error' => $reason]))->status(502);
    }
}

/**
 * One streamed frame on standard output.
 *
 * A stream is length-prefixed where a single response is not: the reader has
 * to know where each frame ends, because more follow. `[4-byte big-endian
 * length][UTF-8 JSON]` is the shape the two adapters already write, and the
 * reader that consumes it never asks which language wrote it.
 */
final class YonStream
{
    /** The reader refuses a larger frame, so refusing it here says why. */
    private const MAX_FRAME_BYTES = 16 * 1024 * 1024;

    public static function event(string $requestId, mixed $value): bool
    {
        // The flags the other two adapters get for free. PHP escapes a forward
        // slash and every non-ASCII character by default, so without them the
        // same event was `{"route":"\/ticks"}` here and `{"route":"/ticks"}`
        // in JavaScript — one value, two payloads.
        $data = json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        if ($data === false) {
            return self::failure($requestId, 'Streamed event is not JSON-encodable.');
        }
        return self::frame($requestId, [
            'protocol_version' => 1,
            'kind' => 'event',
            'request_id' => $requestId,
            'body' => ['encoding' => 'utf8', 'data' => $data],
        ]);
    }

    /**
     * An error frame ends the stream. The reader turns it into a failure
     * rather than presenting it as data, which is the difference between a
     * handler that stopped and one that had nothing more to say.
     */
    public static function failure(string $requestId, string $message): bool
    {
        self::frame($requestId, [
            'protocol_version' => 1,
            'kind' => 'response',
            'request_id' => $requestId,
            'status' => 500,
            'headers' => (object) [],
            'error' => ['code' => 'TY2204', 'message' => $message, 'retryable' => false],
        ]);
        return false;
    }

    private static function frame(string $requestId, array $frame): bool
    {
        $payload = json_encode($frame);
        if ($payload === false || strlen($payload) > self::MAX_FRAME_BYTES) {
            $payload = json_encode([
                'protocol_version' => 1,
                'kind' => 'response',
                'request_id' => $requestId,
                'status' => 500,
                'headers' => (object) [],
                'error' => [
                    'code' => 'TY2203',
                    'message' => 'Streamed event exceeds the protocol frame limit.',
                    'retryable' => false,
                ],
            ]);
            fwrite(STDOUT, pack('N', strlen($payload)) . $payload);
            fflush(STDOUT);
            return false;
        }
        fwrite(STDOUT, pack('N', strlen($payload)) . $payload);
        // Flushed per frame: a stream whose events arrive together at the end
        // is a slow response wearing a stream's clothes.
        fflush(STDOUT);
        return true;
    }
}

// An undeclared method answers 405 without being written.
$yon_request = new YonRequest(json_decode(stream_get_contents(STDIN) ?: '{}', true) ?? []);
$yon_method = $yon_request->method();
if (!method_exists('__YON_CONTROLLER__', $yon_method)) {
    $yon_response = YonResponse::empty(405);
} else {
    $yon_response = __YON_CONTROLLER__::{$yon_method}($yon_request);
}

// A method that yields returns a Generator, and a Generator is a stream: each
// value becomes one frame and end of stream is end of process, because the
// reader takes EOF as the close.
//
// `#[Stream]` on the method is what told the server to read frames at all, and
// Tachyon refuses the two to disagree — so a Generator here means the other end
// is already reading them.
if ($yon_response instanceof Generator) {
    $yon_id = (string) ($yon_request->raw()['request_id'] ?? '');
    try {
        foreach ($yon_response as $yon_event) {
            if (!YonStream::event($yon_id, $yon_event)) {
                exit(0);
            }
        }
    } catch (Throwable $yon_failed) {
        YonStream::failure($yon_id, $yon_failed->getMessage());
    }
    exit(0);
}
echo $yon_response->envelope();
