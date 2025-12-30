# SO2JS GC 重构：从 Compact GC 到 Tri-color Incremental Tracing GC

## 背景
当前的 GC 是 Cheney-style semispace compact GC，经常因平台问题出现 bug。
需要替换为三色标记增量追踪 GC（非线程安全版本）。

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
  - [x] `finish_gc<C: GcContext>(&mut self, ctx: &mut C)`
  - [x] 内部创建实现 `GcVisitor` 的 `Marker` 结构体
  - [x] `alloc` 接受 `ctx` 参数，GC 期间分配的对象标黑
- [x] 修改 `so2js_gc/src/lib.rs` 导出新 trait
- [x] 修改 `so2js_gc/src/tests.rs` 适配新 API（23 个测试全部通过）

### Phase 2: 适配 so2js
- [ ] `so2js` Context 实现 `GcContext`
- [ ] 修改 `Heap::run_gc` 使用新 API
- [ ] 保持 `HeapItem::visit_pointers` 兼容

### Phase 3: 验证
- [x] `so2js_gc` 测试通过（23/23）
- [ ] `so2js` 编译通过
- [ ] 运行 JS 测试

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
