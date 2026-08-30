<?php
#[Controller]
class VersionsController
{
    public static function GET(YonRequest $request): YonResponse
    {
        return YonResponse::json(json_encode([
            'language' => 'PHP',
            'version' => PHP_VERSION,
            'tachyon' => '26.31.06',
        ]));
    }
}
