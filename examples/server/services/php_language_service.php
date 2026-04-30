<?php

final class PhpLanguageService
{
    public function describe(array $request): array
    {
        $context = $request['context'] ?? [];
        return [
            'language' => 'php',
            'message' => 'Hello from PHP!',
            'requestId' => $context['requestId'] ?? 'unknown',
        ];
    }
}
