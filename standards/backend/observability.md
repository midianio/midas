# Observability & Telemetry — the seam

How the backend captures logs, traces, metrics, errors, LLM usage, product analytics, and feature
flags. The seam is defined **once** so every handler inherits it for free — this is `BE-0012`
(structured logs) + `BE-0013` (telemetry through vendor-neutral ports). Canonical code:
`src/telemetry/`.

The principle: pick one telemetry vendor for the bill *(midian: PostHog Cloud — analytics, error
tracking, LLM/AI observability, logs, tracing, session replay, flags)*, but stay **vendor-neutral at
the boundary** — swapping any one capability to another provider must be an adapter swap, not a
handler-code refactor. That requirement is the whole design.

## Two tiers of abstraction

The right abstraction differs by capability, so there are two:

| Tier             | Capabilities                                              | Abstraction                                              | How you swap providers                       |
| ---------------- | --------------------------------------------------------- | -------------------------------------------------------- | -------------------------------------------- |
| **1 — pillars**  | traces, metrics, logs                                     | **OpenTelemetry** (the abstraction is already a standard) | change `OTEL_EXPORTER_OTLP_ENDPOINT`         |
| **2 — products** | error tracking, LLM obs, product analytics, feature flags | **a narrow Rust trait (port)** per capability             | write a new adapter impl; handlers untouched |

Tier 1 needs no custom code to be swap-safe — emitting OTLP *is* the abstraction. Tier 2 has no
universal wire, so each capability gets a port.

---

## Tier 1 — OpenTelemetry pipeline (traces / metrics / logs)

Instrument with the `tracing` crate (already the logging API); a `tracing-opentelemetry` layer turns
spans into OTel spans, exported over OTLP. Logs ride the same subscriber. Metrics use the
`opentelemetry` metrics API through the same exporter.

The subscriber is `EnvFilter` + stdout `fmt` + an `Option<tracing_opentelemetry::layer>` (a no-op when
OTLP is off). The exporter (opentelemetry-otlp **0.27**) uses the real 0.27 builder — note this
differs from the older `new_pipeline()` form in many examples:

```rust
let exporter = SpanExporter::builder().with_tonic().with_endpoint(endpoint).build()?;
let provider = TracerProvider::builder()
    .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
    .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio))))
    .with_resource(Resource::new(vec![KeyValue::new("service.name", "<service>")]))
    .build();
```

Transport is **gRPC/tonic + rustls (webpki roots)** — deliberately *not* the `reqwest`/`http-proto`
client, which pulls native-tls/openssl and breaks the Alpine/musl Docker build. Endpoint defaults to
the OTLP gRPC port (`4317`). **Fail-open:** a bad/absent endpoint logs a warning and stays on stdout.
`init` returns an `OtelGuard`; `main` calls `otel_guard.shutdown()` after HTTP drains, to flush spans.

`[review]` **OTLP export is opt-in by env** — no endpoint → stdout `fmt` only, no behaviour change.
Because Tier 1 is OTLP-native, the pillars can point anywhere (the vendor, or Grafana/Honeycomb) by
flipping one env var, independently of Tier 2. *(midian currently runs Tier 1 dormant — wired,
stdout-only — and relies on Tier 2 for the product-critical telemetry.)*

**Instrumented automatically** (no per-handler code): one root span per request (carrying
`request_id`, `http.method`, `http.route`, `http.status_code`, `latency_ms`); nested `sqlx` query
spans; nested outbound-HTTP spans. **Metrics** (start minimal, all via OTel):
`http_requests_total{route,method,status}` · `http_request_duration_ms{route}` ·
`db_pool_connections{state}` (from `pool.size()`/`pool.num_idle()`) · `llm_tokens_total{model,kind}`.

---

## Tier 2 — the telemetry ports (error / LLM / analytics / flags)

Four narrow traits; the vendor lives only in the adapters. They hang off `AppState`, so every handler
reaches them through `State<AppState>` — no globals.

