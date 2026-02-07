# Aster MVP Design

## 设计目标

1. 用最小实现验证 **Effect + Capability** 的类型化设计。
2. 保持语法接近 Rust 风格，同时提升 AI 场景语义表达力。
3. 保证前端校验可测试、可扩展，便于后续接入 LLVM backend。

## 非目标

- 不在本阶段实现完整执行语义。
- 不在本阶段实现 JIT/AOT 输出。

## 架构设计

```text
Source (.aster)
  -> Lexer (Token stream)
  -> Parser (AST)
  -> HIR Lowering
  -> MIR Lowering
  -> Semantic Checker (effect/capability rules)
  -> LLVM IR Text Backend
  -> llc (obj) + clang (native bin)
  -> Diagnostics / IR / Native Output
```

### 核心组件

- `lexer.rs`：词法切分，输出 `Token`。
- `parser.rs`：递归下降 parser，构建 `Program` AST。
- `hir.rs`：AST 到 HIR 的 lowering 层。
- `mir.rs`：HIR 到 MIR 的 lowering 层。
- `sema.rs`：语义规则引擎，执行 capability 约束检查。
- `llvm.rs`：MIR 到 LLVM IR 文本输出。
- `native.rs`：调用 `llc`/`clang` 将 LLVM IR 编译为本地可执行文件。
- `main.rs`：CLI 封装与诊断输出。

## 语法子集（当前实现）

```text
cap <TypeRef>;

fn <name>(<params>) -> <TypeRef>
  [intent "..."]
  [!{<effect>[, ...]}]
  [requires [<TypeRef>[, ...]]]
  [ensures [<predicate>[, ...]]]
  [failure {<condition> -> <action>(...); ...}]
  [evidence {trace "..."; metrics [m1, ...];}]
;

workflow <name>(<params>) -> <TypeRef>
  [intent "..."]
  [requires [<TypeRef>[, ...]]]
  steps {
    <step_id>: <call>(...) [ensures [...]] [on_fail -> <action>(...)];
  }
  [output {<name>: <TypeRef>; ...}]
  [evidence {trace "..."; metrics [m1, ...];}]
;

agent <name>(<params>) -> <TypeRef>
  [intent "..."]
  state { <from> -> <to>[, ...]; ... }
  policy {
    [allow_tools ["...", ...];]
    [deny_tools ["...", ...];]
    [max_iterations = <number>;]
    [human_in_loop when <predicate>;]
  }
  [requires [<TypeRef>[, ...]]]
  loop { <stage> -> <stage> ...; stop when <predicate>; }
  [ensures [<predicate>[, ...]]]
  [evidence {trace "..."; metrics [m1, ...];}]
;
```

## 方案选择与权衡

| 选项 | 结论 | 原因 |
|---|---|---|
| parser 技术 | 递归下降 | 依赖最少，便于快速迭代语法 |
| 诊断模型 | 自定义 `Diagnostic` | 先保证可控性，后续可接入更丰富错误恢复 |
| capability 绑定 | effect->capability 映射表 | 先建立可验证闭环，再演进为 trait/effect algebra |

## 关键决策

1. 将 `effect` 声明与 `requires` 能力声明耦合校验。
2. 通过 HIR 统一后续语义分析输入。
3. 对 capability 参数形状进行静态校验（如 `Model<P, M, Budget>`）。
4. 引入 MIR 作为后端稳定接口，避免 AST 直接驱动 codegen。
5. native 构建通过外部工具链调用，保持 crate 无额外依赖。
6. 要求能力在顶层 `cap` 声明后才可被函数引用。

## 已知限制

- 暂不支持函数体和表达式 AST。
- 仅支持有限的 capability schema（Model/Net/Tool/Io）。
- `ensures` 当前仅支持简单谓词：`Path|String|Number` 两侧比较（`== != < <= > >= in`）。
- `failure` 当前仅支持规则子集：`condition -> action(args);`，action 支持 `retry/fallback/abort/compensate`。
- `evidence` 当前支持 `trace` 与 `metrics` 子句，语义约束：`trace` 必填、`metrics` 非空建议。
- `workflow` 当前支持最小子集：`steps` 必填、`output/evidence` 可选、`on_fail` 复用 failure action 子集。
- `agent` 当前支持最小子集：`state/policy/loop` 必填，`requires/ensures/evidence` 可选。
- LLVM 后端目前输出文本 IR 且返回默认值，不生成真实业务逻辑。
- native 链路依赖本机安装 `llc` 和 `clang`。
- `native --run` 通过 `-- <args...>` 透传运行参数。
- `native --run --stdin <file>` 支持向子进程注入 stdin（`-` 表示读取当前进程 stdin）。
- `native --run --timeout <ms>` 支持运行超时控制，超时后强制终止子进程。
- 仅支持单文件分析，无 import/module linking。

