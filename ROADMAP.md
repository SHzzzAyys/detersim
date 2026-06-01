# ROADMAP.md — 分阶段任务清单(可作为 GitHub Milestones / Issues)

本文件把 `PRD.md` 拆成 6 个阶段。每个阶段对应一个 **Milestone**;每个 `[ ]` 条目对应一个 **Issue**。
每个 Issue 都带"验收标准(DoD)"与"Codex 注意点"。所有阶段的通用完成标准见 `AGENTS.md` §4。

**铁律(贯穿全程)**:`AGENTS.md` §0 的确定性清单 + 确定性元测试必须始终为绿。Phase 0 的第一个 issue 就是把这个 oracle 建起来。

依赖关系:Phase 0 → 1 → 2 → 3 → 4 是线性强依赖;Phase 5 依赖 0–2(熵带/事件流);Phase 6 依赖 0–4。

---

## Phase 0 — 基础与确定性骨架

**目标**:跑起来一个最小的、可逐字节复现的单线程仿真。先有 oracle,再有一切。

- [ ] **0.1 初始化 Cargo workspace 与 crate 骨架**
  - 内容:按 `PRD.md` §5.1 建立 `crates/*` 空骨架、`examples/pingpong`、`tests/`、`just`/脚本、CI(fmt + clippy `-D warnings`)。
  - 验收:`cargo build --workspace` 通过;`cargo clippy --workspace -- -D warnings` 通过。

- [ ] **0.2 定义 `detersim-core` 的核心 trait**
  - 内容:实现 `SimTime`、`Clock`、`Rng`、`Network`、`Storage`、`Spawn`、`Env`(契约见 `PRD.md` §6)。先只放接口与文档,可暂用 `todo!()` 实现体。
  - 验收:`core` 编译;每个 public item 有 rustdoc;`core` 不依赖 tokio,不依赖任何会起线程/读系统熵的库。
  - Codex 注意点:futures **不要** `Send` 约束;`Network` 默认是不可靠无序数据报。

- [ ] **0.3 确定性 `Rng` + 状态分裂**
  - 内容:基于 ChaCha8 或 SplitMix64 实现 `Rng`,含 `fork()`(派生独立确定性子流)。
  - 验收:property test——同种子两实例产生相同序列;`fork` 出的子流与父流统计独立但确定。

- [ ] **0.4 逻辑时钟 + 事件优先队列**
  - 内容:`SimTime`、`Event { time, seq, kind }`、按 `(time, seq)` 的最小堆;`seq` 单调递增计数器(见 `PRD.md` §7.1)。
  - 验收:property test——堆弹出顺序是 `(time, seq)` 的确定性全序;同插入序列产生同弹出序列。

- [ ] **0.5 单线程协作式 executor + waker**
  - 内容:任务存于 slab(`Pin<Box<dyn Future<Output=()>>>`);waker 唤醒 = 入队 `PollTask`;事件循环 = pop → 推进时钟 → dispatch(见 `PRD.md` §8)。先支持 `Clock::sleep` 与 `Spawn`。
  - 验收:`examples/pingpong`(两任务通过 sleep 交替)能跑到 quiescence;取消/drop 不留幽灵 timer。
  - Codex 注意点:正确实现死锁检测(堆空但有任务 parked)与时间 horizon 上限。

- [ ] **0.6 EntropyTape 雏形(generate / replay)**
  - 内容:`EntropyTape::draw(label)`;generate 模式取种子 PRNG 并记录,replay 模式回放(见 `PRD.md` §7.2)。
  - 验收:同一 tape 在 replay 下产生与 generate 时相同的抽取序列。

- [ ] **0.7 ⭐ 确定性元测试(主 oracle,最高优先级)**
  - 内容:`assert_deterministic!`——对同一 seed 跑两遍 `pingpong`,断言**事件日志逐字节相同**;覆盖"进程内重复"与"进程重启之间"(后者可序列化日志后比对)。
  - 验收:在 ≥10,000 个随机 seed 上逐字节一致;接入 CI 与 `seed_soak`。
  - Codex 注意点:这是后续每个 PR 的门禁,务必稳定、可重放、打印失败 seed。

**Phase 0 退出标准**:pingpong 在 10k seed 上逐字节可复现;CI 含确定性 lint 门禁 + 元测试。

---

## Phase 1 — 网络与 actor 模型

**目标**:多节点、节点寻址的不可靠消息传递,延迟由熵带驱动,可观察到乱序。

- [ ] **1.1 World + 多节点生命周期**
  - 内容:`World::new(seed).add_nodes(n, run_fn).run()`;为每个节点注入带 `node_id` 的 `SimEnv`(各自 fork 的 RNG)。
  - 验收:N 节点各自独立运行;元测试在多节点下仍逐字节一致。

