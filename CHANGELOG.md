# 更新日志

## [1.1.0-alpha.0](https://github.com/lvzhenbo/115-plus-desktop/compare/v1.0.7...v1.1.0-alpha.0) (2026-07-15)

### ✨ 新功能

- 优化上传和下载列表展示，增强用户体验 ([8ab1d1f](https://github.com/lvzhenbo/115-plus-desktop/commit/8ab1d1f716ab73805ce503c55cead198b87fc93a))
- 优化视频加载提示和中心指示器图标 ([dd6778f](https://github.com/lvzhenbo/115-plus-desktop/commit/dd6778f54d29df69ecfadca7c9948dffc69724ae))
- 使用motion优化旋转动画 ([bc8b8e2](https://github.com/lvzhenbo/115-plus-desktop/commit/bc8b8e2aceede3c9c1e3ab14cf6e4a00ec46a049))
- 在批量重命名模态框中添加限流提示信息，并优化布局 ([d4c13b6](https://github.com/lvzhenbo/115-plus-desktop/commit/d4c13b651fedc9d20d5a596b4de5151d5ee8bb1c))
- 增加发布前插入自定义更新日志和解析功能 ([cb43360](https://github.com/lvzhenbo/115-plus-desktop/commit/cb433603c035c6161ad09c3c3ba51aea81e2b4bd))
- 实验性重构下载文件视图，优化任务显示和操作功能，增强用户体验 ([3089e32](https://github.com/lvzhenbo/115-plus-desktop/commit/3089e329f21759a51151a75b75128f0b1d0d1552))
- 将下载功能初步从调用aria2更改为原生rust代码 ([0e422a1](https://github.com/lvzhenbo/115-plus-desktop/commit/0e422a117ba239f1b9069a1d2f4a5fa60f4a27d2))
- 拖拽上传 ([ecc90e9](https://github.com/lvzhenbo/115-plus-desktop/commit/ecc90e9dc4465cde2b705785387a0dd38d83137a))
- 搜索结果添加重命名功能，并优化重命名弹窗数据结构适配更多情况 ([38c9cab](https://github.com/lvzhenbo/115-plus-desktop/commit/38c9cab4765afc316d7c4b4857b721d0e1460fbf))
- 文件浏览器添加搜索功能 ([5ae6b33](https://github.com/lvzhenbo/115-plus-desktop/commit/5ae6b33744a7440b2825201ba44e9b5f68ca4f18))
- 添加 pnpm 工作区配置以禁用 lfa-ponyfill 构建 ([52c89fe](https://github.com/lvzhenbo/115-plus-desktop/commit/52c89fe11670cb21d40fa3048728a87a58d356e2))
- 添加CDN限流处理逻辑，优化下载重试机制 ([66d45a4](https://github.com/lvzhenbo/115-plus-desktop/commit/66d45a449fa8cf00403156b98502ca0a41db6fdf))
- 添加上传任务的预计剩余时间（ETA）功能，优化用户体验（使用CI的用户可能需要删除download.db文件） ([60c6b98](https://github.com/lvzhenbo/115-plus-desktop/commit/60c6b98dc3b527a3882d96cbddbd61ff62b08f0d))
- 添加图片预览功能，支持多图展示和缩略图显示 ([ee1e4bb](https://github.com/lvzhenbo/115-plus-desktop/commit/ee1e4bbe5da9eb85f88aab2ad4de4762b0a5233c))
- 添加应用日志级别设置功能，支持动态调整日志级别 ([d1345a9](https://github.com/lvzhenbo/115-plus-desktop/commit/d1345a946e96e4fa001c9acccb210ab5a92e14e3))
- 添加批量重命名功能，更新相关组件和类型定义 ([7c091dd](https://github.com/lvzhenbo/115-plus-desktop/commit/7c091dd9211c8eb9220c0ac4e74acad418f10783))
- 添加视频旋转功能和居中提示，提取视频播放器控制栏组件， ([ec33851](https://github.com/lvzhenbo/115-plus-desktop/commit/ec33851f2419feae5d2101a8febf570539bb3a9e))
- 添加窗口最小尺寸设置功能，允许用户自定义窗口最小宽度和高度 ([eaf1407](https://github.com/lvzhenbo/115-plus-desktop/commit/eaf1407b2e282c1801124a62ab32b94ccc9121df))
- 添加系统托盘，并清理优化rust注释 ([7bbd4df](https://github.com/lvzhenbo/115-plus-desktop/commit/7bbd4dfb96498ec01d3c029237ad7370aec8bf1e))
- 用户信息弹窗添加0.5s延迟 ([419a28a](https://github.com/lvzhenbo/115-plus-desktop/commit/419a28a3f052224134656e4ac4fed436a685b12f))

### 🐛 Bug 修复

- use custom dbus_id to fix Linux startup crash ([#12](https://github.com/lvzhenbo/115-plus-desktop/issues/12)) ([88783e8](https://github.com/lvzhenbo/115-plus-desktop/commit/88783e8e2c92cfa085bee25e252a643cdb6f0319))
- 一波下载功能修复 ([8de88d4](https://github.com/lvzhenbo/115-plus-desktop/commit/8de88d4c23811935cc41f388fbc742d5958b9c99))
- 修复上传进度聚合与速度计算功能；移除不再使用的上传速度和ETA字段 ([89588d9](https://github.com/lvzhenbo/115-plus-desktop/commit/89588d9a19226049fcce6763bdf99ab0830e35a3))
- 修复之前被搞坏的限流功能，更正活跃任务所需状态 ([984b5d2](https://github.com/lvzhenbo/115-plus-desktop/commit/984b5d291bf002170a1345352527bacc8879cf2c))
- 修复字体读取错误处理，确保从字体文件中正确读取字体家族名称 ([0bd972c](https://github.com/lvzhenbo/115-plus-desktop/commit/0bd972c0bf636347ebfe809551fa1aaa7871e76f))
- 修复文件夹下载前置流程，全程任务持久化 ([15de830](https://github.com/lvzhenbo/115-plus-desktop/commit/15de8304e09d4b17a7279935202a7bb9c5a93f79))
- 修复登录页时无法退出应用的问题 ([a4de6aa](https://github.com/lvzhenbo/115-plus-desktop/commit/a4de6aa96b7e4de8020d02e02640fc6a9ac0817c))
- 再次修复 ([87512a4](https://github.com/lvzhenbo/115-plus-desktop/commit/87512a4d46bc1d6dfc67470c986eae147e28c524))
- 增加前端渲染完成后的等待时间，避免白屏闪烁 ([343099c](https://github.com/lvzhenbo/115-plus-desktop/commit/343099cda8e9280401ac2e42f90df81d526841c7))
- 增强接口错误处理提示 ([b0c43f1](https://github.com/lvzhenbo/115-plus-desktop/commit/b0c43f1d8e86c206922cca67c666589d5137121c))
- 尝试修复跨平台错误 ([09991d6](https://github.com/lvzhenbo/115-plus-desktop/commit/09991d68135ae8caa99f2ce27e706cfa35311a34))
- 尝试解决macos上的特殊问题 ([678795e](https://github.com/lvzhenbo/115-plus-desktop/commit/678795e4b2a66deda10b5917e5518857f73dbcfc))
- 恢复最小化窗口以确保主窗口可见 ([009aa8e](https://github.com/lvzhenbo/115-plus-desktop/commit/009aa8e2f799f801d7490833aa10bc73cfa28de1))
- 更新 GitHub Actions 工作流中的依赖版本 ([235cd98](https://github.com/lvzhenbo/115-plus-desktop/commit/235cd98cfd64faa42481334fca8167449c50ce63))
- 更新 README 以修正下载引擎描述并添加新功能说明 ([87077f2](https://github.com/lvzhenbo/115-plus-desktop/commit/87077f28bd0300ce6f574f5e235590d48ac1dee6))
- 更新依赖，并将无人维护带有漏洞的ttf-parser迁移到skrifa ([cc5b04a](https://github.com/lvzhenbo/115-plus-desktop/commit/cc5b04aa2e1819c132f8abff8689f2073e7203de))
- 替换radash为es-toolkit，并修复和优化一下问题 ([db70a6a](https://github.com/lvzhenbo/115-plus-desktop/commit/db70a6a011e24f1443becc3778caf2bab153db2c))
- 规范类型使用 ([8357293](https://github.com/lvzhenbo/115-plus-desktop/commit/835729342cbe2d46a4493617489e61be6e576700))
- 解决pinia初始化冲突带来的白屏问题 ([8fa4728](https://github.com/lvzhenbo/115-plus-desktop/commit/8fa47287d9235553edf1f3ed82615dddf761ffd0))
- 解决toolbar和enable-search重复定义的问题 ([ef63933](https://github.com/lvzhenbo/115-plus-desktop/commit/ef63933dc889d7c19f6c57e53452d30ad6aec71a))
- 解决更新依赖后的问题 ([f331908](https://github.com/lvzhenbo/115-plus-desktop/commit/f331908d6170e042efcb32e9627e37822feed96d))
- 调整限流参数以优化下载性能 ([5417ae5](https://github.com/lvzhenbo/115-plus-desktop/commit/5417ae5e5d4bba167825b685a57f5f5927dc456c))
- 还原vite alias，tsconfigPaths目前有bug ([459fbf6](https://github.com/lvzhenbo/115-plus-desktop/commit/459fbf6603ac096511a2f5982b8fc81e764ffd2f))

### ♻️ 代码重构

- 使用指令替代motion组件，移除没啥用的缓冲进度条 ([a7537d9](https://github.com/lvzhenbo/115-plus-desktop/commit/a7537d977b9d05cfa5c4fd0391e2c34c108e66e2))
- 重构上传数据处理 ([e1cf3ce](https://github.com/lvzhenbo/115-plus-desktop/commit/e1cf3cef112b59f88b62907de2b1b82cb566211d))
- 重构下载数据处理，从前端转移到rust后端 ([4af781d](https://github.com/lvzhenbo/115-plus-desktop/commit/4af781d58fc6c840d69d1c13c13cf14836098688))
- 重构字幕解析和渲染，具体看详情，理论上应该彻底解决字体渲染问题 ([90fe577](https://github.com/lvzhenbo/115-plus-desktop/commit/90fe577e09c0e2c18fcc36e58a07e6111cade369))

### 🔧 其他更新

- 优化清理工程文件 ([a361981](https://github.com/lvzhenbo/115-plus-desktop/commit/a3619811219b4cbc6b2e3913713f315f42f949cc))
- 使用自定义插件解决发布内容为空的问题 ([ed72176](https://github.com/lvzhenbo/115-plus-desktop/commit/ed72176827538f21389357224428e96618c48c0c))
- 依赖日常更新 ([094f21e](https://github.com/lvzhenbo/115-plus-desktop/commit/094f21e147a9da51bbac551d99506d2286eff22a))
- 更新rust依赖 ([504b530](https://github.com/lvzhenbo/115-plus-desktop/commit/504b53030a891deea58cb59a7b3d86c753815bf7))
- 更新rust依赖 ([39b9dc2](https://github.com/lvzhenbo/115-plus-desktop/commit/39b9dc298ae0329471652003ed498d6d47cf9da3))
- 更新依赖 ([e83f2d1](https://github.com/lvzhenbo/115-plus-desktop/commit/e83f2d12239da7366ee33bf2bb5378bd7c7455a7))
- 更新依赖 ([eec92bc](https://github.com/lvzhenbo/115-plus-desktop/commit/eec92bc9266bcb56819212ab18c3da0a575d6225))
- 更新依赖 ([abbe6cc](https://github.com/lvzhenbo/115-plus-desktop/commit/abbe6ccea97e092b138934232655aed64d0cb34b))
- 更新依赖 ([4a4e196](https://github.com/lvzhenbo/115-plus-desktop/commit/4a4e196b708fb545a93f05d04625e66ac5a29ad4))
- 更新依赖 ([13b0f72](https://github.com/lvzhenbo/115-plus-desktop/commit/13b0f72e75ff96a97e6934fc729350ccb4185431))
- 更新依赖 ([b152157](https://github.com/lvzhenbo/115-plus-desktop/commit/b152157fe2e628cbecb81e30613784843c53ced4))
- 更新依赖 ([3f9197c](https://github.com/lvzhenbo/115-plus-desktop/commit/3f9197c13ebb1fb4017df66d164ef47d2fd7b70f))
- 更新依赖 ([43b32da](https://github.com/lvzhenbo/115-plus-desktop/commit/43b32daec7bce549dad5c75715d534f07865b62d))
- 更新依赖 ([2a08ee0](https://github.com/lvzhenbo/115-plus-desktop/commit/2a08ee064bbdee39b55a911b7e414ab399c905fa))
- 更新依赖 ([22511b5](https://github.com/lvzhenbo/115-plus-desktop/commit/22511b5ab2f71d8ea2259f46265284fd355839a6))
- 更新依赖，并添加lock文件的格式化忽略 ([4fa3692](https://github.com/lvzhenbo/115-plus-desktop/commit/4fa369223853ff1054120ad3920e63a6501b532e))
- 更新依赖，并解决sha1更新后的问题，为ci添加缓存 ([f461adb](https://github.com/lvzhenbo/115-plus-desktop/commit/f461adbff8986b19060f714d56dfd766a021d711))
- 更新依赖，添加eslint忽略项 ([8484ad9](https://github.com/lvzhenbo/115-plus-desktop/commit/8484ad96e434c8c9b19a3538fb541a625d172979))
- 更新依赖，自述里添加一些说明 ([ef8a309](https://github.com/lvzhenbo/115-plus-desktop/commit/ef8a30965164c4d558ce57ff58912bbca579e73c))
- 更新依赖并将pnpm版本升级到11 ([7f353cc](https://github.com/lvzhenbo/115-plus-desktop/commit/7f353cc8b46b68570cb3ab38c24abb9043a6aab2))
- 更新依赖项，替换fs2为fs4并升级相关版本；重构代码以使用LazyLock优化同步 ([123ef87](https://github.com/lvzhenbo/115-plus-desktop/commit/123ef874d3d56981d5f81aa20d0b7f5ad66a93ac))
- 更新问题模板 ([69b7a23](https://github.com/lvzhenbo/115-plus-desktop/commit/69b7a23f312785670a69cef7db5970292158c3c9))
- 格式化VSCode扩展推荐文件 ([c6a15db](https://github.com/lvzhenbo/115-plus-desktop/commit/c6a15dbb5f12e6b124170664e72fd75b5d7b3520))
- 注释掉获取aria2脚本的执行步骤 ([ee1e461](https://github.com/lvzhenbo/115-plus-desktop/commit/ee1e4611e1782498e5d025080cb02be8c1804a0a))
- 添加 Linux 平台兼容性描述 ([d865c57](https://github.com/lvzhenbo/115-plus-desktop/commit/d865c577b184020c1a94cf07377391416bd1b517))
- 添加问题模板 ([46d46af](https://github.com/lvzhenbo/115-plus-desktop/commit/46d46af91389622591f63b7fd4e46adb4adf6556))
- 移除过重的`@release-it/bumper`，改为自定义轻量插件 ([d2497b6](https://github.com/lvzhenbo/115-plus-desktop/commit/d2497b63640be5ed877e2ae8018770e4c4169827))

### 👷 CI 配置

- 优化发布脚本 ([dc80916](https://github.com/lvzhenbo/115-plus-desktop/commit/dc809166485485b67c39ef606036f980ee6f8a02))
- 修复构建工件名称格式，去掉args之间的分隔符 ([6792d9d](https://github.com/lvzhenbo/115-plus-desktop/commit/6792d9d89ec0743a88fd34cf65f321be2d6c0bc4))
- 修复预发布脚本 ([1c52a29](https://github.com/lvzhenbo/115-plus-desktop/commit/1c52a294ffa8a88721a3f537b83b854c6da77095))
- 再修预发布脚本 ([e5d9be5](https://github.com/lvzhenbo/115-plus-desktop/commit/e5d9be5609b5112166a62d319bb8e085b1cebc56))
- 更新CI/CD配置，支持macOS和Ubuntu平台构建 ([f58d40e](https://github.com/lvzhenbo/115-plus-desktop/commit/f58d40eaebd3ae11ff3956dc2dc93737d930b28e))
- 测试aria2的跨平台编译 ([8587b95](https://github.com/lvzhenbo/115-plus-desktop/commit/8587b956f0e2e1199bb15ef953596be566969f03))
- 添加预发布脚本 ([f482bf4](https://github.com/lvzhenbo/115-plus-desktop/commit/f482bf4c52372f423dc2a79b2be58c475d281ab9))

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
