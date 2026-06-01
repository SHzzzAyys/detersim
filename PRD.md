# 确定性仿真测试框架(DST Framework)— 产品与技术规格书

> 代号:`detersim`(暂定,可改)
> 版本:v0.1(规格草案)
> 实现语言:Rust(方法论与语言无关,但本规格以 Rust 为参考实现)

---

## 0. 文档说明 / 如何使用这套文档

本仓库交付物由三份文档组成,分工如下:

- **`PRD.md`(本文档)**:讲清楚"做什么、为什么这么做、核心契约长什么样"。是设计的单一事实来源(source of truth)。
- **`AGENTS.md`**:给编码 Agent(Codex)的工作守则——铁律、构建/测试命令、Definition of Done、禁止事项。每个 PR 前必读。
- **`ROADMAP.md`**:把 PRD 拆成 6 个阶段的可执行 issue 清单,每个 issue 带验收标准。

语言约定:散文用中文;所有类型名、函数签名、命令、标识符用英文;`AGENTS.md` 全英文(开源仓库 + Agent 友好)。

阅读顺序建议:第 1→3→4 节先建立心智模型(尤其第 3 节"不确定性来源清单"和第 4 节"三个关键架构决策"),再看第 6/7 节接口与数据结构,最后看第 12 节测试策略——它解释了为什么这个项目"可被 Agent 正确地构建"。

---

## 1. 愿景、目标与非目标

### 1.1 一句话愿景

让任何分布式 / 并发系统的作者,以很低的成本,在**单线程、完全可复现**的仿真中注入真实失效(网络分区、消息乱序丢失重复、时钟偏移、磁盘撕裂写/位翻转、进程崩溃重启),自动检查一致性不变量,并在失败时把问题收敛成最小可复现用例。

### 1.2 目标(In Scope)

1. **确定性运行时**:一组 `Env` trait + `SimEnv` 实现,保证"同一 seed → 逐字节相同的执行"。
2. **故障注入引擎(nemesis)**:网络/时钟/磁盘/进程四类失效模型,语义正确、可配置、全部走熵带。
3. **一致性检查**:在线不变量 hook;离线线性一致性检查器(Porcupine/WGL 风格);弱一致性模型库;事务模型(stretch)。
4. **可复现与最小化**:seed 级复现(必达);熵带级 shrinking(Phase 5);因果感知的 delta-debugging;失败时序可视化。
5. **可被 Codex 高质量构建**:每一阶段都有客观的 oracle(见第 12 节),使 Agent 的产出可被自动验证。
6. **教学与生产兼顾**:干净的接口、充分的文档、可读的参考实现,降低 DST 方法论的入门门槛。

### 1.3 非目标(Out of Scope,至少 v1)

- **不重新实现底层密码学原语 / TLS / 共识算法本身**:框架是"测试工具",不是被测系统。参考实现(用于自检)会写一个玩具共识,但那是验证手段,不是产品。
- **不做硬件/hypervisor 级确定性**(那是 Antithesis 的路线,闭源、重型)。我们做库级、语言惯用的开源方案。
- **不追求对存量代码零改造**(透明拦截放到 Phase 6 当可选层)。v1 面向"新写的、或愿意按 `Env` 抽象组织的"系统。
- **不做真实集群的黑盒测试**(那是 Jepsen 的领域,与我们互补;我们借用它的检查器思路)。
- **不做跨平台/跨语言的字节级一致**(浮点与平台差异;v1 的确定性保证在"同一二进制 + 同一平台"成立,跨平台一致性作为已知限制记录)。

---

## 2. 背景:什么是 DST,为什么难,以及现有方案的空白

**Deterministic Simulation Testing(DST)** 的核心思想:把一个本应并发、依赖真实时间与 IO 的系统,运行在一个**可控的、单线程的、逻辑时间驱动的仿真**里。因为整个执行由一个种子(seed)决定,所以:

- 任意被发现的 bug 都能用 seed 100% 复现(对比真实并发测试的"偶现、抓不住");
- 可以在仿真里廉价地注入海量、极端的失效组合,把系统逼到现实中罕见但致命的状态;
- 可以加速时间(逻辑时钟瞬间跳跃),一秒钟跑完现实中几小时的超时与重试。

**为什么极难(也正是它对 Codex 推理能力的考验):**

1. 必须系统性消灭一切隐藏的不确定性来源(见第 3 节),这要求对整个运行时的副作用建模。
2. 调度器要在单线程上忠实地模拟并发,并严格保证可复现。
3. 每一类故障都要符合真实失效模型,否则测出的 bug 不可信。
4. 失败 trace 最小化是带因果约束的组合搜索;线性一致性检查本身是 NP 难的图搜索。

