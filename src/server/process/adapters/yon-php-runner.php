<?php

final class YonPhpRunner
{
    public static function routeClassName(string $handlerPath): string
    {
        return explode('.', basename($handlerPath), 2)[0];
    }

    public static function resolveHandler(string $handlerPath): callable
    {
        if (function_exists('handler')) {
            return 'handler';
        }

        $className = self::routeClassName($handlerPath);
        if (class_exists($className)) {
            $instance = new $className();
            if (method_exists($instance, 'handler')) {
                return [$instance, 'handler'];
            }
        }

        throw new RuntimeException('PHP route must define handler($request) or a method-named class with handler($request)');
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
        self::write(call_user_func(self::resolveHandler($argv[1]), $request));
    }
}

try {
    YonPhpRunner::run($argv);
} catch (Throwable $error) {
    fwrite(STDERR, $error->getMessage());
    exit(1);
}
