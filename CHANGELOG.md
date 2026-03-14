# 更新日志

## [1.0.7](https://github.com/lvzhenbo/115-plus-desktop/compare/v1.0.6...v1.0.7) (2026-03-14)

### ✨ 新功能

- 将侧栏折叠功能移到头部，减少误触 ([e909222](https://github.com/lvzhenbo/115-plus-desktop/commit/e90922284104eab271eb5eec107ffaeac60e3cb1))

### 🐛 Bug 修复

- 修复数据库迁移SQL语句的格式 ([6cb6e1b](https://github.com/lvzhenbo/115-plus-desktop/commit/6cb6e1b56d878d9336f5665df41df80b8eb7e5a8))
- 修复空状态组件的居中显示 ([1dcaf8f](https://github.com/lvzhenbo/115-plus-desktop/commit/1dcaf8f40409deea71f4e9a6a4d38cfb8ac7fc4c))

### ♻️ 代码重构

- 使用vueuse优化键盘快捷键处理和单击计时器逻辑 ([16048fd](https://github.com/lvzhenbo/115-plus-desktop/commit/16048fdddaa4f0989b9d9d217d0a3fb222ad4895))
- 简化更新提示弹窗 ([d6e1ec3](https://github.com/lvzhenbo/115-plus-desktop/commit/d6e1ec3eac4b269b858020237fade93fdc548f5e))

### 🔧 其他更新

- 更新依赖，使用undici代替axios ([c006883](https://github.com/lvzhenbo/115-plus-desktop/commit/c006883c57f1c2508c128d75c57fd4c0e24ae7a1))

## [1.0.6](https://github.com/lvzhenbo/115-plus-desktop/compare/v1.0.5...v1.0.6) (2026-03-09)

### ✨ 新功能

- 优化重命名模态框，增加重置按钮并调整输入框样式 ([638f5b3](https://github.com/lvzhenbo/115-plus-desktop/commit/638f5b31453c2f80c7d4935a08562a7baec338a7))
- 在搜索模态中添加高亮显示功能，支持根据搜索值高亮文件名 ([9587c44](https://github.com/lvzhenbo/115-plus-desktop/commit/9587c4411934e598e3f4311d6c57d19502fcbcc1))
- 统一接口速率限制功能，添加接口速率限制设置，支持动态调整请求频率 ([659cc17](https://github.com/lvzhenbo/115-plus-desktop/commit/659cc17b7c6bbec4fdb5ac8e983d03053ddb0c7c))

### 🔧 其他更新

- 更新依赖 ([f80cdd5](https://github.com/lvzhenbo/115-plus-desktop/commit/f80cdd56393ce611c01a1f56900dc6e27857a1ea))
- 添加组件名强制使用 PascalCase 的 ESLint 规则 ([78638d1](https://github.com/lvzhenbo/115-plus-desktop/commit/78638d173bc8e5b4b927e5861d37ac10b03a1bb3))

## [1.0.5](https://github.com/lvzhenbo/115-plus-desktop/compare/v1.0.4...v1.0.5) (2026-03-06)

### 🐛 Bug 修复

- 修复更新应用时被aria2占用问题，并重启应用时弹出提示 ([d18637c](https://github.com/lvzhenbo/115-plus-desktop/commit/d18637c25a2e9ac263eb2cc41a2e03e9968ec530))

### ♻️ 代码重构

- 整理rust代码，分离aria2和数据库相关代码为独立模块 ([89544eb](https://github.com/lvzhenbo/115-plus-desktop/commit/89544eb5f883f8fc96e66684410c0af4132c2f1d))

### 📝 文档更新

- 更新 README.md，完善功能描述和开发说明 ([089d459](https://github.com/lvzhenbo/115-plus-desktop/commit/089d4597b4e8b2d1819673ef25bb86e96c821b55))

## [1.0.4](https://github.com/lvzhenbo/115-plus-desktop/compare/v1.0.3...v1.0.4) (2026-03-05)

### ✨ 新功能

- 添加 marked 库以支持更新说明的 Markdown 渲染 ([0af9c78](https://github.com/lvzhenbo/115-plus-desktop/commit/0af9c78da735117f81c6548b875cd207bf631b9c))

### 🐛 Bug 修复

- 修正 updater 插件的 pubkey 格式 ([ab3a072](https://github.com/lvzhenbo/115-plus-desktop/commit/ab3a0729a72257a39e8f24c44dcd83bd4097fd48))

## [1.0.3](https://github.com/lvzhenbo/115-plus-desktop/compare/v1.0.2...v1.0.3) (2026-03-05)

### 🔧 其他更新

- 工程规范，并测试更新功能 ([71b1efa](https://github.com/lvzhenbo/115-plus-desktop/commit/71b1efa46e8329db4d89b6631376a24576ceb78b))