## 兼容与迁移

- 后续可引入 HIR/MIR 层，不破坏当前 AST 接口。
- CLI `check` 命令可作为未来编译流水线前置 stage。

## 语法规范文档

- `docs/Grammar-Core-v0.ebnf`：当前 parser 已实现语法
- `docs/Grammar-AI-v1.ebnf`：AI 原生扩展目标语法
- `docs/Grammar-Mapping.md`：语法与 AST/HIR/MIR/Sema 映射
- `docs/Grammar-Examples.md`：正反例与预期错误类别

## 变更历史

### 2026-02-07 - Aster MVP 初始落地

- **变更内容**：初始化 Rust workspace，落地 lexer/parser/sema/CLI/tests。
- **变更理由**：将设计方案转为可运行原型，验证核心语义约束。
- **影响范围**：新增 `crates/asterc` 与顶层文档/示例。
- **决策依据**：优先交付最短可验证闭环。

### 2026-02-07 - Phase 2：HIR 与语义约束增强

- **变更内容**：新增 HIR lowering；增强 capability 参数与 effect 匹配校验；扩充测试与示例。
- **变更理由**：提升类型系统表达力与错误定位精度。
- **影响范围**：`crates/asterc/src/hir.rs`、`sema.rs`、CLI 与 tests。
- **决策依据**：在不引入新依赖下提升语义强度。

### 2026-02-07 - Phase 3：MIR 与 LLVM IR 后端骨架

- **变更内容**：新增 MIR 层与 LLVM IR 文本后端；新增 `mir`/`llvm` CLI 命令与测试。
- **变更理由**：为后续真正 LLVM codegen 建立可演进接口与验证路径。
- **影响范围**：`mir.rs`、`llvm.rs`、`lib.rs`、`main.rs`、tests 与 examples。
- **决策依据**：先打通可运行后端通路，再迭代语义与优化。

### 2026-02-07 - Phase 4：Native 可执行产物链路

- **变更内容**：新增 `native.rs`，实现 `llc -> clang` 构建链路；新增 `native` CLI 命令与测试。
- **变更理由**：让 LLVM IR 不止可读，还能直接生成本地二进制进行验证。
- **影响范围**：`native.rs`、`lib.rs`、`main.rs`、tests 与文档。
- **决策依据**：优先使用系统工具链，保持实现简洁可审计。

### 2026-02-07 - Phase 4.1：Native --run 模式

- **变更内容**：`native` 命令支持 `--run`，编译后自动执行并回显退出码/stdout/stderr。
- **变更理由**：减少编译后手动执行步骤，提升端到端验证效率。
- **影响范围**：`main.rs` 参数解析、`native.rs` 运行器、`lib.rs` API 与 tests。
- **决策依据**：在保持无新依赖前提下提供最小可用执行闭环。

### 2026-02-07 - Phase 4.2：Native 运行参数透传

- **变更内容**：`native --run -- <args...>` 支持将参数透传给生成的二进制。
- **变更理由**：支持更真实的运行时验证场景。
- **影响范围**：`main.rs` 参数解析、`native.rs` 运行 API、`lib.rs` 与 tests。
- **决策依据**：保持 CLI 语义兼容，仅增量扩展执行能力。

### 2026-02-07 - Phase 4.3：Native stdin 注入

- **变更内容**：`native --run --stdin <file>` 支持将文件内容写入运行进程 stdin。
- **变更理由**：覆盖需要输入流的运行验证场景。
- **影响范围**：`native.rs` 运行器、`lib.rs` API、`main.rs` 解析与 tests。
- **决策依据**：保持默认行为不变，增量提供 I/O 能力。

### 2026-02-07 - Phase 4.4：Native stdin 流模式