**现有方案与空白:**

| 项目 | 语言 | 路线 | 局限 |
|---|---|---|---|
| FoundationDB Flow | C++ | 自定义语言+运行时,DST 鼻祖 | 与 FDB 强绑定,不可复用 |
| TigerBeetle VOPR | Zig | 整个系统写在抽象 IO 之上 | 单系统的 bespoke 测试,非通用框架 |
| Antithesis | 闭源 | hypervisor 级确定性 | 商业、闭源 |
| **madsim** | Rust | 编译期拦截 tokio API | 拦截法脆弱、与 tokio 强耦合 |
| **turmoil**(tokio 团队) | Rust | 确定性网络仿真 | 偏网络,磁盘/崩溃建模弱 |
| **shuttle**(AWS) | Rust | 受控调度做共享内存并发测试 | 面向 lock-free,非分布式/IO 故障 |
| **loom** | Rust | 穷举内存序模型检查 | 有界、面向并发原语 |
| **Jepsen** | Clojure | 真实集群黑盒 + 故障 + Elle/Knossos 检查 | 非确定性、外部部署(与我们互补) |

**空白(本项目的贡献):没有一个开源、通用、文档良好、语言惯用的 DST *框架*。** 现状要么是 per-project bespoke 测试臂(VOPR/Flow),要么是窄域网络仿真(turmoil),要么是别的并发模型(shuttle/loom)。我们要把 VOPR 这套方法论做成可复用的库。

---

## 3. 第一性原理:不确定性的来源与消除清单

确定性不是"碰巧得到"的,而是"工程出来"的:**让每一个非确定性来源都走一道可注入的接缝(seam)。** 这是整个框架成立的根基,Codex 必须把这张表当作检查清单。

| # | 不确定性来源 | 真实世界的表现 | 在框架中的消除方式 |
|---|---|---|---|
| 1 | 时间 | 墙钟/单调钟、sleep、timeout | `Clock` trait;仿真里时间是逻辑的,仅由调度器推进 |
| 2 | 并发调度 | OS 线程抢占顺序不可控 | 全部跑在单 OS 线程上;协作式调度;在 await 点让出 |
| 3 | 随机数 | `thread_rng`、UUID v4 | `Rng` trait,种子化;主 seed 派生所有子流 |
| 4 | 网络 | 时序、乱序、丢包、重复 | `Network` trait;仿真介质由调度器 + nemesis 控制 |
| 5 | 磁盘/存储 | IO 完成顺序、fsync 前可见性、部分写 | `Storage` trait;PageStore 建模含失效 |
| 6 | 哈希迭代顺序 | `HashMap` 默认随机种子的 SipHash | **核心 bug 源**:核心路径与 SUT 禁用 `HashMap` 迭代;用 `BTreeMap`/`IndexMap` 或固定种子 hasher |
| 7 | 指针/地址依赖 | ASLR 导致地址每次不同 | 禁止按地址哈希/排序;禁止依赖指针 `Ordering` |
| 8 | 外部熵 | 全局可变状态、relaxed 原子的跨线程观测、环境变量 | 禁止全局可变状态;单线程下无需原子;配置显式注入 |
| 9 | 浮点 | 非结合性 + 平台差异 | v1 限定"同一二进制+平台"成立;跨平台一致列为已知限制 |
| 10 | 集合容量/分配 | 因分配地址不同导致行为差异 | 不依赖分配地址;迭代顺序只能来自确定性容器 |

**落地手段(在 CI 中强制,见 `AGENTS.md`):** 用 clippy lint + grep 在 `core` crate 与示例 SUT 中禁止 `std::time::*::now`、`std::net`、`tokio::spawn`、`std::thread`、`rand::thread_rng`、`HashMap`/`HashSet` 的迭代等。

---

## 4. 三个关键架构决策(Trade-off 显式化)

### 决策 A:Capability-passing(能力传递)作为核心,拦截法作为可选层

- **方案**:SUT 对 `Env` 泛型化,`fn run<E: Env>(env: E, ...)`。生产注入 `RealEnv`(tokio + std),测试注入 `SimEnv`。
- **为什么**:
  - 确定性变成**结构性、可静态检查**的性质(SUT 只能通过 `E` 接触世界);
  - 比拦截法**更难被悄悄破坏**,更好维护;
  - **教学价值**高,接口即文档;
  - 与 madsim 的拦截路线**差异化**。
- **代价**:SUT 必须按 `Env` 抽象来写(新代码是优点,存量改造是成本)。
- **缓解**:提供清晰的迁移指南;把"madsim 式透明拦截"放到 Phase 6,作为降低存量接入门槛的可选层。

