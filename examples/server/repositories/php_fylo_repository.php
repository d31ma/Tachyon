<?php

final class PhpFyloRepository
{
    private string $root;
    private ?string $executable;

    public function __construct(?string $root = null)
    {
        $this->root = $root ?? getenv('FYLO_ROOT') ?: getcwd() . '/server/data/language-route-events';
        $path = getenv('FYLO_EXEC_PATH');
        $this->executable = $path === false || $path === '' ? null : $path;
    }

    /** @param array<string, mixed> $request */
    public function machine(array $request): mixed
    {
        $descriptor = [
            0 => ['pipe', 'r'],
            1 => ['pipe', 'w'],
            2 => ['pipe', 'w'],
        ];
        $command = $this->executable === null
            ? ['bunx', '--bun', 'fylo.exec', 'exec']
            : [$this->executable, 'exec'];
        $process = proc_open([...$command, '--request', '-', '--root', $this->root], $descriptor, $pipes);
        if (!is_resource($process)) {
            throw new RuntimeException('Unable to start fylo.exec');
        }
        fwrite($pipes[0], json_encode($request, JSON_UNESCAPED_SLASHES));
        fclose($pipes[0]);
        $stdout = stream_get_contents($pipes[1]);
        fclose($pipes[1]);
        $stderr = stream_get_contents($pipes[2]);
        fclose($pipes[2]);
        $code = proc_close($process);
        if ($code !== 0) {
            throw new RuntimeException($stderr !== '' ? $stderr : $stdout);
        }
        $response = json_decode($stdout === '' ? '{}' : $stdout, true);
        if (!is_array($response) || !($response['ok'] ?? false)) {
            $error = is_array($response) ? ($response['error']['message'] ?? 'fylo.exec returned an error') : 'Invalid fylo.exec response';
            throw new RuntimeException($error);
        }
        return $response['result'] ?? null;
    }

    /** @return array<string, mixed> */
    public function writeSample(string $language, string $requestId): array
    {
        $collection = 'language-route-events';
        $this->machine(['op' => 'createCollection', 'collection' => $collection]);
        $document = [
            'language' => $language,
            'source' => 'fylo.exec',
            'requestId' => $requestId,
        ];
        $id = $this->machine(['op' => 'putData', 'collection' => $collection, 'data' => $document]);
        $found = $this->machine([
            'op' => 'findDocs',
            'collection' => $collection,
            'query' => ['$ops' => [['language' => ['$eq' => $language]]]],
        ]);
        return [
            'collection' => $collection,
            'id' => (string) $id,
            'document' => $document,
            'matched' => is_array($found) ? count($found) : 0,
            'operations' => ['createCollection', 'putData', 'findDocs'],
            'resultCount' => 3,
        ];
    }
}