- **变更内容**：`--stdin -` 支持直接从 CLI 当前 stdin 流读取输入并透传给运行进程。
- **变更理由**：支持 shell pipeline 场景，避免临时文件。
- **影响范围**：`main.rs` stdin 数据读取路径、参数解析测试与文档示例。
- **决策依据**：兼容已有 `--stdin <file>` 行为并最小化改动。

### 2026-02-07 - Phase 4.5：Native 运行超时控制

- **变更内容**：`native --run --timeout <ms>` 支持超时终止执行并返回错误。
- **变更理由**：避免运行阶段长时间卡住，提高 CI 与自动化稳定性。
- **影响范围**：`native.rs` 运行器轮询与 kill、`main.rs` 选项解析、`lib.rs` API 与 tests。
- **决策依据**：通过标准库实现，无额外依赖，兼容现有命令行为。

### 2026-02-07 - Phase 5：AI v1 函数契约子集（intent + ensures）

- **变更内容**：为 `fn` 增加 `intent` 与 `ensures` 语法；补齐 lexer/parser/AST/HIR/Sema 与测试覆盖。
- **变更理由**：让 AI-native 语义声明进入可解析、可校验闭环。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、tests 与文档。
- **决策依据**：保持 Core v0 兼容前提下，优先实现 AI v1 最小高价值子集。

### 2026-02-07 - Phase 5.1：AI v1 失败策略子集（failure）

- **变更内容**：为 `fn` 增加 `failure { condition -> action(...); }` 语法与最小语义校验。
- **变更理由**：补齐 AI-native 合约中的失败处理闭环，支持静态约束检查。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、tests 与文档。
- **决策依据**：先实现可解析可校验最小子集，后续再扩展复合动作与策略语义。

### 2026-02-07 - Phase 5.2：AI v1 证据规范子集（evidence）

- **变更内容**：为 `fn` 增加 `evidence { trace ...; metrics [...] ; }` 语法与语义检查。
- **变更理由**：提供可审计输出契约，强化 AI-native 运行可观测性。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、tests 与文档。
- **决策依据**：优先实现可解析+可校验基础能力，再迭代 artifacts/扩展证据模型。

### 2026-02-07 - Phase 6：AI v1 Workflow 最小子集

- **变更内容**：新增 `workflow` 顶层声明、`steps`/`on_fail`/`output`/`evidence` 最小语法与语义校验。
- **变更理由**：把多步骤 AI 流程纳入静态可验证模型，建立编排级约束闭环。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、tests 与文档。
- **决策依据**：优先支持 step 去重与失败策略合法性，再扩展 SLA/step-call 语义。

### 2026-02-07 - Phase 7：AI v1 Agent 最小子集

- **变更内容**：新增 `agent` 顶层声明与 `state/policy/loop` 语法，落地最小语义校验。
- **变更理由**：将 agent 生命周期与策略约束纳入静态检查，减少运行期策略冲突。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、tests 与文档。
- **决策依据**：先实现结构与冲突检测，再扩展 reachability 和高级 policy 语义。

### 2026-02-07 - Phase 7.1：Agent 语义校验增强

- **变更内容**：新增 state reachability 语义检查（不可达状态 warning）与 policy deny precedence 报告（allow/deny 重叠时 warning）。
- **变更理由**：在保留冲突 error 的同时，提供更可操作的策略解释与状态机质量反馈。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与文档。
- **决策依据**：先以 warning 形式给出静态指导，避免破坏现有最小子集兼容性。

### 2026-02-07 - Phase 7.2：Agent 活性与终止性提示

- **变更内容**：新增 stop condition 状态目标校验（unknown/unreachable warning），并在无 `max_iterations` 且缺乏可达终态时给出 non-termination warning。
- **变更理由**：提前暴露潜在死循环与停机条件配置错误，降低 agent 运行期不可控风险。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与文档。
- **决策依据**：优先以静态 warning 方式提供可操作反馈，保持现有语法与行为向后兼容。

### 2026-02-07 - Phase 7.3：Agent SCC 循环活性校验

- **变更内容**：引入基于 SCC 的可达闭环检测；对无出口且未被 stop state 覆盖的可达循环给出 warning。
- **变更理由**：提升对隐式死循环场景的静态洞察，补齐仅靠终态/stop 检查的盲区。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与文档。
- **决策依据**：在不改变语法的前提下增强语义检查强度，继续保持向后兼容。