- [ ] **1.2 仿真网络介质 + `Network` 实现**
  - 内容:`send` = 经熵带抽取延迟后入队 `DeliverMsg`;`recv` = 从节点 inbox 取并在空时挂起。
  - 验收:`examples` 里一个 N 节点 echo/gossip 能端到端通信;不同 seed 下可观察到投递乱序;元测试绿。

- [ ] **1.3 ClockExt::timeout 组合子**
  - 内容:基于 `sleep` + select 的超时组合子。
  - 验收:超时在仿真时间下正确触发;被超时的分支正确取消、不留幽灵事件。

**Phase 1 退出标准**:多节点不可靠消息系统可跑、可复现;乱序可观测。

---

## Phase 2 — 故障注入(nemesis)

**目标**:网络/时钟类故障,全部走熵带,可复现可重放。

- [ ] **2.1 ConnectivityMatrix + 分区**
  - 内容:连通矩阵(支持非对称、多组);`DeliverMsg` 投递前检查连通性;`partition`/`heal_all`(见 `PRD.md` §7.3、§9.1)。
  - 验收:注入双向/非对称分区后消息按预期被拦截;限时分区自愈;元测试绿。

- [ ] **2.2 丢包 / 重复 / 额外延迟**
  - 内容:按熵带 `gen_bool` 丢弃投递;小概率重复入队;延迟抖动。
  - 验收:可配置开关与配比;trace 中可见这些事件;同 seed 复现。

- [ ] **2.3 时钟偏移(skew)**
  - 内容:每节点时钟偏移量,使各节点 `now()` 不一致但各自单调。
  - 验收:依赖时间假设的玩具逻辑能因 skew 暴露问题;元测试绿。

- [ ] **2.4 NemesisPlan 接口 + 内置策略**
  - 内容:`NemesisPlan::next_action`;内置 `RandomPartition`、`ClockSkew`、`Composite`(见 `PRD.md` §9.5)。
  - 验收:可组合多策略;全部决策经熵带 ⇒ 可重放。

**Phase 2 退出标准**:网络/时钟故障可注入、可复现;有第一个 plant-a-bug(如"分区下脑裂")被稳定复现。

---

## Phase 3 — 存储与崩溃重启

**目标**:磁盘抽象 + 五类失效 + 进程崩溃/重启,测试崩溃一致性(本框架最硬核的故障语义之一)。

- [ ] **3.1 PageStore + `Storage` 实现**
  - 内容:页存储,`committed`/`pending` 两层;`write_at`/`read_at`/`flush`/`len`(见 `PRD.md` §7.4)。
  - 验收:flush 前能读到自己的写;flush 后进入 committed;元测试绿。

- [ ] **3.2 磁盘失效模型**
  - 内容:latency、lost-on-crash、torn write、bit rot、pre-fsync reorder,均由熵带/nemesis 驱动。
  - 验收:`examples/wal`(玩具 WAL)在 bit rot 下能被 checksum 检出;各失效可单独开关。

- [ ] **3.3 进程崩溃 / 重启语义**
  - 内容:崩溃 = drop 节点全部任务(易失态丢失)+ 仅保留已 flush 数据 + 触发 lost/torn;重启 = 重跑 init 并从磁盘恢复(见 `PRD.md` §9.4)。
  - 验收:`wal` 在"写入中途崩溃 → 重启"后满足恢复不变量;崩溃重启循环模式可用;无幽灵 waker/timer 泄漏。
  - Codex 注意点:崩溃时清理该节点在事件堆中的所有未决事件,否则破坏确定性与正确性。

**Phase 3 退出标准**:崩溃恢复可被测试;plant-a-bug"未 fsync 就 ack 导致丢数据"被稳定复现。

---

## Phase 4 — 不变量 + 线性一致性检查器

**目标**:从"系统能跑"升级到"系统是对的"。

- [ ] **4.1 在线不变量 hook**
  - 内容:`check_invariants(&self)` 在静止点/周期性调用;提供常用断言辅助。
  - 验收:`examples` 里一个故意违反不变量的 SUT 被即时捕获并打印 seed。

- [ ] **4.2 History 记录 + Model trait**
  - 内容:`OpRecord`(invoke/complete/input/output)记录;用户 `Model::{init,step}`(见 `PRD.md` §7.5)。
  - 验收:`examples/kv-register` 能产出可供检查的历史。

