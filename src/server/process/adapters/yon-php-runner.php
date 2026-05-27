<?php

final class YonPhpRunner
{
    public static function resolveHandlerClass(string $handlerPath): string
    {
        if (!class_exists('Handler')) {
            throw new RuntimeException('PHP route must define a class named Handler');
        }
        return 'Handler';
    }

    public static function resolveMethod(string $className, string $method): void
    {
        if (!method_exists($className, $method)) {
            throw new RuntimeException("Handler class does not implement static {$method}()");
        }
    }

    public static function write(mixed $value): void
    {
        if ($value === null) {
            return;
        }

        if (is_string($value)) {
            fwrite(STDOUT, $value);
            return;
        }

        fwrite(STDOUT, json_encode($value, JSON_UNESCAPED_SLASHES));
    }

    public static function run(array $argv): void
    {
        if (!isset($argv[1])) {
            throw new RuntimeException('Missing handler path');
        }

        $input = stream_get_contents(STDIN);
        $request = json_decode($input === '' ? '{}' : $input, true);
        $method = $request['method'] ?? null;
        if ($method === null || $method === '') {
            throw new RuntimeException('Missing HTTP method in request payload');
        }

        $source = file_get_contents($argv[1]);
        if ($source === false) {
            throw new RuntimeException('Unable to read handler file');
        }

        $source = preg_replace('/^#![^\r\n]*(?:\r?\n)?/', '', $source) ?? $source;
        $tempPath = tempnam(sys_get_temp_dir(), 'yon_php_handler_');
        if ($tempPath === false) {
            throw new RuntimeException('Unable to create temporary PHP handler file');
        }

        file_put_contents($tempPath, $source);
        try {
            require $tempPath;
        } finally {
            @unlink($tempPath);
        }

        $className = self::resolveHandlerClass($argv[1]);
        self::resolveMethod($className, $method);
        self::write(call_user_func([$className, $method], $request));
    }
}

try {
    YonPhpRunner::run($argv);
} catch (Throwable $error) {
    fwrite(STDERR, $error->getMessage());
    exit(1);
}
