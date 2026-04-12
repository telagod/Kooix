# Kooix Showcase

## 1. Demo application: log triage

Files:

- `examples/demo_log_triage_lib.kooix`
- `examples/demo_log_triage_main.kooix`
- `examples/demo_log_triage_sample.log`

What it demonstrates:

- module-aware imports (`import "demo_log_triage_lib" as Demo;`)
- typed records and enums (`ScanStats`, `Health`)
- `match` + `while` over text bytes
- host-backed file I/O through stdlib (`fs_read_text`, `fs_write_text`)
- native-packaged Kooix CLI turning source into a runnable binary

Run it:

```bash
cargo run -p kooixc -- native examples/demo_log_triage_main.kooix /tmp/kx-demo-log-triage --run -- \
  examples/demo_log_triage_sample.log /tmp/kx-demo-log-triage.report

cat /tmp/kx-demo-log-triage.report
```

Expected output shape:

```text
Kooix Demo: log triage report
status=critical
lines=6
errors=2
warnings=1
infos=3
critical=1
score=20
```

## 2. Benchmark: interpreter vs native on the same Kooix source

Files:

- `examples/benchmark_text_scan.kooix`
- `scripts/benchmark_text_scan.sh`

What it demonstrates:

- one Kooix source can run through the Stage0 interpreter (`kooixc run`)
- the same source can also be compiled to native (`kooixc native`) for materially lower runtime overhead
- the benchmark stresses hot-path text prefix classification, one of the current language/runtime strengths

Run it:

```bash
./scripts/benchmark_text_scan.sh
```

The script will:

1. build `target/debug/kooixc`
2. compile the benchmark source once to `/tmp/kx-benchmark-text-scan`
3. measure interpreter runs
4. measure native binary runs
5. print average runtime and speedup

## Why this is a good first-language showcase

- **Single source, two execution paths**: prototype quickly with `run`, then switch to `native` for packaged execution.
- **Typed text-processing core**: `Text`, `Option`, `Result`, `List`, records, enums, and `match` are already enough for deterministic scanners and small automation tools.
- **Host boundary stays explicit**: file and argv access are visible in source via stdlib/host intrinsics instead of being hidden in magical runtime state.
- **Production path is already visible**: the same repository now ships release tarballs and validates bootstrap-heavy gates, so the demo is not detached from the real toolchain.
