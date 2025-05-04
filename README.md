# 115+ 桌面客户端

这是一个基于 115 网盘开放平台实现的第三方开源桌面客户端，使用Tauri2和Vue3实现，支持文件管理、视频播放和文件下载功能。

## 下载

正式版暂未发布，可到 [Github Action](https://github.com/lvzhenbo/115-plus-desktop/actions) 下载 CI 流水线产物

## 功能

- [x] 手机扫码登录
- [x] 用户信息
- [ ] 文件上传
- [x] 文件列表
  - [x] 文件（夹）复制、移动、删除、重命名和详情
  - [x] 新建文件夹
- [x] 文件搜索
- [x] 文件下载
- [x] 回收站列表
  - [x] 文件还原
  - [x] 删除/清空回收站
- [x] 视频播放
  - [x] 获取并记忆视频播放进度
  - [ ] 视频字幕
- [x] 云下载
  - [x] 下载配额
  - [x] 链接下载
  - [ ] BT种子解析下载
  - [x] 云下载任务列表
  - [x] 下载任务删除
  - [x] 下载任务文件打开

## 开发说明

### 建议开发配置

- IDE: [VS Code](https://code.visualstudio.com/) + [Volar](https://marketplace.visualstudio.com/items?itemName=Vue.volar) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
- Node.js: 最新LTS
- 包管理器: 启用Corepack
- Rust: 最新稳定版
- 网络环境: 可连Github

### 115网盘开放平台AppID和AppKey设置

```conf
# .env文件
VITE_APP_ID=你的AppID
VITE_APP_KEY=你的AppKey
```

## 许可证

MIT
