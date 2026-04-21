// In a standalone app, change this import to:
// import type { MiddlewareModule } from '@d31ma/tachyon'
import type { MiddlewareModule } from "../src/server/route-handler.js"

type UpstashResponse = Array<{
    result?: unknown
    error?: string
}>

const UPSTASH_URL = process.env.UPSTASH_REDIS_REST_URL?.replace(/\/+$/, '')
const UPSTASH_TOKEN = process.env.UPSTASH_REDIS_REST_TOKEN
const RATE_LIMIT_PREFIX = process.env.UPSTASH_RATE_LIMIT_PREFIX || 'tachyon:rate-limit'

// Redis Lua keeps the increment and TTL setup atomic across every app instance.
const RATE_LIMIT_SCRIPT = `
local current = redis.call("INCR", KEYS[1])
if current == 1 then
  redis.call("PEXPIRE", KEYS[1], ARGV[2])
end
local ttl = redis.call("PTTL", KEYS[1])
if ttl < 0 then
  ttl = tonumber(ARGV[2])
  redis.call("PEXPIRE", KEYS[1], ttl)
end
local remaining = tonumber(ARGV[1]) - current
return { current, remaining, ttl }
`.trim()

let warnedMissingConfig = false

function getConfiguredLimit() {
    return Number(process.env.RATE_LIMIT_MAX || 0)
}

function getConfiguredWindowMs() {
    return Number(process.env.RATE_LIMIT_WINDOW_MS || 0)
}

async function takeSharedRateLimit(key: string, limit: number, windowMs: number) {
    if (!UPSTASH_URL || !UPSTASH_TOKEN) {
        if (!warnedMissingConfig) {
            warnedMissingConfig = true
            console.warn(
                '[tachyon] Upstash rate limiter is configured but UPSTASH_REDIS_REST_URL / UPSTASH_REDIS_REST_TOKEN are missing; skipping shared rate limiting'
            )
        }
        return null
    }

    const response = await fetch(`${UPSTASH_URL}/pipeline`, {
        method: 'POST',
        headers: {
            Authorization: `Bearer ${UPSTASH_TOKEN}`,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify([
            ['EVAL', RATE_LIMIT_SCRIPT, '1', key, String(limit), String(windowMs)],
        ]),
    })

    const payload = await response.json() as UpstashResponse
    const result = payload[0]

    if (!response.ok || !result || result.error) {
        throw new Error(result?.error || `Upstash rate limit request failed with ${response.status}`)
    }

    const [current, remaining, ttl] = result.result as [number, number, number]
    const ttlMs = Math.max(Number(ttl) || 0, 0)

    return {
        allowed: Number(current) <= limit,
        limit,
        remaining: Math.max(Number(remaining) || 0, 0),
        resetAt: Date.now() + ttlMs,
        headers: {
            'X-RateLimit-Backend': 'upstash',
        },
    }
}

const middleware: MiddlewareModule = {
    rateLimiter: {
        async take(request, context) {
            const limit = getConfiguredLimit()
            const windowMs = getConfiguredWindowMs()

            if (limit <= 0 || windowMs <= 0) return null

            const pathname = new URL(request.url).pathname
            const key = `${RATE_LIMIT_PREFIX}:${context.ipAddress}:${pathname}`

            return takeSharedRateLimit(key, limit, windowMs)
        },
    },
}

export default middleware
