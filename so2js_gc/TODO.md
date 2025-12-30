# SO2JS GC 重构：从 Compact GC 到 Tri-color Incremental Tracing GC

## 背景
当前的 GC 是 Cheney-style semispace compact GC，经常因平台问题出现 bug。
需要替换为三色标记增量追踪 GC（非线程安全版本）。

---

## 架构分析

### 当前架构（Cheney-style Semispace Copying GC）

```
so2js/runtime/gc/
├── mod.rs                    # 导出 GC 相关类型
├── heap.rs                   # Heap - 双 semispace 设计，bump 分配
├── garbage_collector.rs      # GarbageCollector - Cheney 复制算法，实现 HeapVisitor
├── heap_visitor.rs           # HeapVisitor trait - visit(), visit_weak() 等
├── heap_item.rs              # HeapItem trait - byte_size(), visit_pointers()
├── pointer.rs                # HeapPtr<T> - 裸指针包装
├── handle.rs                 # Handle<T> - 安全的栈上引用
└── heap_trait_object.rs      # trait object 相关
```

**关键类型**：
- `Heap`: 双 semispace（from/to），通过 `swap_heaps()` 切换
- `GarbageCollector`: 实现 `HeapVisitor`，`run()` 同步执行 GC
- `HeapVisitor`: GC 访问者 trait，69 个类型实现 `visit_pointers(&mut self, visitor: &mut impl HeapVisitor)`
- `HeapPtr<T>`: 堆上对象的直接指针（GC 期间不安全，因为复制 GC 会移动对象）
- `Handle<T>`: 栈上安全引用（通过 HandleScope 管理）

**GC 流程**：
1. `Context::visit_roots_for_gc()` 访问所有根
2. `GarbageCollector` 实现 `HeapVisitor::visit()`，将对象从 from-space 复制到 to-space
3. 复制时留下转发指针（forwarding pointer）
4. 处理弱引用（WeakRef, WeakSet, WeakMap, FinalizationRegistry）
5. 交换 from/to space

### 新架构（Tri-color Mark-Sweep GC）

```
so2js_gc/src/           # 独立 GC crate（已完成）
├── heap.rs             # Heap - 链表管理对象，非移动式
├── gc_header.rs        # GcHeader - 对象头（颜色、大小、next）
├── gray_queue.rs       # 灰队列
├── visitor.rs          # GcVisitor + GcContext traits
├── pointer.rs          # GcPtr<T> - 非移动指针
└── tests.rs            # 23 个测试

so2js/runtime/gc/       # 运行时适配层（待修改）
├── heap.rs             # 删除 semispace，使用 so2js_gc::Heap
├── garbage_collector.rs # 删除复制逻辑，实现 GcContext
├── heap_visitor.rs     # 可能删除或改为 re-export GcVisitor
├── heap_item.rs        # 保留，但 visit_pointers 用 GcVisitor
├── pointer.rs          # HeapPtr<T> 改为 re-export GcPtr<T> 或适配
└── handle.rs           # 基本不变
```

---

## 待办事项

### Phase 1: 重构 so2js_gc API ✅ 完成
- [x] 创建 `so2js_gc/src/visitor.rs`
  - [x] 定义 `GcVisitor` trait
  - [x] 定义 `GcContext` trait
- [x] 修改 `so2js_gc/src/heap.rs`
  - [x] 删除 `trace_object_fn`, `process_weak_refs_fn` 字段（从未添加，直接用 trait）
  - [x] `start_gc<C: GcContext>(&mut self, ctx: &mut C)`
  - [x] `gc_step<C: GcContext>(&mut self, ctx: &mut C) -> bool`
  - [x] `finish_gc<C: GcContext>(&mut self, ctx: &mut C) -> usize`
  - [x] 内部创建实现 `GcVisitor` 的 `Marker` 结构体
  - [x] `alloc` 接受 `ctx` 参数，GC 期间分配的对象标黑
- [x] 修改 `so2js_gc/src/lib.rs` 导出新 trait
- [x] 修改 `so2js_gc/src/tests.rs` 适配新 API（23 个测试全部通过）

### Phase 2: 适配 so2js

#### 2.1 桥接层设计
- [ ] 在 `so2js/runtime/gc/mod.rs` 中 re-export `so2js_gc` 类型
- [ ] 统一 `HeapPtr<T>` 和 `GcPtr<T>`（选择其一或适配）
- [ ] 统一 `HeapVisitor` 和 `GcVisitor`（选择其一或适配）

#### 2.2 修改 Heap
- [ ] `so2js/runtime/gc/heap.rs`:
  - [ ] 删除 semispace 相关字段（`start`, `current`, `end`, `next_heap_start`, `next_heap_end`）
  - [ ] 内部持有 `so2js_gc::Heap`
  - [ ] 修改 `alloc()` 方法调用 `so2js_gc::Heap::alloc()`
  - [ ] 删除 `swap_heaps()`，添加 `run_gc()` 调用增量 GC

#### 2.3 实现 GcContext for Context
- [ ] `so2js/runtime/context.rs`:
  - [ ] `impl GcContext for Context`
  - [ ] `visit_roots()`: 调用现有的 `visit_common_roots`, `visit_post_initialization_roots`, `visit_permanent_roots`
  - [ ] `trace_object()`: 根据 `HeapItemDescriptor::kind()` 分发到对应类型的 `visit_pointers()`
  - [ ] `process_weak_refs()`: 处理 WeakRef, WeakSet, WeakMap, FinalizationRegistry