> 这是最 load-bearing 的决策。如果改成拦截优先,确定性保证会从"结构性"退化为"靠拦截覆盖度",而拦截覆盖度难以验证——这正是会让 Codex 写出"看起来对、实则偶发不确定"代码的陷阱。

### 决策 B:统一的事件优先队列 + 熵带,而非"重排就绪集"

- **并发即事件**:所有并发都表达为单一优先队列里的事件,键为 `(SimTime, seq)`,`seq` 为插入时单调递增计数器。堆的弹出顺序因此是**插入顺序与时间的确定性函数**。
- **确定性来源**:堆序确定 ⇒ 只要所有时间/延迟来自种子化熵带,整体执行就确定。**不需要**对就绪任务做随机重排。
- **交织探索来源**:事件的延迟(网络延迟、唤醒抖动)由熵带的 PRNG 抽取决定 ⇒ 不同 seed → 不同时间 → 不同相对顺序 → 不同交织。
- **熵带(EntropyTape)**:一条 append-only 的控制面随机抽取序列(延迟、丢/留、分区翻转、崩溃点、调度抖动)。两种模式:
  - **Generate**:抽取来自种子化 PRNG 并追加到带上;
  - **Replay**:抽取从给定带读取(便于为 shrinking 改写带后重放)。
- **SUT 自身随机**通过 `env.rng()` 取一条**fork 出的独立子流**:也是确定性的,但与控制面熵带概念上分离。

### 决策 C:单线程 ⇒ futures 不要求 `Send`

- 一切在单 OS 线程执行,因此 `SimEnv` 的 future **无需 `Send`/`Sync` 约束**。这消除了一整类异步 Rust 的摩擦,且语义正确。
- `JoinHandle`、内部 waker、任务存储都按 `!Send` 单线程模型设计。

---

## 5. 系统架构总览

### 5.1 模块 / crate 划分(Cargo workspace)

```
detersim/
├── Cargo.toml                # workspace
├── AGENTS.md
├── PRD.md
├── ROADMAP.md
├── crates/
│   ├── detersim-core/        # Env/Clock/Rng/Network/Storage/Spawn traits + 公共类型
│   │   ├── env.rs            #   trait Env 及关联类型
│   │   ├── clock.rs
│   │   ├── rng.rs
│   │   ├── net.rs
│   │   ├── storage.rs
│   │   ├── spawn.rs
│   │   └── time.rs           #   SimTime / Duration newtype
│   ├── detersim-sim/         # SimEnv:确定性调度器 + 仿真介质 + 熵带
│   │   ├── scheduler.rs      #   事件堆 + 协作式 executor + waker
│   │   ├── tape.rs           #   EntropyTape(generate/replay)
│   │   ├── net_sim.rs        #   仿真网络介质 + ConnectivityMatrix
│   │   ├── storage_sim.rs    #   PageStore + 磁盘失效模型
│   │   ├── world.rs          #   World:节点集合、崩溃/重启、生命周期
│   │   └── trace.rs          #   事件记录(用于 determinism meta-test 与可视化)
│   ├── detersim-real/        # RealEnv:tokio + std 的生产实现
│   ├── detersim-nemesis/     # 故障注入引擎(网络/时钟/磁盘/进程策略)
│   ├── detersim-check/       # 一致性检查:在线不变量 + 线性一致性(Porcupine/WGL)+ 弱模型
│   ├── detersim-shrink/      # 熵带最小化 + 因果感知 delta-debugging(Phase 5)
│   ├── detersim-viz/         # 失败时序图导出(JSON + HTML viewer)(Phase 5)
│   └── detersim-testkit/     # 给用户写测试用的便捷宏与 harness:run_sim!/assert_deterministic!
├── examples/
│   ├── pingpong/             # Phase 0/1 玩具
│   ├── kv-register/          # Phase 4 线性一致性目标
│   ├── wal/                  # Phase 3 崩溃恢复目标
│   └── vsr/                  # Phase 6 参考实现(Viewstamped Replication / Raft)
└── tests/                    # 跨 crate 集成 + determinism meta-test + plant-a-bug 套件
```

### 5.2 数据流(一次仿真运行)

