# SO2JS GC 重构：Tri-color Incremental Mark-Sweep GC

## 当前状态

✅ **Phase 1 完成** - `so2js_gc` 独立 GC crate (24 测试通过)
✅ **Phase 2 完成** - `so2js` 整合，编译通过
✅ **Phase 3 完成** - 实现 `GcContext`
✅ **Phase 4 完成** - GcHeader 指针压缩优化
✅ **Phase 5 完成** - 移除 HeapInfo，HandleContext 移至 ContextCell
🔄 **Phase 6 进行中** - 修复运行时崩溃

---

## 🚨 当前问题：`to_handle()` 崩溃

### 问题描述

运行 example 时 SIGSEGV 崩溃：

```
<so2js_gc::gc_header::GcHeader>::context_ptr (gc_header.rs:121)
<HeapPtr<ObjectValue>>::to_handle (handle.rs:538)
<Intrinsics>::get (intrinsics.rs:499)
<Realm>::get_intrinsic (realm.rs:118)
<TypedArrayPrototype>::new (typed_array_prototype.rs:106)
<Intrinsics>::initialize (intrinsics.rs:251)
```

### 根本原因

1. **旧设计**：使用 1GB 对齐的堆，`HeapInfo::from_heap_ptr(ptr)` 通过 `(ptr & HEAP_BASE_MASK)` 找到 HeapInfo
   - 即使是 dangling 指针，掩码运算后也能找到一个有效的 HeapInfo 地址（虽然逻辑上不对，但不会崩溃）

2. **新设计**：移除 1GB 对齐，从 `GcHeader` 获取 `context_ptr`
   - `HeapPtr::to_handle()` 调用 `GcHeader::from_object_ptr(self.as_ptr())`
   - 对于未初始化的指针（`HeapPtr::uninit()` = dangling），计算 `dangling - 24` 得到无效地址
   - 读取无效地址的 `context_and_color` 导致 SIGSEGV

3. **触发场景**：`Intrinsics::initialize` 初始化过程中
   - `TypedArrayPrototype::new` 调用 `realm.get_intrinsic(Intrinsic::ArrayPrototypeToString)`
   - 但此时 `ArrayPrototypeToString` 尚未初始化，是 `HeapPtr::uninit()`
   - `is_dangling()` 检查失败：`NonNull::dangling()` = `0x8`，但实际值是 `0x8006000000000000`

### 解决方案

**方案 A：让 `to_handle()` 接受 Context 参数** ⭐ 推荐
```rust
// 改变签名
impl<T: IsHeapItem> HeapPtr<T> {
    pub fn to_handle(self, cx: Context) -> Handle<T> {
        let handle_context = &mut cx.handle_context;
        Handle::new(handle_context, T::to_handle_contents(self))
    }
}
```
- 优点：最安全、最清晰
- 缺点：需要修改所有 `to_handle()` 调用点（约 100+ 处）

**方案 B：修复初始化顺序**
- 确保被依赖的 intrinsics 先初始化
- 缺点：容易遗漏，不是根本解决

**方案 C：恢复全局 Context 访问机制**
- 使用 thread-local 或其他方式存储当前 Context
- 缺点：增加复杂度，可能有多线程问题

---

## 架构

### so2js_gc (独立 GC 核心)
```
so2js_gc/src/
├── heap.rs          # Heap - 链表管理对象，增量三色标记清扫
├── gc_header.rs     # GcHeader - 对象头（context_ptr+color 压缩、大小、next）
├── gray_queue.rs    # 灰队列
├── visitor.rs       # GcVisitor + GcContext traits
├── pointer.rs       # GcPtr<T> - 非移动指针
└── tests.rs         # 24 个测试 ✅
```

### so2js/runtime/gc (运行时适配层)
```
so2js/runtime/gc/
├── mod.rs           # 导出
├── heap.rs          # Heap 包装 so2js_gc::Heap
├── heap_visitor.rs  # GcVisitorExt 扩展 trait
├── heap_item.rs     # HeapItem trait + visit_pointers_for_kind()
├── pointer.rs       # HeapPtr<T> = #[repr(transparent)] wrapper of GcPtr<T>
├── handle.rs        # Handle<T>, HandleScope, HandleContext
└── heap_trait_object.rs
```

**已删除**: 
- `garbage_collector.rs` (旧 Cheney GC)
- `HeapInfo` (1GB 对齐相关)

### GcHeader 指针压缩

```rust
#[repr(C)]
pub struct GcHeader {
    /// 指针压缩：低 3 位存 GC color (0-2)，高位存 context_ptr
    /// 因为 context_ptr 是 8 字节对齐的，低 3 位始终为 0
    context_and_color: usize,
    alloc_size: usize,
    next_object: Option<NonNull<GcHeader>>,
}
// SIZE = 24 bytes (3 x usize on 64-bit)
```

### ContextCell 结构

```rust
pub struct ContextCell {
    pub heap: Heap,
    pub handle_context: HandleContext,  // 从 HeapInfo 移过来
    // ... 其他字段
}

impl ContextCell {
    pub fn as_ptr(&self) -> *mut () { self as *const _ as *mut () }
    pub fn from_context_cell_ptr(ptr: *mut ()) -> &'static mut ContextCell { ... }
}
```

---

## 待办事项

### Phase 6: 修复 to_handle 崩溃 🔥

- [ ] 选择解决方案（推荐方案 A）
- [ ] 修改 `HeapPtr::to_handle` 签名为 `to_handle(self, cx: Context)`
- [ ] 批量更新所有调用点
- [ ] 运行测试验证

### Phase 7: 验证

- [ ] `cargo test -p so2js`
- [ ] `cargo test -p so2js_tests`
- [ ] 运行 example

### 后续优化

- [ ] 实现 `process_weak_refs` - 处理 WeakRef, WeakMap, WeakSet, FinalizationRegistry
- [ ] 添加写屏障 (write barrier) 用于增量 GC 正确性
- [ ] 性能调优：GC 步进大小、触发阈值

---

## 变更日志

- 2025-12-31: Phase 1 - 实现 so2js_gc 增量 GC (24 测试通过)
- 2025-12-31: Phase 2 - 整合到 so2js
  - 删除 `garbage_collector.rs` (Cheney GC)
  - `HeapPtr<T>` 改为包装 `GcPtr<T>`
  - 创建 `GcVisitorExt` 扩展 trait
  - 批量替换 `HeapVisitor` → `GcVisitor`
  - `so2js::Heap` 包装 `so2js_gc::Heap`
  - **编译通过！**
- 2025-12-31: Phase 3 - 实现 GcContext
  - `RuntimeContext` 实现 `GcContext`
  - `visit_roots` 调用 `Context::visit_roots_for_gc`
  - `trace_object` 调用 `AnyHeapItem::visit_pointers_for_kind`
  - **编译通过！**
- 2025-12-31: Phase 4 - GcHeader 指针压缩
  - `context_and_color: usize` 低 3 位存 color，高位存 context_ptr
  - 添加 `GcContext::as_context_ptr()` 方法
  - 24 测试通过
- 2025-12-31: Phase 5 - 移除 HeapInfo
  - 删除 `HeapInfo` 和 1GB 对齐分配
  - `HandleContext` 移至 `ContextCell`
  - `HandleScope` 改为存储 `context_ptr`
  - `to_handle()` 从 `GcHeader` 获取 `context_ptr`
  - **编译通过，但运行崩溃！**
- 2025-12-31: Phase 6 - 调试崩溃问题
  - 发现 `to_handle()` 对未初始化指针崩溃
  - 问题：intrinsics 初始化期间访问未初始化的 intrinsic
  - **待修复**