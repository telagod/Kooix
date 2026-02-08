# Kooix MVP Design

## 设计目标

1. 用最小实现验证 **Effect + Capability** 的类型化设计。
2. 保持语法接近 Rust 风格，同时提升 AI 场景语义表达力。
3. 保证前端校验可测试、可扩展，便于后续接入 LLVM backend。

## 非目标

- 不在本阶段实现完整执行语义。
- 不在本阶段实现 JIT/AOT 输出。

## 架构设计

```text
Source (.kooix)
  -> Lexer (Token stream)
  -> Parser (AST)
  -> HIR Lowering
  -> MIR Lowering
  -> Semantic Checker (effect/capability rules)
  -> Interpreter (Kooix-Core subset, optional)
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
- `interp.rs`：Kooix-Core 函数体子集的最小 interpreter（用于 bootstrap/runtime 早期闭环）。
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
  [{ ... }]
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

- 支持函数体与基础表达式 AST 的解析与类型检查（Frontend）；提供最小 interpreter `run`（仅纯函数体子集、禁用 effects），但暂不支持 MIR/LLVM lowering 与完整 runtime/stdlib。
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
- 支持最小 `import` 多文件加载（CLI loader 拼接源文件）；尚无 module/namespace/export 与包管理。

## 兼容与迁移

- 后续可引入 HIR/MIR 层，不破坏当前 AST 接口。
- CLI `check` 命令可作为未来编译流水线前置 stage。

## 语法规范文档

- `docs/Grammar-Core-v0.ebnf`：当前 parser 已实现语法
- `docs/Grammar-AI-v1.ebnf`：AI 原生扩展目标语法
- `docs/Grammar-Mapping.md`：语法与 AST/HIR/MIR/Sema 映射
- `docs/Grammar-Examples.md`：正反例与预期错误类别

## 变更历史

### 2026-02-07 - Kooix MVP 初始落地

- **变更内容**：初始化 Rust workspace，落地 lexer/parser/sema/CLI/tests。
- **变更理由**：将设计方案转为可运行原型，验证核心语义约束。
- **影响范围**：新增 `crates/kooixc` 与顶层文档/示例。
- **决策依据**：优先交付最短可验证闭环。

### 2026-02-07 - Phase 2：HIR 与语义约束增强

- **变更内容**：新增 HIR lowering；增强 capability 参数与 effect 匹配校验；扩充测试与示例。
- **变更理由**：提升类型系统表达力与错误定位精度。
- **影响范围**：`crates/kooixc/src/hir.rs`、`sema.rs`、CLI 与 tests。
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

### 2026-02-07 - Phase 4.6：Timeout 稳定性修复

- **变更内容**：优化超时轮询逻辑（deadline + 二次 `try_wait` 边界确认）；重写 `run_executable_times_out` 为无临时脚本依赖版本，并补充 fast-path 不超时测试。
- **变更理由**：修复历史 flaky 行为，确保 timeout 语义在边界时刻可预测且可复现。
- **影响范围**：`native.rs`、`compiler_tests.rs`、README/CONTRIBUTING。
- **决策依据**：优先消除测试环境差异与临时文件竞争，再增强超时判定稳健性。

### 2026-02-07 - Phase 4.7：Timeout 高可靠性加固

- **变更内容**：补充 kill 失败时的存活校验与错误上抛；新增 timeout/fast-path 重复压测测试（20x）。
- **变更理由**：进一步降低极端调度抖动下的误判与潜在挂死风险。
- **影响范围**：`native.rs`、`compiler_tests.rs` 与 README 测试状态。
- **决策依据**：通过“实现稳健化 + 压测回归”双路径提升可信度。

### 2026-02-07 - Phase 4.8：Windows Timeout 回归覆盖

- **变更内容**：新增 `cmd.exe` 路径下 timeout/fast-path/重复压测测试分支（`#[cfg(windows)]`）。
- **变更理由**：补齐跨平台行为验证，降低仅 Unix 覆盖导致的隐藏回归风险。
- **影响范围**：`compiler_tests.rs` 与设计文档。
- **决策依据**：在不引入新依赖前提下，使用系统 shell 保持测试可执行与可维护。