```
seed ──► EntropyTape(generate) ──► SimEnv ──┐
                                            │   被测系统(SUT)对 Env 泛型化
World 配置(N 节点 / 工作负载 / nemesis 计划)│   只通过 Env 接触世界
                                            ▼
                         ┌──────────── Scheduler(单线程事件循环)────────────┐
                         │  pop 最早事件 → 推进逻辑时钟 → 唤醒/poll 对应任务   │
                         │  ▲ 网络延迟/丢包/分区、磁盘延迟/撕裂、崩溃点         │
                         │  └─ 全部从 EntropyTape 取(经 Nemesis)            │
                         └───────────────────────────────────────────────────┘
                                            │
              运行时:记录 Trace(事件日志)+ History(客户端操作)
                                            ▼
                    退出后:在线不变量 + 离线一致性检查器
                                            ▼
            失败? ──否──► 通过(记录覆盖率,可反馈给 seed 选择)
              │是
              ▼
   打印失败 seed(必达复现) ──► Phase5:熵带 shrinking ──► 最小可复现用例 + 时序图
```

---

## 6. 核心接口定义(目标契约)

> 以下 Rust 签名是 **契约**,Codex 应据此实现 `detersim-core`。允许在保持语义的前提下微调(如生命周期、`async fn in trait` 的封装方式)。所有 `async fn` 在单线程模型下**不要求 `Send`**。

### 6.1 时间

```rust
// time.rs
/// 逻辑时间,自仿真起点起的纳秒数。单调递增。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SimTime(pub u64);

impl SimTime {
    pub const ZERO: SimTime = SimTime(0);
    pub fn saturating_add(self, d: core::time::Duration) -> SimTime { /* ns 相加,饱和 */ }
}
```

### 6.2 Clock

```rust
// clock.rs
pub trait Clock: Clone {
    /// 当前逻辑时间。一次运行内单调。
    fn now(&self) -> SimTime;

    /// 挂起当前任务,直到至少 `dur` 逻辑时间流逝。
    async fn sleep(&self, dur: core::time::Duration);

    /// 挂起当前任务,直到逻辑时间到达 `deadline`(已过则立即返回)。
    async fn sleep_until(&self, deadline: SimTime);
}

/// 组合子:超时。作为扩展 trait 提供(默认实现基于 sleep + select)。
pub trait ClockExt: Clock {
    async fn timeout<F: core::future::Future>(
        &self, dur: core::time::Duration, fut: F,
    ) -> Result<F::Output, Timeout>;
}
```

### 6.3 Rng

```rust
// rng.rs
/// 确定性随机源。实现必须是可复现的(如 ChaCha8 / SplitMix64 + 状态分裂)。
pub trait Rng {
    fn next_u64(&mut self) -> u64;
    fn gen_bool(&mut self, p: f64) -> bool;
    /// 半开区间 [lo, hi)。
    fn gen_range_u64(&mut self, lo: u64, hi: u64) -> u64;

    /// 派生一条与当前流统计独立、但确定性的子流。
    /// 关键:给每个节点/任务独立子流,使"别处新增任务"不扰动此处序列,
    ///       从而提升 shrinking 的稳定性。
    fn fork(&mut self) -> Self where Self: Sized;
}
```

### 6.4 Network(节点寻址、不可靠、无序)

```rust
// net.rs
pub type NodeId = u32;

/// 默认语义:不可靠、无序的数据报。介质可能丢弃/重复/延迟/乱序。
/// 这是"诚实的原语"——可靠性与有序性由 SUT 自行构建(并因此被测试)。
pub trait Network {
    async fn send(&self, to: NodeId, msg: bytes::Bytes);
    /// 接收下一个投递到本节点的数据报,返回 (来源, 内容)。
    async fn recv(&self) -> (NodeId, bytes::Bytes);
}
```

> 设计说明:从"不可靠数据报"起步是刻意的——它强迫 SUT 写出正确的重传/去重/排序逻辑,而这些正是要被故障注入考验的地方。可靠有序流(socket 风格 connect/listen)作为上层在 Phase 6 提供(给 QUIC 这类系统用)。

### 6.5 Storage(块/页设备,含 fsync 语义)

```rust
// storage.rs
pub type StorageResult<T> = Result<T, StorageError>;

pub trait Storage {
    async fn write_at(&self, offset: u64, data: &[u8]) -> StorageResult<()>;
    async fn read_at(&self, offset: u64, buf: &mut [u8]) -> StorageResult<usize>;

    /// 使此前所有写入持久化。**只有 flush 过的数据能在崩溃后存活。**
    async fn flush(&self) -> StorageResult<()>;

    async fn len(&self) -> u64;
}
```

### 6.6 Spawn 与 JoinHandle

```rust
// spawn.rs
pub trait Spawn {
    /// 注意:无 Send 约束(单线程)。
    fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: core::future::Future + 'static;
}

pub struct JoinHandle<T> { /* 任务句柄;await 得到输出;abort() 取消 */ }
```

### 6.7 Env(聚合)

