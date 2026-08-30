# ADR 0014: Environment-Selected Yon Isolation Backends

- Status: Accepted
- Date: 2026-08-09

## Context

Yon supervises one bounded child process per handler invocation. That boundary
contains crashes, protocol violations, output, concurrency, deadlines, and
environment inheritance, but it does not remove the child process's ambient
operating-system filesystem or network authority.

Hardware-isolated multi-tenant deployments need a stronger backend such as a
Firecracker microVM. Isolation is an operator security decision, however. An
application repository must not be able to weaken the deployment boundary by
editing `tachyon.json` or another project file.

Firecracker is also a low-level Linux/KVM virtual-machine monitor rather than
an application runtime. Yon should preserve route and handler semantics while
a separately owned control program manages kernels, root filesystems, pools,
snapshots, networking, the jailer, and host qualification.

## Decision

Yon selects handler isolation exclusively from the parent process environment:

| Variable | Meaning | Default or bound |
| --- | --- | --- |
| `YON_ISOLATION` | `process` or `firecracker` | `process` |
| `YON_FIRECRACKER_DRIVER` | Absolute control-program path | required for `firecracker`; regular, non-symlinked, executable, and not group/world-writable |
| `YON_FIRECRACKER_POOL` | Operator-owned pool identifier | `default`; 1–64 portable identifier characters |
| `YON_FIRECRACKER_VCPUS` | Requested virtual CPUs | `1`; 1–32 |
| `YON_FIRECRACKER_MEMORY_MIB` | Requested guest memory | `256`; 128–32,768 MiB |
| `YON_FIRECRACKER_EGRESS` | Guest network policy | `deny`; no other value is currently accepted |

There is no project-file equivalent and no handler-request override.
Malformed, partial, non-Unicode, or out-of-range configuration fails with
`TY2010` before a handler runs.

`process` retains the existing direct-spawn backend. `firecracker` directly
spawns the configured control program without a shell and sends it the same
length-prefixed Handler Protocol v1 request used by built-in adapters. The
control program receives canonical project/source metadata, adapter identity,
pool, CPU, memory, and deny-egress policy as separated arguments. It must
return one bounded Handler Protocol v1 response and accept the existing
cancellation frame. Yon retains deadline, stderr, output, cancellation,
forced-termination, and concurrency supervision around the control program.

The driver contract currently accepts only validated JavaScript and Python
source snapshots, whose stable adapter identities are `javascript.v1` and
`python.v1`. The project-relative `--source` identity is resolved below an
owned project-shaped `--project-root`, so an ambient authored-path replacement
cannot change what the control program transfers. TypeScript and the Java,
PHP, Kotlin, C#, and Rust direct paths
depend on a runtime workspace prepared by Tachyon. Those workspaces are never
represented by the project-relative `--source` argument. Firecracker mode
therefore rejects them with `TY2010` before the driver starts. A future
extension must define an authenticated, bounded artifact-set transfer; passing
an individual prepared artifact path or authored source as a substitute is not
an artifact contract.

The control program is a trusted deployment component. It is responsible for
proving that the selected pool really uses Firecracker, the jailer, unique
credentials, cgroups, namespaces, seccomp, immutable images, safe snapshot
lineage, and deny-by-default guest networking. Merely selecting the backend is
not evidence of hardware isolation.

The same environment-selected policy applies to HTTP route handlers,
middleware, scheduled workers, and explicit `ty handler invoke`. Yon handlers
are never executed during compilation.

## Consequences

- Application code cannot downgrade production isolation.
- Handler Protocol v1 stays independent of virtualization and language.
- A thin control client may connect to a local pool daemon, so microVMs need
  not be created once per HTTP request.
- Process mode remains behavior-compatible unless an operator opts into
  another backend.
- Firecracker remains deployable only where the control program and host have
  independent production evidence.
- Firecracker mode is currently limited to JavaScript and Python; selecting it
  for a prepared TypeScript or direct-language handler fails closed before the
  driver starts.
- Allowlisted egress, writable filesystems, warm-pool policy, snapshot
  identity, attestations, and a first-party control program require later
  vertical slices. They may not be implied by this transport boundary.

## Rejected Alternatives

- **Isolation in `tachyon.json`**: lets an application weaken an operator's
  security boundary and mixes deployment authority into source configuration.
- **Invoke the Firecracker binary as a handler**: Firecracker's API does not
  speak Handler Protocol and does not understand Yon sources or routes.
- **One hard-coded virtualization backend inside the supervisor**: couples
  application semantics to Linux/KVM lifecycle details and prevents qualified
  provider substitution.
- **Allow unrestricted egress initially**: creates an SSRF and data-exfiltration
  boundary before origin, DNS, redirect, and response controls exist.

## Acceptance Gate

- environment parsing proves defaults, bounds, missing values, unsafe paths,
  invalid identifiers, and deny-only egress;
- a real executable control-driver fixture receives framed Handler Protocol v1
  plus the bounded policy and returns a validated response;
- prepared TypeScript and direct-language sources are rejected before the
  control driver starts, while JavaScript and Python retain their stable
  adapter identities;
- process-mode handler tests remain unchanged;
- every handler entry point constructs its supervisor from the same parent
  environment;
- documentation makes no hardware-isolation claim without qualified driver and
  host evidence.