### 2026-02-08 - Phase 4.9：Timeout 进程树终止加固

- **变更内容**：Unix 下运行进程独立 process group，timeout 时优先 kill 进程组以避免孙进程泄漏；Windows 下 best-effort 调用 `taskkill /T /F` 清理进程树，并保留 `child.kill()` 兜底。
- **变更理由**：提升超时语义的可靠性，避免 shell/launcher 场景下只杀父进程导致残留子进程拖慢 CI 或污染系统。
- **影响范围**：`native.rs` 与 timeout 测试用例。
- **决策依据**：不引入新依赖，采用标准库 + 平台原生能力实现可审计的进程树终止。

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

### 2026-02-07 - Phase 6.1：Workflow 调用目标声明校验

- **变更内容**：新增 step call 目标存在性检查；当调用目标未在顶层声明（`fn`/`workflow`/`agent`）时给出 warning。
- **变更理由**：提前暴露编排层误拼写/漏声明问题，减少运行期才发现调用失效。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先落地“声明级”校验，后续再扩展到签名与类型流校验。

### 2026-02-07 - Phase 6.2：Workflow 调用签名与参数类型校验

- **变更内容**：将 step call 从“仅目标名”扩展为“目标+参数列表”语义节点；新增调用 arity 校验、基础参数类型校验（字符串/数字字面量与 workflow 参数推导）。
- **变更理由**：让 workflow 编排的错误尽可能在静态阶段暴露，减少调用参数错配进入运行时。
- **影响范围**：`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先实现可验证的最小类型流（literal + workflow params），后续再扩展跨 step 变量绑定与复杂表达式推导。

### 2026-02-07 - Phase 6.3：Workflow 跨 Step 符号绑定（最小闭环）

- **变更内容**：step 参数类型推导从“仅 workflow params”扩展为“workflow params + 已声明前序 step id”；支持将前序 step 返回类型作为后续 step 入参参与静态校验。
- **变更理由**：让 workflow 编排具备最小数据流语义，提前暴露 step 链路上的类型错配与前向引用错误。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先落地 root symbol 级别绑定（不做成员投影），以低风险方式推进类型流主线能力。

### 2026-02-07 - Phase 6.4：Workflow Output Contract 类型流校验

- **变更内容**：为 `output { ... }` 增加语义检查：重复字段 error、输出字段类型来源覆盖 warning、返回类型未在 output 合同中暴露 warning。
- **变更理由**：让 workflow 返回合同与内部数据流形成静态闭环，提前发现“有返回类型但输出合同不可达/不一致”的设计缺陷。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先做最小 root-level 类型流校验，不引入表达式绑定语法，保持语法面稳定。

### 2026-02-07 - Phase 6.5：Workflow Output 显式绑定语法（`= symbol`）

- **变更内容**：为 output 字段增加可选显式来源绑定 `field: Type = symbol.path;`；语义层新增绑定符号可达性与类型一致性校验。
- **变更理由**：将 output 合同从“按类型猜测映射”推进到“声明式绑定”，提高 AI 与人类对数据流意图的可读性和可验证性。
- **影响范围**：`ast.rs`、`parser.rs`、`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先支持 root symbol 与保守 member-path warning，不一次性引入复杂投影类型系统。

### 2026-02-07 - Phase 6.6：Member Path 投影类型流（容器子集）

- **变更内容**：为 step 参数与 output 绑定的 `symbol.path` 增加 member 投影推导，支持 `Option/Result/Map/List/Vec/Array` 的最小规则集；未知 member 给出可解释 warning。
- **变更理由**：把“仅 root symbol 可推导”的能力推进到容器级路径表达，提升 workflow 数据流表达力与静态检查精度。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先落地 deterministic 容器规则，暂不引入用户自定义结构体字段系统，控制复杂度与兼容风险。

### 2026-02-07 - Phase 6.7：Output 隐式绑定歧义告警

