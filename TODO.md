- [x] no_std support

- [x] GC refactor decisions are confirmed with user before action and logged here

- [ ] GC重构计划：
	- [x] 确认目标：使用`so2js_gc`替换`so2js/runtime/gc`胶水实现，保持intrinsics暂不改动（保留基于kind的`visit_pointers_for_kind`派发逻辑，其他旧gc胶水待移除）
	- [ ] 梳理范围：`context`、GC交互、VM行为依赖的GC接口，识别与旧GC绑定的模块
	- [ ] 设计方案：定义新的GC接口（分配、写屏障、根扫描、增量步骤）与上下文生命周期
	- [ ] 迁移顺序：先抽象接口→替换上下文/VM接入点→移除旧`gc`目录→接入新`so2js_gc`
	- [ ] 风险与验证：列出API差异、并发/增量语义、栈根处理，设计回归测试策略

- [ ] 全局安装：采用 `GlobalInstaller` trait 抽象安装流程，先设计API再接入

- [ ] Rust runtime 组织整洁化：当前 runtime 目录层级/巨型注册表（如 rust_runtime.rs）可读性差，重构时一并整理


- [x] 修复 so2js_gc 分配对齐：保持 GcHeader repr(C)/8 字节对齐，alloc 对齐超出8时报错避免 UB

- [ ] StackRoot 模块化方案：保持内部模块（不拆子crate），重组目录（移除旧 runtime/gc，新增 runtime/stack 挂载 handle/StackRoot）


- [ ] 移除 header 反查 Context：
	- [x] 删除 Value/HeapPtr 无参 to_stack 依赖 GcHeader.context_ptr（只接受显式 Context）
	- [x] 仅保留显式 to_stack(cx) 路径（to_stack_with 已移除，调用点已清理）
	- [x] 更新 Escapable 等调用链，不再隐式获取 Context
	- [ ] 编译/测试验证无隐式 Context 反查

- [ ] StackRoot/NaN-box路线：选择“纯指针Handle”方案（StackRoot仅指针，非指针直接传值或显式to_stack_with(cx)，移除GcHeader反查Context）

- [ ] 迁移 StackRoot 目录：
	- [x] 新建 runtime/stack 模块承载 handle/StackRoot/Scope/宏
	- [x] 调整所有引用路径（Context、Value、HeapPtr 等调用方）；所有 to_stack_with 已清理
	- [x] 删除旧 runtime/gc 中 handle 代码，保留 GC 相关接口清晰分层
	- 运行测试验证
