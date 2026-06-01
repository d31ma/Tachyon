<?php

require_once getcwd() . '/server/repositories/php_fylo_repository.php';

final class PhpLanguageService
{
    public function describe(array $request): array
    {
        $context = $request['context'] ?? [];
        $requestId = $context['requestId'] ?? 'unknown';
        return [
            'language' => 'php',
            'message' => 'Hello from PHP!',
            'requestId' => $requestId,
            'fylo' => $this->repository()->batchSample('php', $requestId),
        ];
    }

    private function repository(): PhpFyloRepository
    {
        static $repository = null;
        if ($repository === null) {
            $repository = new PhpFyloRepository();
        }
        return $repository;
    }
}