```rust
// src/telemetry/mod.rs
pub trait Analytics: Send + Sync {            // product + behavioral events
    fn capture(&self, distinct_id: &str, event: &str, props: serde_json::Value);
}
pub trait ErrorSink: Send + Sync {            // unexpected-error reporting; swap → Sentry = a new impl
    fn capture(&self, err: &ErrorEvent);      // type, message, stacktrace, request_id, user_id, route
}
pub trait LlmObserver: Send + Sync {          // LLM cost/tokens/latency; swap → Helicone/Langfuse = a new impl
    fn record(&self, gen: &LlmGeneration);    // model, provider, in/out tokens, cost, latency, trace_id, status
}
#[async_trait::async_trait]
pub trait FeatureFlags: Send + Sync {
    async fn variant(&self, flag: &str, ctx: &FlagContext) -> Option<String>;
    async fn enabled(&self, flag: &str, ctx: &FlagContext) -> bool {
        self.variant(flag, ctx).await.is_some_and(|v| v != "false")
    }
}

/// The bundle carried in AppState. `Telemetry::vendor(cfg)` is the entire swap surface.
#[derive(Clone)]
pub struct Telemetry {
    pub analytics: std::sync::Arc<dyn Analytics>,
    pub errors:    std::sync::Arc<dyn ErrorSink>,
    pub llm:       std::sync::Arc<dyn LlmObserver>,
    pub flags:     std::sync::Arc<dyn FeatureFlags>,
}
impl Telemetry {
    pub fn noop() -> Self { /* for tests + local-without-keys */ }
    // midian: Telemetry::posthog(cfg) — one adapter constructor backing analytics + errors + llm
}
```

`[review]` **`capture`/`record` are non-blocking.** They push onto an in-process bounded `mpsc` queue
and return immediately — a request never waits on the telemetry vendor. One shared background worker
drains, batches, and POSTs; this also gives clean shutdown flushing. *(midian: error, LLM, and
analytics events are all captures under one PostHog client/worker — `$exception`, `$ai_generation`,
normal events. Flags are a read path and get their own adapter.)*

---

## Wiring — how it propagates without per-handler code

The point of the seam: a porter writing slice #94 should **not** have to remember telemetry. It rides
choke points every slice already flows through.

1. **Request span + IDs** — global tower layers in `routes::build`:
   ```rust
   .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))   // x-request-id if absent
   .layer(TraceLayer::new_for_http().make_span_with(|req| {
       tracing::info_span!("http", request_id = %req_id(req), method = %req.method(),
                           route = %route(req), user_id = tracing::field::Empty,
                           status = tracing::field::Empty)
   }))
   .layer(PropagateRequestIdLayer::x_request_id())            // echo it back
   ```
   Every log line and OTel span in that request inherits `request_id`.
2. **Error capture rides `AppError::into_response()`** — the single place every route returns errors.
   We already log there; one `errors.capture()` for 5xx + an OTel `exception` event on the current span
   makes error tracking automatic for every handler. (Reachable via a task-local `Telemetry` or by
   recording on `Span::current()` and forwarding from an `ErrorLayer`.)
3. **LLM observability rides the AI-client wrapper** — the single LLM choke point. After each
   call/stream completes, `llm.record(...)` with model/tokens/cost/latency. Every AI endpoint gets it
   for free.
4. **`user_id` enrichment** — the auth extractor records the verified `sub` onto the current span, so
   every log/trace/error after auth is attributed.
5. **Product events + flags are explicit** (semantic, not mechanical): call `st.telemetry.analytics.capture(...)`
   / `st.telemetry.flags.variant(...)` where it means something — but **through the port**, never a
   raw vendor call.

**Rule for porters** (`BE-0012` + `BE-0013`): never call a vendor SDK directly, never `println!`/bare
`log`. Logs = `tracing::{info,warn,error}!` with fields. Errors = return `AppError` (capture is
automatic). LLM = through the AI client. Analytics/flags = through `st.telemetry`. Enforced by
`clippy -D warnings` + a deny on `print_stdout`/`print_stderr`.

---

## Cross-cutting rules (baked into every adapter)

- `[review]` **PII scrubbing at the emit boundary.** A central `scrub()` runs in the vendor worker and
  an OTel span processor: **allowlist** fields, don't blocklist — drop/redact `email`, `token`,
  `authorization`, session cookies, raw request bodies. Never put a raw token/secret/PII in an error
  message. *(midian handles user PII — emails, study data — so this is load-bearing, not optional.)*
- `[review]` **Sampling & cost guardrails.** SSE + LLM generate huge span/event volume. Head-based
  trace sampling (`OTEL_TRACE_SAMPLE_RATIO`, default 0.1 prod / 1.0 dev); **errors and slow requests
  always sampled** (tail rule). SSE streams emit *span events*, not a span per token. The `mpsc` queue
  is bounded → drop-with-counter under load (never block a request) and increment
  `telemetry_dropped_total` so silent loss is visible.