#### 2.4 修改 GarbageCollector
- [ ] `so2js/runtime/gc/garbage_collector.rs`:
  - [ ] 删除 Cheney 复制算法相关代码
  - [ ] 删除 `move_heap_item`, `copy_or_fix_pointer` 等
  - [ ] 删除转发指针逻辑
  - [ ] 保留弱引用处理逻辑，但改为基于 `Heap::is_alive()` 判断
  - [ ] `GcType::Normal` / `GcType::Grow` 可能需要调整

#### 2.5 统一 HeapVisitor 和 GcVisitor
- [ ] 选择方案：
  - **方案 A**: 用 `GcVisitor` 替换 `HeapVisitor`
    - 修改所有 69 个 `impl HeapItem` 的 `visit_pointers` 签名
    - 优点：统一接口
    - 缺点：修改量大
  - **方案 B**: 让 `HeapVisitor` 扩展 `GcVisitor`
    - `HeapVisitor: GcVisitor` 继承
    - 优点：向后兼容
    - 缺点：两套 trait 可能混乱
  - **方案 C**: 让 `Marker` 同时实现 `HeapVisitor` 和 `GcVisitor`
    - 优点：不改现有代码
    - 缺点：需要写适配层
- [ ] **待决定**：询问用户选择哪个方案

#### 2.6 修改 HeapItem 实现（如选择方案 A）
涉及 69 个类型的 `visit_pointers` 方法签名修改：
- [ ] `so2js/runtime/gc/heap_item.rs` (AnyHeapItem)
- [ ] `so2js/runtime/value.rs` (SymbolValue, BigIntValue)
- [ ] `so2js/runtime/object_value.rs` (ObjectValue)
- [ ] `so2js/runtime/string_value.rs` (StringValue, ConcatString, FlatString)
- [ ] `so2js/runtime/array_object.rs` (ArrayObject)
- [ ] `so2js/runtime/array_properties.rs` (ArrayProperties, DenseArrayProperties, SparseArrayProperties)
- [ ] `so2js/runtime/realm.rs` (Realm, GlobalScopes)
- [ ] `so2js/runtime/scope.rs` (Scope)
- [ ] `so2js/runtime/promise_object.rs` (PromiseObject, PromiseReaction, PromiseCapability)
- [ ] `so2js/runtime/proxy_object.rs` (ProxyObject)
- [ ] `so2js/runtime/generator_object.rs` (GeneratorObject)
- [ ] `so2js/runtime/async_generator_object.rs` (AsyncGeneratorObject, AsyncGeneratorRequest)
- [ ] `so2js/runtime/arguments_object.rs` (MappedArgumentsObject, UnmappedArgumentsObject)
- [ ] `so2js/runtime/bytecode/function.rs` (Closure, BytecodeFunction)
- [ ] `so2js/runtime/bytecode/constant_table.rs` (ConstantTable)
- [ ] `so2js/runtime/bytecode/exception_handlers.rs` (ExceptionHandlers)
- [ ] `so2js/runtime/intrinsics/*.rs` (约 30+ 类型)
- [ ] `so2js/runtime/module/*.rs` (SourceTextModule, SyntheticModule, etc.)
- [ ] ... 其他

#### 2.7 修改指针类型（如需要）
- [ ] `HeapPtr<T>` 目前是裸指针包装
- [ ] `GcPtr<T>` 也是裸指针包装
- [ ] 可能需要统一或添加 `From`/`Into` 转换
- [ ] 检查 `HeapPtr::uninit()` 和 `GcPtr::dangling()` 的对应关系

#### 2.8 Handle 和 HandleScope
- [ ] `Handle<T>` 基本不需要改动（已经是安全的栈引用）
- [ ] `HandleScope` 可能需要在 `alloc` 时配合 GC

### Phase 3: 验证
- [x] `so2js_gc` 测试通过（23/23）
- [ ] `so2js` 编译通过
- [ ] 运行 `cargo test` 在 so2js
- [ ] 运行 `so2js_tests` 测试套件
- [ ] 运行 test262 测试（如适用）

---

## 变更日志
- 2025-12-31: 创建 TODO 文件，完成代码分析
- 2025-12-31: 重构 so2js_gc，移除循环依赖，简化为纯净的 GC 核心
- 2025-12-31: 添加 21 个单元测试并全部通过（包括弱引用测试）
- 2025-12-31: Review 发现当前 GC 不是增量式的，决定实现真正的增量 GC
- 2025-12-31: 分析原有 GC 设计，重新设计 API 接口（GcVisitor + GcContext trait）
- 2025-12-31: **Phase 1 完成** - 实现 trait-based 增量 GC API
  - 新增 `visitor.rs`: `GcVisitor` trait（对象 trace 用）、`GcContext` trait（运行时实现）
  - `Marker` 结构体：只借用 `gray_queue`，实现 `GcVisitor`，解决借用冲突
  - `alloc` 接受 `&mut impl GcContext`，GC 期间分配的对象标黑（浮动垃圾）
  - 修复：GC 任意阶段（不只是 marking）分配的对象都标黑，防止立即被清扫
  - `finish_gc` 返回执行的 step 数量
- 2025-12-31: **Phase 2 开始** - 分析 so2js 架构
  - 当前：Cheney-style semispace copying GC
  - 目标：非移动式三色标记清扫 GC
  - 识别 69 个 `HeapItem` 实现需要适配
  - 识别需要决定的关键问题：HeapVisitor vs GcVisitor 统一方案