- **变更内容**：对未显式 `= symbol` 绑定的 output 字段，若同类型匹配到多个来源符号，新增 ambiguity warning 并提示使用显式绑定。
- **变更理由**：避免 output 合同在多候选场景下产生隐式不确定性，让 AI 与人类都能明确数据来源。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：保持向后兼容，不强制报错；通过可解释 warning 引导迁移到显式绑定语法。

### 2026-02-07 - Phase 6.8：Output 同名隐式绑定优先级

- **变更内容**：在 output 未显式绑定时，若字段名与 workflow 作用域符号同名且类型匹配，优先按名称绑定并抑制歧义告警；若同名但类型不匹配，新增可解释 warning。
- **变更理由**：让“字段名即语义”的常见写法更稳定，减少不必要的歧义噪音，并在命名冲突时给出可操作反馈。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：以最小规则提升确定性，不改变显式 `= symbol` 的最高优先级。

### 2026-02-07 - Phase 6.9：Record 声明与字段投影

- **变更内容**：新增顶层 `record` 声明；为 workflow step/output 的 `symbol.path` 类型推导加入用户记录类型字段映射。
- **变更理由**：补齐 AI-native 数据语义主线，让业务对象字段可被静态验证地引用，而不局限于容器内建规则。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先实现最小 record field map（无泛型/方法），优先交付可读可验的字段路径能力。

### 2026-02-07 - Phase 6.10：Record 泛型字段投影（最小子集）

- **变更内容**：在 `record` 声明中支持可选泛型参数（如 `record Box<T>`）；workflow step/output 的 member path 投影支持按实例化类型做字段泛型替换（`Box<Answer>.value -> Answer`）。
- **变更理由**：提升 DSL 数据流表达力，让抽象数据容器在编排层保持强类型可推导。
- **影响范围**：`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先落地“声明 + 投影替换”闭环，不引入约束系统与方法分派；泛型参数数量不匹配时保持 warning 级别兼容。

### 2026-02-07 - Phase 6.11：Record 泛型实参数量静态校验

- **变更内容**：新增 record type arity 全局校验，对 `fn/workflow/agent` 的参数与返回类型、`workflow output` 字段类型、`record` 字段类型中的 record 泛型实参数量不匹配给出 error。
- **变更理由**：把原本只在 member projection 阶段暴露的问题前移到声明阶段，减少隐式降级与后续连锁告警。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：在不引入约束系统的前提下先强化 arity correctness，提升类型系统可靠性与诊断确定性。

### 2026-02-07 - Phase 6.12：Record 泛型约束（Bound）最小子集

- **变更内容**：为 record 泛型参数增加可选 bound 语法（`record Box<T: Answer>`）；新增声明期 bound 校验，并在 member projection 时约束不满足即拒绝投影。
- **变更理由**：让泛型不仅“数量正确”，还具备最小语义约束，进一步降低错误类型进入工作流主链的概率。
- **影响范围**：`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先用单 bound + 兼容既有类型兼容规则实现最小闭环，后续再扩展为 `where`/多约束体系。

### 2026-02-07 - Phase 6.13：Record 多 Bound + where 子句（最小子集）

- **变更内容**：record 泛型参数支持多 bound（`record Box<T: Answer + Summary>`），并支持 `where` 子句追加约束（`record Box<T> where T: Answer + Summary { ... };`）；语义层要求 type arg 同时满足全部 bound。
- **变更理由**：让约束表达从“单一硬约束”升级到“可组合约束”，同时在泛型参数较多时保持声明可读性。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：约束检查保持保守（沿用现有类型兼容规则），不引入 trait 系统与约束求解器，优先交付可验证闭环。

### 2026-02-07 - Phase 6.14：Record-as-Trait 结构化 Bound + 约束诊断收敛

- **变更内容**：当 bound 引用到 record 类型时，按字段子集规则进行结构化满足校验（actual record 至少包含 bound record 的全部字段，且字段类型深度兼容）；bound 校验对多约束做去重归一，并对多失败项给出聚合 error。
- **变更理由**：让 bound 从“名义类型等价”升级到“语义结构约束”，把 AI-native schema 约束落到可验证的静态规则上，同时降低多 bound 场景的诊断噪音。
- **影响范围**：`sema.rs`、`compiler_tests.rs` 与语法映射文档。
- **决策依据**：先以 record schema 作为最小 trait 载体，不引入独立 trait 声明与约束求解器；优先让编排与契约场景可读可验。

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