- [ ] **4.3 线性一致性检查器(Porcupine / WGL)**
  - 内容:WGL 线性化搜索 + Lowe 优化 + bitset 记忆化;输出见证顺序或反例(见 `PRD.md` §10.2)。
  - 验收:故意有 bug 的 KV(如丢更新)被判**不可线性化**并给出反例;正确 KV 通过;有界历史下性能可接受(含超时降级)。
  - Codex 注意点:这是 NP 难搜索,务必先小规模正确,再做剪枝与记忆化;对不返回(in-flight)操作正确处理。

- [ ] **4.4 弱一致性检查器(顺序 / 因果)**
  - 内容:sequential、causal 各一个检查器。
  - 验收:各配一个通过样例 + 一个违反样例。

**Phase 4 退出标准**:线性一致性检查器在 plant-a-bug KV 上召回 100%;正确实现不误报。

---

## Phase 5 — Trace 工具 + 最小化

**目标**:把失败用例变成人类可读的最小可复现案例(加分项;seed 复现已是底线)。

- [ ] **5.1 Trace 导出(JSON)**
  - 内容:把事件日志 + nemesis 动作 + 历史导出为结构化 JSON。
  - 验收:任一失败运行可导出完整 trace;字段足以重放。

- [ ] **5.2 熵带最小化(shrinking)**
  - 内容:在保持失败前提下 shrink 熵带 + 缩减工作负载;Hypothesis 式结构化内部 shrinking 处理 cursor 错位(见 `PRD.md` §11.2)。
  - 验收(shrinking soundness):最小化结果**仍复现原失败**;最小化尽量单调;有时间预算上限。

- [ ] **5.3 因果感知 delta-debugging**
  - 内容:删事件尊重 happens-before(不投递未发送的消息)。
  - 验收:最小化产物在因果上自洽、可重放。

- [ ] **5.4 时序可视化(HTML viewer)**
  - 内容:JSON → 泳道时序图(节点为泳道、消息为带箭头线、故障点高亮)。
  - 验收:能渲染一个最小失败用例的时序图。
  - 注意:viewer 为本地静态 HTML,不调用任何外部服务。

**Phase 5 退出标准**:一个"大"失败用例能被 shrink 成最小用例且仍复现;可视化可用。

---

## Phase 6 — 验证与 stretch

**目标**:用真实算法验证整套框架的价值;开启可选高级方向。

- [ ] **6.1 ⭐ 参考实现:VSR 或 Raft 写在 `Env` 之上**
  - 内容:`examples/vsr` 实现一个玩具但完整的复制协议,仅通过 `Env` 接触世界。
  - 验收:框架在 seed 预算内**重新发现注入的每一类 bug**;**理想情况下找到一个真实的细微 bug**。这是项目级成功的核心证据(`PRD.md` §16.1)。

- [ ] **6.2 覆盖率引导的 seed 选择(stretch)**
  - 内容:收集执行覆盖(状态/分支),反馈以偏置 seed 探索(类 fuzzing)。
  - 验收:在同等预算下比纯随机更快命中已知 bug。

- [ ] **6.3 socket 风格可靠有序流 API(stretch)**
  - 内容:在不可靠数据报之上提供 connect/listen/stream,给 TCP/QUIC 类系统用。
  - 验收:一个基于流的玩具系统可在框架内运行并被注入故障。

- [ ] **6.4 事务一致性检查器 Elle 风格(stretch)**
  - 内容:依赖图环检测,判定串行化/快照隔离(`PRD.md` §10.4)。
  - 验收:一个违反 SI 的样例被检出。

- [ ] **6.5 透明运行时拦截(stretch,降低存量接入门槛)**
  - 内容:madsim 式 cfg 拦截作为**可选层**,使部分存量 tokio 代码少改造即可仿真。
  - 验收:一个原本用 tokio 的小项目在拦截层下可被仿真;拦截层不削弱 capability-passing 核心的确定性保证。

**Phase 6 退出标准**:VSR/Raft 验证通过;README + 教程 + 各 crate rustdoc 完整;新用户 30 分钟内可接入(`PRD.md` §16.3)。

---

## 附:建议的 Issue 模板

```md
## 目标
（一句话:这个 issue 让框架获得什么能力）

## 范围 / 不做
- 做:...
- 不做:...（避免范围蔓延)

## 实现要点
- 参考 PRD §X、AGENTS §Y
- 关键数据结构 / 接口:...

## 验收标准(DoD)
- [ ] 通用门禁(AGENTS §4):build / clippy / 确定性元测试 / 10k seed soak
- [ ] 本 issue 专属:...
- [ ] 若涉及故障子系统:对应 plant-a-bug 被稳定复现

## 复现
失败打印 `DST_SEED=<n>`;`DST_SEED=<n> cargo test <name>` 复现
```
