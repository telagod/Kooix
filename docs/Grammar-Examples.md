# Grammar Examples (Core v0 + AI v1)

## Core v0 Examples (Implemented)

### Valid: capability + function with effects/requires

```kooix
cap Net<"api.openai.com">;
cap Model<"openai", "gpt-4o-mini", 1000>;

fn summarize(doc: Text) -> Summary !{model(openai), net} requires [Model<"openai", "gpt-4o-mini", 1000>, Net<"api.openai.com">];
```

### Valid: pure function

```kooix
fn add(x: Int, y: Int) -> Int;
```

### Invalid: malformed capability shape

```kooix
cap Model<"openai", "gpt-4o-mini">;
fn summarize(doc: Text) -> Summary !{model(openai)} requires [Model<"openai", "gpt-4o-mini">];
```

Expected category: semantic error (capability arity mismatch).

## AI v1 Examples (Partial + Target)

> Current implementation parses function-level contract subset + minimal `workflow` + minimal `agent` subset.

### Valid now: function with intent + ensures + failure

```kooix
fn summarize(doc: Text) -> Summary
intent "compress input document into an actionable summary"
ensures [output.confidence >= 0]
failure {
  timeout -> retry(exp_backoff, max=3);
  invalid_output -> fallback("template");
}
evidence {
  trace "summarize.v1";
  metrics [latency_ms, token_in, token_out];
}
;
```

### Target syntax (not fully implemented yet)

### Valid now: minimal workflow

```kooix
cap Tool<"vector_search", "read-only">;

workflow ingest_and_answer(input: Query) -> Answer
intent "retrieve context and generate grounded answer"
requires [Tool<"vector_search", "read-only">]
steps {
  s1: normalize(input) ensures [output.score >= 0];
  s2: retrieve(input) on_fail -> retry(exp_backoff, max=2);
}
output {
  answer: Answer;
}
evidence {
  trace "workflow.ingest_and_answer.v1";
  metrics [step_latency_ms, token_total];
}
;
```

### Valid now: minimal agent

```kooix
cap Tool<"kb_search", "read-only">;

agent support_agent(input: Ticket) -> Resolution
intent "resolve support tickets within policy boundaries"
state {
  INIT -> CLASSIFIED;
  CLASSIFIED -> DONE, ESCALATED;
  any -> ESCALATED;
}
policy {
  allow_tools ["kb_search", "order_lookup"];
  deny_tools ["payment_refund"];
  max_iterations = 8;
  human_in_loop when input.risk_score > 7;
}
requires [Tool<"kb_search", "read-only">]
loop { perceive -> reason -> act -> observe; stop when state == DONE; }
ensures [state == DONE]
evidence {
  trace "agent.support.v1";
  metrics [iteration_count, policy_block_count];
}
;
```

### Function (AI-native contract)

```kooix
fn summarize(doc: Text) -> Summary
intent "compress input document into an actionable summary"
requires [cap Model<"openai", "gpt-4o-mini", 1000>, cap Net<"api.openai.com">]
effects !{ model("openai"), net("api.openai.com") }
ensures [len(output.items) <= 8, output.confidence >= 0.75]
failure {
  timeout -> retry(exp_backoff, max=3);
  invalid_output -> fallback("template");
}
evidence {
  trace "summarize.v1";
  metrics [latency_ms, token_in, token_out, cost_usd];
}
;
```

### Workflow (explicit step graph)

```kooix
workflow ingest_and_answer(input: Query) -> Answer
intent "retrieve context and generate grounded answer"
requires [cap Tool<"vector_search", "read-only">, cap Model<"openai", "gpt-4o-mini", 2000>]
sla { p95_latency_ms <= 2500; }
steps {
  s1: normalize(input) ensures [normalized.lang in ["zh", "en"]];
  s2: retrieve(normalized)
      ensures [len(docs) >= 1]
      on_fail -> retry(exp_backoff, max=2);
  s3: generate(docs, normalized)
      ensures [answer.confidence >= 0.70]
      on_fail -> fallback("template_answer");
}
output {
  answer: Answer;
}
evidence {
  trace "workflow.ingest_and_answer.v1";
  metrics [step_latency_ms, token_total];
}
;
```

### Agent (state + policy + loop)

```kooix
agent support_agent(input: Ticket) -> Resolution
intent "resolve support tickets within policy boundaries"
state {
  INIT -> CLASSIFIED;
  CLASSIFIED -> DONE, ESCALATED;
  any -> ESCALATED;
}
policy {
  allow_tools ["kb_search", "order_lookup"];
  deny_tools ["payment_refund"];
  max_iterations = 8;
  human_in_loop when risk_score > 0.7;
}
loop { perceive -> reason -> act -> observe; stop when state == DONE; }
ensures [state == DONE or state == ESCALATED]
evidence {
  trace "agent.support.v1";
  metrics [iteration_count, policy_block_count];
}
;
```

## AI v1 Negative Examples (Target validation)

### Invalid: unknown failure action

```kooix
fn f(x: Int) -> Int
failure {
  timeout -> teleport("nowhere");
}
;
```

Expected category: parser/sema error (unknown failure action).

### Invalid: evidence metrics empty

```kooix
fn f(x: Int) -> Int
evidence {
  trace "f.v1";
  metrics [];
}
;
```

Expected category: semantic error or warning (empty metrics list).

### Invalid: agent policy conflict

```kooix
agent a(input: Ticket) -> Resolution
state { INIT -> DONE; }
policy {
  allow_tools ["foo"];
  deny_tools ["foo"];
}
loop { perceive -> act; stop when true == true; }
;
```

Expected category: semantic warning/error (policy conflict; deny precedence).