```rust
// env.rs
/// SUT 唯一允许的"世界入口"。SUT 写成 `fn run<E: Env>(env: E, ...)`。
pub trait Env: Clone + 'static {
    type Clock: Clock;
    type Rng: Rng;
    type Net: Network;
    type Storage: Storage;

    fn clock(&self) -> Self::Clock;
    /// 取一条 fork 出的独立确定性 RNG 流(SUT 自身随机用)。
    fn rng(&self) -> Self::Rng;
    fn net(&self) -> Self::Net;
    fn storage(&self) -> Self::Storage;

    fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: core::future::Future + 'static;

    /// 当前节点 id(SimEnv 下由 World 注入;RealEnv 下来自配置)。
    fn node_id(&self) -> NodeId;
}
```

**SUT 编写范式(用户视角):**

```rust
async fn run_node<E: Env>(env: E) {
    let clock = env.clock();
    let net = env.net();
    loop {
        let (from, msg) = net.recv().await;
        // ...处理,可能 sleep/send/读写 storage...
        clock.sleep(Duration::from_millis(50)).await;
        net.send(from, reply).await;
    }
}

// 生产:run_node(RealEnv::new()).await;
// 测试:World::new(seed).add_nodes(5, run_node).run();
```

---

## 7. 核心数据结构

### 7.1 事件与事件堆

```rust
struct Event {
    time: SimTime,
    seq:  u64,        // 插入时单调递增 ⇒ (time, seq) 是确定性全序
    kind: EventKind,
}

enum EventKind {
    PollTask(TaskId),                 // 唤醒并 poll 某任务一次
    DeliverMsg { to: NodeId, from: NodeId, msg: Bytes },
    TimerFire(TimerId),               // sleep/timeout 到期
    StorageComplete(IoId, StorageResult<()>),
    Nemesis(NemesisAction),           // 注入故障(分区翻转、崩溃等)
}

// 最小堆,按 (time, seq) 排序;弹出顺序确定。
struct EventQueue(BinaryHeap<Reverse<(SimTime, u64, EventId)>>);
```

### 7.2 熵带 EntropyTape

```rust
enum TapeMode { Generate(ChaCha8Rng), Replay { tape: Vec<u64>, cursor: usize } }

struct EntropyTape {
    mode: TapeMode,
    log:  Vec<u64>,   // 记录每一次控制面抽取(generate 模式追加;replay 模式回显)
}

impl EntropyTape {
    /// 控制面的每一次随机决策都经此函数,带可读 label(便于 trace 与 shrinking 定位)。
    fn draw(&mut self, label: TapeLabel) -> u64 { /* generate: 取 PRNG 并 push;replay: 读 cursor */ }
}
```

> shrinking 难点(诚实记录):移除一次控制决策会使下游 cursor 错位 ⇒ 重放脱同步。Phase 5 采用 Hypothesis 式"结构化/带跳过的内部 shrinking";这是研究级难题。**seed 级复现(总是可用)是头号卖点,熵带 shrinking 是加分项,不可过度承诺。**

### 7.3 连通矩阵 ConnectivityMatrix

```rust
struct ConnectivityMatrix {
    n: usize,
    can_deliver: Vec<bool>, // [from * n + to];支持非对称分区(A→B 通而 B→A 断)
}
impl ConnectivityMatrix {
    fn can(&self, from: NodeId, to: NodeId) -> bool;
    fn set(&mut self, from: NodeId, to: NodeId, ok: bool);
    fn partition(&mut self, group_a: &[NodeId], group_b: &[NodeId]); // 双向切断两组
    fn heal_all(&mut self);
}
```

### 7.4 PageStore(磁盘失效建模)

```rust
struct PageStore {
    page_size: usize,
    committed: Vec<Page>,            // 已 flush,崩溃后存活
    pending:   BTreeMap<usize, Page>,// 已写未 flush
}
// 失效模型(均由 nemesis/熵带驱动):
//  - latency:完成事件延迟
//  - lost-on-crash:崩溃时丢弃 pending(模拟页缓存未落盘)
//  - torn write:多页写在崩溃时仅落一部分页
//  - bit rot:对 committed 页随机翻位(模拟静默损坏 ⇒ 测试 checksum)
//  - pre-fsync reorder:flush 前写入对外可见顺序可被打乱(测试存储引擎的持久化纪律)
```

### 7.5 一致性检查的历史记录

```rust
struct OpRecord {
    process:  u32,
    invoke:   SimTime,
    complete: Option<SimTime>, // None = 运行结束时仍在飞行中
    input:    Value,
    output:   Option<Value>,
}

/// 用户提供的顺序规约(sequential specification)。
trait Model {
    type State: Clone;
    fn init() -> Self::State;
    /// 在给定状态上应用一个操作,返回新状态与"该操作本应产生的输出"。
    fn step(state: &Self::State, input: &Value) -> (Self::State, Value);
}
```

