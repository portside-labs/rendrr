# Security Policy

## Reporting a vulnerability

If you believe you've found a security issue in Rendrr, please **do not open
a public GitHub issue**. Instead, report it privately via GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
feature on this repository.

Please include:

- A description of the issue and its impact
- A minimal reproduction (template, payload, request)
- Your environment (Rendrr version, OS, OAuth/TLS configuration)

We aim to acknowledge reports within 5 business days.

## Operational hardening notes

- **Always run behind TLS** in any deployment that handles real user data —
  either via `TLS_CERT_PATH`/`TLS_KEY_PATH` directly on the container, or via
  a TLS-terminating reverse proxy in front of it.
- **Use OAuth in any shared/production deployment.** Without `OAUTH_ISSUER`
  set, all endpoints are open.
- **Restrict the IAM credentials** used for the S3-compatible buckets to just
  the `PutObject`, `GetObject`, `HeadObject`, and `DeleteObject` permissions
  on the template and render buckets.
- **Rate limiting is your proxy's job.** Rendrr has no built-in rate limiter.
  Rendering is CPU- and memory-bound, so an unmetered public deployment is
  trivially exhaustible. Put a limiter in the reverse proxy or API gateway in
  front of it. An in-process limiter was deliberately left out: the service is
  stateless and meant to scale horizontally, where per-instance counters give
  a misleading picture of the actual request rate.

- **Templates are user-controlled content.** They are validated for DOCX
  structure and Handlebars syntax, but should still be treated as untrusted
  when rendered with sensitive data.

## Built-in limits

These are enforced by the service itself. They are guardrails against
accidental and hostile resource exhaustion, not a substitute for the
network-level controls above.

| Limit                          | Value  | Notes                                        |
| ------------------------------ | ------ | -------------------------------------------- |
| Request body                   | 50MB   | Outer HTTP guard; returns 413.               |
| Template upload                | 25MB   | Compressed size; returns 400.                |
| Decompressed archive entry     | 100MB  | Guards against ZIP bombs (a `.docx` is a ZIP). |
| Render JSON payload            | 10MB   | Returns 400.                                 |
| JSON nesting depth             | 100    | Guards against stack exhaustion.             |
| Array length in render data    | 10,000 | Per array.                                   |
| Image download                 | 10MB   | Enforced while streaming, not after buffering. |
| Image dimensions               | 4096px | Per side.                                    |
| Image fetch timeout            | 30s    |                                              |
| Image fetch redirects          | 3      | Each hop re-checked against the SSRF guard.  |

## Outbound requests and SSRF

The `{{image}}` helper fetches URLs supplied in caller-controlled render data,
which makes the server an HTTP client on the caller's behalf. By default
Rendrr:

- accepts only `http` and `https` schemes;
- resolves the target host and refuses to connect if **any** resolved address
  is non-public — loopback, RFC1918, CGNAT, multicast, or link-local, the last
  of which covers cloud metadata endpoints such as `169.254.169.254`;
- follows redirects manually, re-running both checks on every hop, so a public
  URL cannot bounce to a private one.

`IMAGE_FETCH_ALLOW_PRIVATE_NETWORKS=true` disables the address check. Set it
only when you serve template images from inside your own network **and** trust
every client that can submit a render — with it on, any caller can use the
render endpoint to probe your internal network.

One residual gap is worth stating plainly: the hostname is resolved once for
the check and again by the HTTP client, so an attacker who controls a DNS
server and can win that race could still redirect the connection. Closing it
requires pinning the connection to the validated address, which the HTTP
client in use does not expose.