### 2026-02-08 - Phase 8.0：Kooix-Core 函数体 Frontend（M1）

- **变更内容**：为 `fn` 增加可选函数体 block：`{ ... }`；支持 `let`/`return` 与基础表达式（literal/path/call/`+`/`==`/`!=`）；新增函数体返回类型与调用签名静态校验；为避免误编译，含函数体的程序在 `mir/llvm/native` 阶段直接报错（MIR/LLVM lowering 尚未实现）。
- **变更理由**：把 Kooix 从“声明级 DSL”推进到“可写编译器的 Kooix-Core”起点，为后续 VM/解释器路线与自举（self-hosting）铺路。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`hir.rs`、`sema.rs`、`lib.rs` 与 tests。
- **决策依据**：先落地可解析+可类型检查的最小函数体子集（M1），并用明确门禁阻断后端误用；后续在 M2 引入 VM/解释器或真正 MIR lowering。

### 2026-02-08 - Phase 8.1：Interpreter `run` 最小闭环（M2 起步）

- **变更内容**：新增 `interp.rs` 最小 interpreter（支持 `let`/`return`/基础表达式/函数调用）；新增 CLI 命令 `run`；新增 `run_source` API；新增 `examples/run.kooix` 与解释器测试用例；解释器明确禁止 effects，避免误触发真实 I/O。
- **变更理由**：提供不依赖 LLVM 工具链的可运行闭环，为 bootstrap 的 runtime/stdlib 演进打底，并让语义扩展可以通过运行测试快速验证。
- **影响范围**：`interp.rs`、`lib.rs`、`main.rs`、`compiler_tests.rs`、README 与 grammar/mapping 文档。
- **决策依据**：先解释执行，后字节码/LLVM；优先保证确定性与可审计性，不支持的语义直接报错而非降级忽略。

### 2026-02-08 - Phase 8.2：`if/else` 表达式（类型收敛 + interpreter）

- **变更内容**：为表达式系统增加 `if/else`（以 block 为分支体，`else` 可选）；新增语义规则：条件必须为 `Bool`、分支类型必须收敛；interpreter 支持 `if/else` 执行；补齐 tests 与 grammar/mapping 示例。
- **变更理由**：补齐编译器自举所需的最小控制流能力，为后续 `while/match` 与更复杂的类型推导打基础。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`sema.rs`、`interp.rs`、tests 与文档。
- **决策依据**：先从 `if/else` 起步（最小可验证闭环），并强制类型收敛避免隐式动态类型/运行期分支类型漂移。

### 2026-02-08 - Phase 8.3：`while` + assignment（类型校验 + interpreter）

- **变更内容**：新增 `while` 表达式与赋值语句 `x = expr;`；sema 增加 `while` 条件必须为 `Bool`、assignment 必须写入已声明变量且类型一致；interpreter 引入作用域栈（block 内 `let` 不泄漏、assignment 可更新外层变量），并对 `while` 增加最大迭代次数保护避免非终止挂死。
- **变更理由**：为自举所需的最小“可迭代状态机”能力铺路：仅靠 `if` 很难实现迭代算法/解释器自身的循环执行；同时保持强类型与可审计（assignment 不允许隐式声明或变更类型）。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`sema.rs`、`interp.rs`、`compiler_tests.rs` 与 grammar 示例。
- **决策依据**：先落地最小可用的循环 + 变量更新（仍保持语义简单），后续再引入 `break/continue`、`mut`、更严谨的 borrow/liveness 规则。

### 2026-02-08 - Phase 8.4：record literal + member projection（类型校验 + interpreter）

