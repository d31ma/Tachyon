<?php

require_once getcwd() . '/server/services/php_language_service.php';

class Handler {
    public static function GET($request) {
        $responses = [
            '406' => ['code' => '406', 'detail' => 'not acceptable'],
            '407' => ['code' => '407', 'detail' => 'proxy auth required'],
            '408' => ['code' => '408', 'detail' => 'request timeout'],
            '409' => ['code' => '409', 'detail' => 'conflict'],
            '410' => ['code' => '410', 'detail' => 'gone'],
        ];
        $query = $request['query'] ?? [];
        $raw = $query['code'] ?? '';
        $code = is_numeric($raw) ? (string) intval($raw) : (string) $raw;
        if (array_key_exists($code, $responses)) {
            return $responses[$code];
        }

        return (new PhpLanguageService())->describe($request);
    }
}
