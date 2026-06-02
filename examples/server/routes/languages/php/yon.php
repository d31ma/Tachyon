<?php

require_once getcwd() . '/server/services/php_language_service.php';

class Handler {
    private const RESPONSES = [
        '406' => ['code' => '406', 'detail' => 'not acceptable'],
        '407' => ['code' => '407', 'detail' => 'proxy auth required'],
        '408' => ['code' => '408', 'detail' => 'request timeout'],
        '409' => ['code' => '409', 'detail' => 'conflict'],
        '410' => ['code' => '410', 'detail' => 'gone'],
    ];

    public static function GET($request) {
        $code = self::statusCode($request);
        if (array_key_exists($code, self::RESPONSES)) {
            return self::RESPONSES[$code];
        }

        return self::service()->describe($request);
    }

    private static function statusCode($request): string {
        $query = $request['query'] ?? [];
        $raw = $query['code'] ?? '';
        return is_numeric($raw) ? (string) intval($raw) : (string) $raw;
    }

    private static function service(): PhpLanguageService {
        static $service = null;
        if ($service === null) {
            $service = new PhpLanguageService();
        }
        return $service;
    }
}
