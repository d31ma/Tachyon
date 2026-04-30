<?php

require_once getcwd() . '/server/services/php_language_service.php';

function handler($request) {
    return (new PhpLanguageService())->describe($request);
}