- `[review]` **Graceful flush on shutdown.** `main.rs` sequence: drain HTTP → close the telemetry
  `mpsc` sender → await the worker's final flush → `opentelemetry::global::shutdown_tracer_provider()`
  → close the DB pool. Bounded by a timeout. (This is the shutdown ordering `BE-0017` refers to.)
- `[review]` **Fail-open, never fail-closed.** Vendor down or keys missing → the app serves normally;
  telemetry is best-effort. `Telemetry::noop()` is the zero-config local/test default.

---

## Vendor specifics *(midian: PostHog Cloud)*

The seam is vendor-neutral; this is the current adapter's wiring, kept as a worked example.

- **Host:** `https://us.i.posthog.com` (US) / `https://eu.i.posthog.com` (EU) — config.
- **Keys:** project key `phc_…` (`POSTHOG_API_KEY`) for capture/flags; a personal key
  (`POSTHOG_PERSONAL_KEY`) only for **local** flag evaluation. The `phc_` key is public-by-design
  (write-only capture).
- **Capture:** batch to `POST {host}/batch/` (`{api_key, batch:[{event,distinct_id,properties}]}`).
- **Event shapes:** errors → `$exception` (`$exception_list:[{type,value,stacktrace:{frames}}]`);
  LLM → `$ai_generation` (`$ai_model`, `$ai_provider`, `$ai_input_tokens`, `$ai_output_tokens`,
  `$ai_latency`, `$ai_trace_id`; `$ai_input`/`$ai_output` only after scrub) — powers AI observability;
  product → normal named events.
- **Feature flags:** prefer **local evaluation** for latency (fetch definitions periodically with the
  personal key, evaluate in-process against `FlagContext { distinct_id, properties }`); fall back to
  remote `POST {host}/decide` on cache miss.
- **OTLP (logs/tracing):** set `OTEL_EXPORTER_OTLP_ENDPOINT` + `OTEL_EXPORTER_OTLP_HEADERS` per the
  vendor's OTel docs; verify endpoint/auth at wire-up (these surfaces can be beta/alpha).

## Config / env reference (`config.rs`)

| Env var                       | Purpose                                       | Default                         |
| ----------------------------- | --------------------------------------------- | ------------------------------- |
| `POSTHOG_API_KEY`             | capture + remote flags                        | none → telemetry = noop         |
| `POSTHOG_PERSONAL_KEY`        | local flag evaluation (optional)              | none → remote `/decide`         |
| `POSTHOG_HOST`                | US/EU cloud host                              | `https://us.i.posthog.com`      |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | traces/metrics/logs target                    | none → OTel disabled (fmt-only) |
| `OTEL_EXPORTER_OTLP_HEADERS`  | OTLP auth                                      | none                            |
| `OTEL_TRACE_SAMPLE_RATIO`     | head sampling                                 | `1.0` dev / `0.1` prod          |
| `RUST_LOG` / `EnvFilter`      | log level                                     | `info`                          |

`[review]` **Startup validation:** `Config::from_env` warns loudly when production has no telemetry
keys and **fails fast** on malformed required config — don't silently fall back.

## Cargo deps (as wired)

```toml
opentelemetry = "0.27"
opentelemetry_sdk = { version = "0.27", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.27", default-features = false, features = [
    "trace", "grpc-tonic", "tls-webpki-roots",   # gRPC + rustls; NOT reqwest/http-proto (openssl)
] }
tracing-opentelemetry = "0.28"
uuid = { version = "1", features = ["v4"] }
async-trait = "0.1"
# reqwest (existing, rustls) backs the vendor batching client + flag /decide calls.
```

## Porter checklist (short form)

- [ ] Logs via `tracing::{info,warn,error}!` with structured fields — **never** `println!`/bare log.
- [ ] Errors: return `AppError`; capture is automatic. Never put secrets/PII in the message.
- [ ] LLM: call through the AI-client wrapper (records usage); don't hit the LLM provider raw.
- [ ] Analytics/flags: through `st.telemetry.*`; never a raw vendor call.
- [ ] New scalar fields that could be PII → add to the scrub allowlist review.
