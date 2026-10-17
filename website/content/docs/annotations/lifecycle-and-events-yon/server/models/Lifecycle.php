<?php
class Lifecycle {
    public function __construct(
        /** @var list<string> */
        public array $log,
        public int $settled,
        public int $audited,
    ) {}
}