- **变更内容**：为 Kooix-Core 表达式增加 record literal：`TypeName { field: expr; ... }`；放开函数体中的 member projection（`x.y.z`）并复用 record schema + generic substitution 做静态类型推导；interpreter 增加 record value 运行时表示与 member access；补齐正反例 tests 与 grammar/mapping 示例。
- **变更理由**：写编译器必须能“构造结构化数据 + 读取字段”。没有 record value 与字段访问，Stage1 编译器本体几乎无法落地（AST/TypeRef/Token 等都需要结构化表示）。
- **影响范围**：`ast.rs`、`parser.rs`、`sema.rs`、`interp.rs`、`compiler_tests.rs` 与文档。
- **决策依据**：先落地最小 record value（仅字段存取，不引入方法/trait/生命周期）；用既有 record schema 与投影推导能力复用实现，避免引入新一套类型规则。

### 2026-02-08 - Phase 8.5：enum + match（类型校验 + interpreter）

- **变更内容**：新增 `enum` 声明与 `match` 表达式（pattern: `_`/`Variant(bind?)`，arm body 支持 expr 或 block，arm 分隔符 `=>`）；sema 增加 enum variant 构造类型推导、match arms 类型收敛与穷尽性校验；interpreter 增加 enum value 运行时表示、variant 构造与 match 执行；修复 `if/while/match` 条件/判别式与 record literal 的语法歧义（record literal 需括号包裹）。
- **变更理由**：自举编译器需要代数数据类型（AST/Token/Result/Option）与结构化分支控制流（match）。同时保持强类型与可审计：missing variant 直接 error。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`sema.rs`、`interp.rs`、`compiler_tests.rs` 与语法文档。
- **决策依据**：先落地 enum/match 的最小可用子集，为 Stage1 编译器数据结构与错误处理打基础；更复杂的泛型推导与语法糖（如 `?`）留到后续阶段。

### 2026-02-08 - Phase 8.6：最小 import 多文件加载（include 风格）

- **变更内容**：新增顶层 `import "path";` 声明；CLI 引入 `loader`：按 import 递归加载 `.kooix` 文件并拼接为单一 source，诊断输出尽量定位到原始文件路径与行列号。
- **变更理由**：自举编译器不可避免需要多文件组织（AST/lexer/parser/sema 等模块拆分）；先用 include 风格建立最短闭环，后续再演进为 module/namespace/export。
- **影响范围**：`token.rs`、`lexer.rs`、`ast.rs`、`parser.rs`、`lib.rs`、`loader.rs`、`main.rs` 与语法示例。
- **决策依据**：保持实现可审计（无新依赖），优先提供可用的多文件工作流与更可读的 diagnostics，为后续真正模块系统打底。

### 2026-02-08 - Phase 8.7：stdlib prelude 起步 + call arg expected-type 推导

- **变更内容**：调用表达式（call）在参数逐个校验时，将参数类型作为 expected type 传入表达式类型推导（使 `Some(7)` 之类的泛型 enum variant 在函数参数位置可推导）；新增 `stdlib/prelude.kooix`（`Option/Result/List/Pair`）与 `examples/stdlib_smoke.kooix` 作为可运行 smoke。
- **变更理由**：降低泛型 enum 的使用门槛，为写 Stage1 编译器所需的数据结构（Option/Result/List）铺路。
- **影响范围**：`sema.rs`、`compiler_tests.rs`、`stdlib/`、`examples/` 与 README。
- **决策依据**：保持推导规则最小化（expected-type 驱动），不引入全量 unification/trait 求解器。

### 2026-02-08 - Phase 8.8：enum variant namespacing + 跨 enum 重名放开

- **变更内容**：扩展 enum variant 的构造与 pattern 语法，支持 `Enum.Variant` / `Enum.Variant(payload)` 与 `Enum.Variant(bind?)`；允许跨 enum 同名 variant；当 unqualified variant 出现歧义时给出 error 并要求使用 namespaced 形式；interpreter 引入 qualified/unqualified variant registry 保证运行期可解析。
- **变更理由**：移除“全局 variant 名唯一”限制，避免 stdlib/编译器 AST 被迫使用难读前缀命名；同时让代码语义更显式，AI 读取更像文档。
- **影响范围**：`ast.rs`、`parser.rs`、`sema.rs`、`interp.rs`、`compiler_tests.rs` 与语法文档。
- **决策依据**：先以语法级 namespacing 提供可用的冲突消解手段，后续再演进为 module/namespace/export 体系。