---

## 8. 确定性调度器设计(`detersim-sim/scheduler.rs`)

### 8.1 事件循环(伪代码)

```
loop {
    // 1) 把当前所有"就绪"任务作为当前时间(+可选 PRNG 抖动)的 PollTask 事件入堆
    //    —— 但注意:唤醒本身就是通过入堆完成的,见 8.2
    if event_queue.is_empty() {
        if any_task_parked() { report_deadlock(); }  // 无事件却有任务挂起 ⇒ 死锁
        break; // quiescence:世界已静止
    }
    let ev = event_queue.pop();          // 取 (time, seq) 最小
    clock.advance_to(ev.time);           // 逻辑时钟跳到该时间(可瞬间前进)
    if clock.now() > horizon { report_time_skip(); break; } // 防失控
    dispatch(ev);                        // 见 8.3
}
```

### 8.2 协作式 executor 与 waker

- 每个任务存为 `Pin<Box<dyn Future<Output=()>>>`,以 `TaskId` 存于 slab。
- 为任务 T 构造的 `Waker`,被唤醒时**向堆入队一个 `PollTask(T)` 事件**(时间 = `now`,可加 PRNG 抖动以探索交织)。
- 仿真原语(`sleep`/`recv`/storage op)的实现:注册当前任务 waker,并入队相应的未来事件(timer 到期 / 消息投递 / IO 完成),该事件 fire 时唤醒任务。
- `dispatch(PollTask(T))`:用 T 的 waker poll 其 future 一次。`Pending` ⇒ 等待下次唤醒;`Ready` ⇒ 任务完成,唤醒其 `JoinHandle` 的等待者。

### 8.3 dispatch 各事件

- `DeliverMsg`:先查 `ConnectivityMatrix.can(from,to)`(分区可能拦截);命中则推入 `to` 的 inbox 并唤醒其 `recv`。
- `TimerFire`/`StorageComplete`:唤醒对应任务。
- `Nemesis`:执行故障动作(翻转分区、触发崩溃、注入位翻转等),细节见第 9 节。

### 8.4 必须正确处理的边界

- **死锁检测**:堆空但有任务 `Pending` 且永不会被唤醒 ⇒ 报告(附最后状态)。
- **时间失控 / livelock**:逻辑时间超过 `horizon`,或墙钟超出预算 ⇒ 终止并报告。
- **公平性**:对同时间就绪的多任务,排序必须确定(由 `(time, seq)` + 可选 PRNG 抖动给出),且抖动取自熵带。
- **取消**:`JoinHandle::abort` 与崩溃导致的任务 drop,必须正确清理已注册的 waker/timer,不能留下幽灵事件。

---

## 9. 故障注入引擎(`detersim-nemesis`)

所有故障决策**经熵带**,从而可复现、可重放、可 shrink。Nemesis 是一个根据计划(schedule)在特定逻辑时间注入 `NemesisAction` 的组件。

### 9.1 网络故障

- **延迟**:每次投递的延迟从配置区间按熵带抽取。
- **丢包**:按概率(熵带 `gen_bool`)丢弃投递。
- **重复**:以小概率把一条投递入队两次(不同延迟)。
- **乱序**:由不同延迟自然产生(无需特殊逻辑)。
- **分区**:翻转 `ConnectivityMatrix`,支持**非对称**与**多组**分区;支持限时分区后自愈。

### 9.2 时钟故障

- **偏移(skew)**:每节点一个时钟偏移量,使各节点 `now()` 不一致(但每节点自身仍单调)。
- **漂移(drift,stretch)**:偏移随时间变化。
- 用于测试依赖时间假设的逻辑(租约/lease、超时选举)。

### 9.3 磁盘故障

见 7.4:latency / lost-on-crash / torn write / bit rot / pre-fsync reorder。每类均可单独开关与配比。

### 9.4 进程崩溃 / 重启(最关键)

- **崩溃**:在熵带选定的时刻,drop 该节点全部任务(易失状态丢失),仅保留其 `Storage` 中**已 flush** 的数据;同时触发 lost-on-crash / torn write。
- **重启**:重新运行该节点的初始化逻辑,它**必须从磁盘恢复**。这是崩溃一致性的核心测试。
- **崩溃重启循环模式**:反复崩溃,逼出恢复路径里的 bug。

### 9.5 Nemesis 计划接口

```rust
trait NemesisPlan {
    /// 给定当前世界与一次熵带抽取,决定下一个注入动作及其时间(或 None 表示静默)。
    fn next_action(&mut self, world: &WorldView, tape: &mut EntropyTape) -> Option<(SimTime, NemesisAction)>;
}
// 内置:RandomPartition, RandomCrash, ClockSkew, DiskCorruption, Composite(多个叠加)。
```

