<?php
// No shebang: Yon knows how to start every language it runs, and appends the
// protocol runtime after this file. A handler declares its methods and nothing
// about its own execution.
#[Controller]
class ChecksumController
{
    public static function GET(YonRequest $request): YonResponse
    {
        return YonResponse::json(json_encode([
            'language' => 'PHP',
            'version' => PHP_VERSION,
            'route' => $request->route(),
        ]));
    }
}
