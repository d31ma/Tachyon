class ApplicationEventPublisher {
    function publishEvent($value, $retain = false) {
        if (!is_object($value) || $value instanceof stdClass || $value instanceof UnitEnum) {
            fwrite(STDERR, "Yon local event publication failed.\n");
            return $value;
        }
        __YonEvents::publish((new ReflectionClass($value))->getShortName(), $value, $retain === true);
        return $value;
    }
}
class __YonEvents {
    private static $active = null;
    private $queue = [];
    private $retained = [];
    private $listeners = [];
    public $destroyers = [];
    private $closed = false;
    private $deliveries = 0;
    private $topics = [];
    private $outbound = [];
    private $only = null;
    private $delivered = null;
    function __construct($topics = []) {
        if (self::$active !== null) throw new RuntimeException('Nested local event invocation');
        $this->topics = array_fill_keys($topics, true);
        self::$active = $this;
    }
    static function delivery($body, $topics) {
        $events = new __YonEvents($topics);
        $delivery = self::decode($body, 64);
        $events->only = array_fill_keys(array_map('strval', $delivery['listeners']), true);
        $events->delivered = [(string) $delivery['topic'], json_encode($delivery['payload'], JSON_THROW_ON_ERROR)];
        return $events;
    }
    static function decode($encoded, $depth) {
        return self::plain(json_decode($encoded, false, $depth, JSON_THROW_ON_ERROR));
    }
    private static function plain($value) {
        if (is_array($value)) return array_map([self::class, 'plain'], $value);
        if (!$value instanceof stdClass || get_object_vars($value) === []) return $value;
        return array_map([self::class, 'plain'], get_object_vars($value));
    }
    function receiveDelivery() {
        if ($this->delivered !== null) $this->queue[] = $this->delivered;
    }
    static function publish($name, $value, $retain) {
        try {
        $scope = self::$active;
        if ($scope === null || $scope->closed) throw new RuntimeException('Local event owner is disposed');
        $encoded = json_encode($value, JSON_THROW_ON_ERROR, 32);
        if (strlen($encoded) > 65536 || count($scope->queue) >= 128) throw new RuntimeException('Local event capacity exceeded');
        if ($retain) $scope->retained[$name] = $encoded;
        $scope->queue[] = [$name, $encoded];
        if (isset($scope->topics[$name])) $scope->outbound[] = [$name, $encoded];
        } catch (Throwable $failure) { fwrite(STDERR, "Yon local event publication failed.\n"); }
        return $value;
    }
    static function fits($models, $shape, $value) {
        if (str_starts_with($shape, '?')) return $value === null || self::fits($models, substr($shape, 1), $value);
        if (str_starts_with($shape, '[]')) {
            if (!is_array($value) || !array_is_list($value)) return false;
            foreach ($value as $item) if (!self::fits($models, substr($shape, 2), $item)) return false;
            return true;
        }
        if ($shape === 'any') return true;
        if ($shape === 'string') return is_string($value);
        if ($shape === 'integer') return is_int($value);
        if ($shape === 'number') return is_int($value) || is_float($value);
        if ($shape === 'boolean') return is_bool($value);
        if ($value instanceof stdClass) $value = [];
        $model = $models[$shape] ?? null;
        if ($model === null || !is_array($value) || ($value !== [] && array_is_list($value))) return false;
        $fields = $model['fields'];
        if (!$model['open']) foreach (array_keys($value) as $key) if (!array_key_exists($key, $fields)) return false;
        foreach ($fields as $name => $field) {
            $required = str_starts_with($field, '!');
            if (!array_key_exists($name, $value)) { if ($required) return false; continue; }
            if (!self::fits($models, $required ? substr($field, 1) : $field, $value[$name])) return false;
        }
        return true;
    }
    static function matches($matcher, $value, $event) {
        try { return self::test($matcher, $value, $event); } catch (Throwable $failure) { return false; }
    }
    private static function operand($node, $value, $event) {
        if ($node['k'] === 'lit') return $node['v'] ?? null;
        if ($node['k'] === 'event') return $event;
        $current = $value;
        foreach ($node['path'] ?? [] as $segment) {
            if (!is_array($current) || !array_key_exists($segment, $current)) return null;
            $current = $current[$segment];
        }
        return $current;
    }
    private static function truthy($value) {
        if ($value === null || $value === false || $value === 0 || $value === 0.0 || $value === '') return false;
        return !(is_float($value) && is_nan($value));
    }
    private static function test($node, $value, $event) {
        switch ($node['k']) {
            case 'and': return self::test($node['l'], $value, $event) && self::test($node['r'], $value, $event);
            case 'or': return self::test($node['l'], $value, $event) || self::test($node['r'], $value, $event);
            case 'not': return !self::test($node['e'], $value, $event);
            case 'truthy': return self::truthy(self::operand($node['e'], $value, $event));
            case 'cmp':
                $left = self::operand($node['l'], $value, $event);
                $right = self::operand($node['r'], $value, $event);
                $numbers = (is_int($left) || is_float($left)) && (is_int($right) || is_float($right));
                $strings = is_string($left) && is_string($right);
                if ($node['op'] === 'eq' || $node['op'] === 'ne') {
                    $equal = $numbers ? $left == $right : (($strings || is_bool($left) || $left === null) && $left === $right);
                    return ($node['op'] === 'eq') === $equal;
                }
                if (!$numbers && !$strings) return false;
                $order = $strings ? strcmp($left, $right) : $left <=> $right;
                if ($node['op'] === 'lt') return $order < 0;
                if ($node['op'] === 'le') return $order <= 0;
                if ($node['op'] === 'gt') return $order > 0;
                return $order >= 0;
        }
        return false;
    }
    function listen($options, $callback, $replay, $key) {
        if ($this->only !== null && !isset($this->only[$key])) return;
        $options = json_decode($options, true, 32, JSON_THROW_ON_ERROR);
        foreach ($options['events'] as $name) {
            if (count($this->listeners) >= 1024) throw new RuntimeException('Local listener capacity exceeded');
            $this->listeners[] = ['name' => $name, 'callback' => $callback, 'order' => (int) ($options['order'] ?? 0),
                                  'condition' => $options['condition'] ?? null, 'models' => $options['models'] ?? null,
                                  'returns' => $options['returns'] ?? null, 'background' => ($options['async'] ?? false) === true,
                                  'sequence' => count($this->listeners)];
            if ($replay && array_key_exists($name, $this->retained)) $this->queue[] = [$name, $this->retained[$name]];
        }
        usort($this->listeners, fn($left, $right) => [$left['order'], $left['sequence']] <=> [$right['order'], $right['sequence']]);
    }
    private static function republish($result, $returns) {
        if ($returns === null) return;
        $items = is_array($result) && array_is_list($result) ? array_slice($result, 0, 32) : [$result];
        foreach ($items as $item) {
            if ($item !== null) self::publish($returns, $item, false);
        }
    }
    function drain() {
        while (count($this->queue)) {
            [$name, $encoded] = array_shift($this->queue);
            if (++$this->deliveries > 4096) { $this->queue = []; fwrite(STDERR, "Yon local event delivery bound exceeded.\n"); return; }
            $deferred = [];
            foreach ($this->listeners as $listener) if ($listener['name'] === $name) {
                $value = self::decode($encoded, 32);
                if ($listener['models'] !== null && !self::fits($listener['models'], $name, $value)) { fwrite(STDERR, "Yon local event payload does not fit the listener model.\n"); continue; }
                if ($listener['condition'] !== null && !self::matches($listener['condition'], $value, $name)) continue;
                if ($listener['background']) { $deferred[] = [$listener, $value]; continue; }
                try { self::republish(($listener['callback'])($value), $listener['returns']); }
                catch (Throwable $failure) { fwrite(STDERR, "Yon local event listener failed.\n"); }
            }
            foreach ($deferred as [$listener, $value]) {
                try { ($listener['callback'])($value); }
                catch (Throwable $failure) { fwrite(STDERR, "Yon local event listener failed.\n"); }
            }
        }
        $this->sendTopics();
    }
    private function sendTopics() {
        foreach ($this->outbound as [$name, $encoded]) {
            try { YonManagedResources::call(['operation' => 'topic', 'topic' => $name, 'payload_json' => $encoded]); }
            catch (Throwable $failure) { fwrite(STDERR, "Yon topic publication failed.\n"); }
        }
        $this->outbound = [];
    }
    function close() {
        if ($this->closed) return;
        $this->closed = true; $this->queue = []; $this->listeners = [];
        foreach (array_reverse($this->destroyers) as $destroy) {
            try { $destroy(); } catch (Throwable $failure) { fwrite(STDERR, "Yon local lifecycle cleanup failed.\n"); }
        }
        $this->retained = []; self::$active = null;
    }
}