---

## 10. 一致性检查(`detersim-check`)

### 10.1 在线不变量(便宜、覆盖广)

- SUT/测试暴露 `fn check_invariants(&self) -> Result<(), Violation>`,调度器在**静止点**或周期性调用。
- 例:"已提交的日志条目永不改变""同一 term 不能有两个 leader""余额永不为负"。
- 廉价且能抓住大量问题;应优先大量使用。

### 10.2 离线线性一致性检查器(Porcupine / WGL 算法)

- 输入:`Vec<OpRecord>` + 用户 `Model`。
- 判定:是否存在一个满足**实时优先序**(real-time precedence)的并发操作排列,使按该顺序在 `Model` 上 apply 得到所观测的全部输出。
- 算法:Wing-Gong 线性化搜索 + Lowe 优化 + bitset 记忆化(参考 Porcupine 的实现思路)。一般情形 NP 难,有界历史下用良好搜索 + 剪枝可行。
- 输出:线性化成功的见证顺序,或不可线性化的反例(指出冲突操作)。

### 10.3 弱一致性模型库

- 顺序一致性(sequential)、因果一致性(causal):各自独立的检查器,约束更弱。

### 10.4 事务一致性(stretch,Phase 6)

- 串行化 / 快照隔离:Elle 风格——在依赖图(读依赖/写依赖/版本序边)上做环检测。研究级,放到 stretch。

---

## 11. Trace、复现与最小化(`detersim-shrink` / `detersim-viz`)

### 11.1 seed 级复现(必达)

- 记录失败运行的 seed + 熵带。重跑 = 精确复现。**这是头号特性,任何阶段都必须成立。**
- 每个失败测试都打印其 seed;设置 `DST_SEED=<n>` 即可复现。

### 11.2 熵带级最小化(Phase 5,加分)

- **目标**:把"巨大的失败用例"收敛成人类可读的最小用例。
- **方法**:
  - 把所有控制决策表示为可消费的熵带 ⇒ 最小化 = 在保持失败的前提下 shrink 熵带 + 缩减工作负载(delta-debugging);
  - **因果感知**:删事件须尊重 happens-before(不能投递从未发送的消息);
  - 采用 Hypothesis 式结构化内部 shrinking 处理 cursor 错位问题。
- **shrinking soundness**(见第 12 节):最小化后的用例**必须仍能复现原失败**。

### 11.3 可视化(Phase 5)

- 导出事件 trace 为 JSON;附一个 HTML 时序/泳道图查看器(节点为泳道,消息为带箭头的线,故障注入点高亮)。

---

## 12. 测试与 Oracle 策略(本项目最重要的一节)

框架本身是测试工具,**必须被最严苛地自测**。以下 oracle 让 Codex 的每一步产出都可被客观验证——这是该项目"可被 Agent 高质量构建"的根本原因。

1. **确定性元测试(主 oracle,Phase 0 第一件事)**:同一 seed 跑两遍,断言**事件日志逐字节相同**。范围:进程内重复、进程重启之间;跨机/跨 OS 列为已知限制。**任何泄漏的不确定性都会让它变红**,因此它能在 Codex 写错时立刻报警。
2. **差分对照(differential)**:在可行处,SUT 在 `SimEnv` 与 `RealEnv` 下应产生相同的**逻辑结果**(不比时序)。
3. **"种 bug、找 bug"(plant-a-bug,召回 oracle)**:维护一批**已知注入特定 bug**的参考 SUT(丢更新、双 leader、未 fsync 就 ack、漏去重……),框架必须在 seed 预算内可靠地复现每一类。
4. **重放保真**:记录的 trace 重放到完全相同的失败。
5. **shrinking 健全性**:最小化结果必须仍复现原失败(且最小化尽量单调)。
6. **对框架自身数据结构做 property test**:事件堆、熵带、连通矩阵的不变量。
7. **CI 禁用 API(无泄漏门禁)**:clippy + grep 禁止 `std::time::*::now`、`std::net`、`tokio::spawn`、`std::thread`、`rand::thread_rng`、`HashMap`/`HashSet` 迭代等出现在 `core` 与示例 SUT 路径。

> 工程纪律:**Phase 0 先写确定性元测试,再写别的。** 没有它,后续一切"看起来能跑"的代码都不可信。

---

## 13. 路线图概览(详见 `ROADMAP.md`)

