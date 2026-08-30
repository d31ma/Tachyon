#!/usr/bin/env bash
# Empirically verifies every feature claim in docs/PARITY_LEDGER.md.
#
# Every row is proven by running the real `ty` binary against a minimal
# fixture and observing what actually happens. Nothing here is asserted from
# reading source. Output is a table of FEATURE | EXPECTED | ACTUAL | VERDICT.
set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TY="${TAC_BIN:-${REPO}/target/release/ty}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ty-verify-XXXXXX")"

# Probe projects live outside the repo, so the repo's own TypeScript is put on
# PATH. Without this the probes fall back to whatever tsc the machine happens
# to have, and the tac.ts claims verify the fail-closed path instead of the
# working one.
if [ -x "${REPO}/node_modules/.bin/tsc" ]; then
  PATH="${REPO}/node_modules/.bin:${PATH}"
  export PATH
fi
trap 'rm -rf "${WORK}"' EXIT

PASS=0; FAIL=0
declare -a ROWS=()

# probe <name> <expectation> — reads fixture files from stdin as "path<TAB>content"
# Sets PROBE_RESULT to "ok" or the first TY#### diagnostic emitted.
probe() {
  local name="$1" project="${WORK}/$(echo "$1" | tr -c 'a-zA-Z0-9' '_')"
  rm -rf "${project}"; mkdir -p "${project}"
  while IFS=$'\t' read -r path content; do
    [ -z "${path}" ] && continue
    mkdir -p "${project}/$(dirname "${path}")"
    printf '%b' "${content}" > "${project}/${path}"
  done
  local out
  out="$("${TY}" build "${project}" 2>&1)"
  if [ $? -eq 0 ]; then
    PROBE_RESULT="ok"
    PROBE_DIST="${project}/dist"
  else
    PROBE_RESULT="$(printf '%s' "${out}" | grep -oE 'TY[0-9]{4}' | head -1)"
    PROBE_RESULT="${PROBE_RESULT:-FAILED}"
    PROBE_DIST=""
  fi
}

record() {
  local feature="$1" expected="$2" actual="$3"
  if [ "${expected}" = "${actual}" ]; then
    ROWS+=("PASS|${feature}|${expected}|${actual}"); PASS=$((PASS+1))
  else
    ROWS+=("FAIL|${feature}|${expected}|${actual}"); FAIL=$((FAIL+1))
  fi
}

check() { # check <feature> <expected>  (fixtures on stdin)
  probe "$1"
  record "$1" "$2" "${PROBE_RESULT}"
}

PAGE='<main aria-label="T"><h1>T</h1></main>'

echo "verifying against: ${TY}"
"${TY}" --version
echo

# ---------------------------------------------------------------- Tac layer
check "tac.html static page" "ok" <<EOF
client/pages/tac.html	${PAGE}
EOF

check "tac.css companion" "ok" <<EOF
client/pages/tac.html	${PAGE}
client/pages/tac.css	h1{color:red}
EOF

check "tac.js companion" "ok" <<EOF
client/pages/tac.html	${PAGE}
client/pages/tac.js	export default {}
EOF

# tac.ts is emitted by the TypeScript compiler, so the expectation depends on
# whether a version 6-or-newer compiler is reachable. TY1009 is the correct, documented
# outcome when it is not; silently passing either way would prove nothing.
TS_MAJOR="$(tsc --version 2>/dev/null | awk '{print $NF}' | cut -d. -f1)"
if [ "${TS_MAJOR:-0}" -ge 6 ] 2>/dev/null; then
  TS_EXPECTED="ok"
else
  TS_EXPECTED="TY1009"
  echo "note: TypeScript ${TS_MAJOR:-none} on PATH; tac.ts expected to fail closed"
fi
check "tac.ts companion" "${TS_EXPECTED}" <<EOF
client/pages/tac.html	${PAGE}
client/pages/tac.ts	export const x: number = 1
EOF

check "tac.py polyglot companion" "TY1008" <<EOF
client/pages/tac.html	${PAGE}
client/pages/tac.py	x = 1
EOF

check "inline <style> in view" "ok" <<EOF
client/pages/tac.html	<main aria-label="T"><style>h1{color:red}</style><h1>T</h1></main>
EOF

check "inline <script> in view" "TY1306" <<EOF
client/pages/tac.html	<main aria-label="T"><script>console.log(1)</script><h1>T</h1></main>
EOF

check "released literal page state and class" "ok" <<EOF
client/pages/tac.html	<script>let count = 0</script><main aria-label="T"><button on:click="count += 1">Add</button><output>{count}</output></main>
client/pages/tac.js	export default class {\n  @onMount\n  initialize() {}\n}
EOF

check "nested route directory" "ok" <<EOF
client/pages/tac.html	${PAGE}
client/pages/about/tac.html	<main aria-label="A"><h2>A</h2></main>
EOF

check "dynamic route segment _id" "ok" <<EOF
client/pages/tac.html	${PAGE}
client/pages/_id/tac.html	<main aria-label="D"><h2>D</h2></main>
EOF