- **Phase 0 — 基础与确定性骨架**:workspace、逻辑时钟、单线程协作式 executor、`Rng`、熵带雏形、**确定性元测试**。
- **Phase 1 — 网络与 actor 模型**:`Network`、节点寻址消息、仿真介质 + PRNG 延迟。
- **Phase 2 — 故障注入(nemesis)**:分区(含非对称)、丢/重/延、时钟偏移,全部走熵带。
- **Phase 3 — 存储与崩溃重启**:`Storage`、PageStore、五类磁盘失效、崩溃/重启语义。
- **Phase 4 — 不变量 + 线性一致性检查器**:在线 hook + Porcupine/WGL 检查器。
- **Phase 5 — Trace 工具 + shrinking**:熵带最小化、因果感知 delta-debug、时序可视化。
- **Phase 6 — 验证与 stretch**:用真实参考算法(VSR/Raft)验证;socket 风格 API、Elle 检查器、透明拦截、覆盖率引导的 seed 选择。

---

## 14. 技术选型与依赖

- **语言**:Rust。理由:async 可被自定义 executor 完全接管、零成本抽象的性能、内存安全、与 madsim/turmoil/TigerBeetle 生态贴近。
- **允许的依赖**:确定性 PRNG(`rand_chacha` / 自实现 SplitMix64)、`bytes`、`thiserror`、`tracing`(仅日志,不参与控制流)、测试用 `proptest`。
- **生产侧 `RealEnv`**:`tokio`(current-thread 或 multi-thread)。
- **禁止 / 谨慎**:任何会**偷偷起线程**、用**系统 RNG**、或引入**不可控全局状态**的依赖,均不得进入 `core`/`sim`/示例 SUT 路径。
- **不自己实现底层密码学**:若 Phase 6 的参考实现需要 TLS,复用经审计的库(如 `rustls`),绝不手写原语。

---

## 15. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 隐藏的不确定性泄漏 | 整个框架失去意义 | 确定性元测试 + CI 禁用 API + 第 3 节清单逐条核对 |
| 自定义 async executor 复杂度 | 进度与正确性风险 | 以 tokio current-thread 为参考;先跑通 pingpong 再加复杂度;大量 property test |
| 线性一致性检查器性能 | 大历史跑不动 | 限定有界历史;bitset 记忆化;先小后大;提供超时与降级 |
| 熵带 shrinking 是研究级难题 | 可能做不到理想效果 | seed 复现作为必达底线;shrinking 列为加分项,不阻塞前序阶段 |
| 范围蔓延(想做成 madsim+Jepsen+一切) | 永远做不完 | 严守非目标;参考实现只为自检;socket/事务/拦截全在 stretch |
| 跨平台浮点/行为差异 | 跨机复现失败 | v1 明确限定"同二进制+同平台";作为已知限制写入文档 |

---

## 16. 项目级成功标准

1. 一个真实参考算法(VSR 或 Raft)写在 `Env` 之上,框架能在 seed 预算内**重新发现注入的每一类 bug**,且理想情况下找到一个**真实的细微 bug**。
2. 确定性元测试在 ≥10,000 个随机 seed 上保持逐字节一致(进程内 + 进程重启)。
3. 一个新用户能在 < 30 分钟内,照文档把自己的小型分布式组件接入并跑出第一个故障注入测试。
4. plant-a-bug 套件的每一类已知 bug 都被稳定复现(召回 100%)。
5. 文档(README + 教程 + 每个 crate 的 rustdoc)完整,示例可运行。

---

## 附录 A:术语表

- **DST**:Deterministic Simulation Testing,确定性仿真测试。
- **SUT**:System Under Test,被测系统。
- **Env / capability-passing**:SUT 通过注入的 `Env` 能力接触外部世界的范式。
- **Entropy tape(熵带)**:控制面随机决策的可记录/可重放序列。
- **Nemesis**:故障注入组件(术语借自 Jepsen)。
- **Linearizability / 线性一致性**:并发历史可被重排为尊重实时序的合法顺序执行。
- **Quiescence(静止)**:事件堆空、无就绪任务,世界已settled。
- **Shrinking / 最小化**:把失败用例收敛到最小可复现形式。

## 附录 B:灵感与参考来源(供实现者查阅)

- FoundationDB 的 Flow 与仿真测试方法(DST 鼻祖)。
- TigerBeetle 的 VOPR(把整个系统写在抽象 IO 之上)。
- madsim / turmoil / shuttle / loom(Rust 生态里的相邻方案,见第 2 节对比)。
- Jepsen(故障注入命名法)与 Knossos / Porcupine / Elle(一致性检查算法)。
- Hypothesis 的内部 byte-stream shrinking(熵带最小化思路)。

> 注:以上为方法论参考,实现以本规格的接口契约为准。