check "invalid dynamic segment name" "TY1006" <<EOF
client/pages/tac.html	${PAGE}
client/pages/_1bad/tac.html	<main aria-label="D"><h2>D</h2></main>
EOF

check "void elements unclosed" "ok" <<EOF
client/pages/tac.html	<main aria-label="T"><img src="/a.png" alt="A"><hr><br></main>
EOF

check "component two-level path" "ok" <<EOF
client/pages/tac.html	<main aria-label="T"><product-card><p>s</p></product-card></main>
client/components/product/card/tac.html	<article aria-label="C"><slot></slot></article>
EOF

# The page-level case passed while the component case failed for months,
# because only the page case was probed. Both are checked now.
check "component tac.css companion" "ok" <<EOF
client/pages/tac.html	<main aria-label="T"><product-card><p>s</p></product-card></main>
client/components/product/card/tac.html	<article aria-label="C"><slot></slot></article>
client/components/product/card/tac.css	article{color:red}
EOF

check "component tac.ts companion" "${TS_EXPECTED}" <<EOF
client/pages/tac.html	<main aria-label="T"><product-card><p>s</p></product-card></main>
client/components/product/card/tac.html	<article aria-label="C"><slot></slot></article>
client/components/product/card/tac.ts	export const x: number = 1
EOF

# A one-level directory names its tag now, so the claim to verify is that it
# works — and that a directory named for a real HTML element does not.
check "component one-level path" "ok" <<EOF
client/pages/tac.html	<main aria-label="T"><clicker>s</clicker></main>
client/components/clicker/tac.html	<article aria-label="C"><slot></slot></article>
EOF

check "component shadowing an HTML element" "TY1401" <<EOF
client/pages/tac.html	<main aria-label="T"><h1>T</h1></main>
client/components/section/tac.html	<article aria-label="C">s</article>
EOF

# --------------------------------------------------------------- Yon layer
check "yon.html is rejected" "TY1008" <<EOF
server/routes/yon.html	<main>not a Yon endpoint</main>
EOF

check "yon.js handler discovery" "ok" <<EOF
server/routes/yon.js	@Controller\nexport class RootController { static GET() { return { ok: true } } }
EOF

check "yon.py handler discovery" "ok" <<EOF
server/routes/yon.py	@Controller\nclass RootController:\n    @staticmethod\n    def GET(request):\n        return {"ok": True}
EOF

# Maintained Yon languages are discovered through their mandatory controller.
check "yon.ts controller discovery" "ok" <<EOF
server/routes/yon.ts	@Controller\nexport class RootController { static GET() { return {} } }
EOF

check "yon.rs controller discovery" "ok" <<EOF
server/routes/yon.rs	#[Controller]\nstruct RootController;\nimpl RootController {\n    fn GET(_request: &YonRequest) -> YonResponse { YonResponse::json("{}") }\n}
EOF

# Arbitrary interpreter registration no longer turns another language into Yon.
check "yon.rb remains unsupported when an interpreter is registered" "TY2003" <<EOF
server/routes/yon.rb	require 'json'\nrequest = JSON.parse(STDIN.read)\nputs JSON.generate({ status: 200, body: JSON.generate({ ok: true }) })\n
.tachyonrc	{"interpreters":{"rb":["ruby"]}}
EOF

# Structural controls belong only to Tac and are serialized for the browser.
check "conditional <if :when>/<else>" "ok" <<EOF
client/pages/tac.html	<main><if :when="on"><p>y</p></if><else><p>n</p></else></main>
EOF

check "iteration <for :each>" "ok" <<EOF
client/pages/tac.html	<main><for :each="i in items"><p>{i}</p></for></main>
EOF

check "dynamic attribute :attr" "ok" <<EOF
client/pages/tac.html	<main :aria-label="label"><h1>x</h1></main>
EOF

check "middleware.js present" "ok" <<EOF
client/pages/tac.html	${PAGE}
middleware.js	@Controller\nexport class AccessController { static GET() { return { status: 204 } } }
EOF

check "server/workers present" "ok" <<EOF
client/pages/tac.html	${PAGE}
server/workers/job.js	export default {}
EOF

check ".tachyonrc present" "ok" <<EOF
client/pages/tac.html	${PAGE}
.tachyonrc	{"interpreters":{}}
EOF

check "handler importing a service module is not executed at build" "ok" <<EOF
server/routes/yon.js	import { ValueService } from '../services/s.js'\n@Controller\nexport class RootController { static GET() { return { v: ValueService.v } } }
server/services/s.js	@Service\nexport class ValueService { static v = 'from-service' }
EOF

# ------------------------------------------------------------------ report
echo
printf '%-6s %-42s %-10s %s\n' "VERDICT" "FEATURE" "EXPECTED" "ACTUAL"
printf '%.0s-' {1..86}; echo
for row in "${ROWS[@]}"; do
  IFS='|' read -r verdict feature expected actual <<< "${row}"
  printf '%-6s %-42s %-10s %s\n' "${verdict}" "${feature}" "${expected}" "${actual}"
done
echo
echo "${PASS} claims confirmed, ${FAIL} claims wrong"
[ "${FAIL}" -eq 0 ]
