# 115+ 桌面客户端

![GitHub License](https://img.shields.io/github/license/lvzhenbo/115-plus-desktop) ![GitHub Release](https://img.shields.io/github/v/release/lvzhenbo/115-plus-desktop) ![GitHub Actions](https://img.shields.io/github/actions/workflow/status/lvzhenbo/115-plus-desktop/ci.yml)

基于 [115 网盘开放平台](https://open.115.com/) 的第三方开源桌面客户端，使用 **Tauri 2** + **Vue 3** + **TypeScript** 构建，支持文件管理、原生高速下载、OSS 分片上传和 HLS 视频播放。

## 下载

前往 [Releases](https://github.com/lvzhenbo/115-plus-desktop/releases) 下载最新稳定版本安装包，也可到 [GitHub Actions](https://github.com/lvzhenbo/115-plus-desktop/actions) 下载 CI 流水线产物。

支持 Windows、macOS 和 Linux 平台，**目前非Windows平台正在处于测试状态，欢迎有相关环境和设备的用户使用CI构建测试，一起来推进1.1.0版本。**

## 功能

### 用户

- [x] 手机扫码登录
- [x] 用户信息查看

### 文件管理

- [x] 文件/文件夹列表（支持列表/网格视图切换、自定义排序）
- [x] 文件（夹）复制、移动、删除、重命名
- [x] 文件详情查看
- [x] 新建文件夹
- [x] 文件搜索

### 文件下载

- [x] 原生多线程高速下载
- [x] 断点续传
- [x] 文件夹递归下载
- [x] 下载暂停/恢复/重试
- [x] 下载任务持久化（SQLite）

### 文件上传

- [x] 文件和文件夹上传
- [x] OSS 分片上传（支持大文件）
- [x] 秒传检测（SHA1 预校验）
- [x] 断点续传
- [x] 上传暂停/继续

### 视频播放

- [x] HLS 在线视频播放
- [x] 播放进度记忆与恢复
- [x] 视频字幕

### 云下载（离线下载）

- [x] 链接离线下载
- [ ] BT 种子解析下载
- [x] 云下载任务列表与管理
- [x] 下载配额查看
- [x] 任务文件直接打开

### 回收站

- [x] 文件还原
- [x] 删除/清空回收站

### 系统

- [x] 应用内自动更新
- [x] 窗口状态保存（位置、大小）
- [x] 单实例运行
- [x] 深色/浅色主题（跟随系统）

## 技术栈

| 类别        | 技术                          |
| ----------- | ----------------------------- |
| 桌面框架    | Tauri 2                       |
| 前端框架    | Vue 3 + TypeScript            |
| 构建工具    | Vite                          |
| UI 组件库   | Naive UI                      |
| 样式        | Tailwind CSS                  |
| 状态管理    | Pinia（持久化至 Tauri Store） |
| HTTP 客户端 | Alova                         |
| 视频播放    | HLS.js                        |
| 数据库      | SQLite                        |
| 后端语言    | Rust                          |

## 项目结构

```
src/                          # 前端源码
├── api/                      # 115 网盘 API 封装
│   └── types/                # API 类型定义
├── assets/                   # 静态资源
├── components/               # 可复用 UI 组件
│   ├── BatchRenameModal/     # 批量重命名
│   ├── DetailModal/          # 文件详情
│   ├── FileExplorer/         # 文件浏览器
│   ├── FolderModal/          # 文件夹操作
│   ├── NewFolderModal/       # 新建文件夹
│   └── RenameModal/          # 重命名
├── composables/              # 组合式函数（下载/上传管理、更新检查、字幕控制等）
├── layout/                   # 布局组件
│   └── components/           # 布局子组件（离线下载弹窗、搜索弹窗）
├── router/                   # 路由配置
├── store/                    # Pinia 状态管理
├── styles/                   # 样式文件（Tailwind CSS）
├── utils/                    # 工具函数
│   ├── http/                 # HTTP 适配器（Alova、Tauri）
│   └── subtitles/            # 字幕解析与渲染（ASS/文本）
└── views/                    # 页面视图
    ├── About/                # 关于
    ├── CloudDownload/        # 云下载（离线下载）
    ├── Download/             # 下载管理
    ├── Home/                 # 首页（文件列表）
    ├── Login/                # 登录
    ├── RecycleBin/           # 回收站
    ├── Setting/              # 设置
    ├── Upload/               # 上传管理
    ├── UserInfo/             # 用户信息
    └── VideoPlayer/          # 视频播放器
src-tauri/                    # Tauri 后端（Rust）
├── src/
│   ├── lib.rs                # 主程序（Aria2 管理、插件注册）
│   ├── main.rs               # 入口
│   ├── subtitle.rs           # 字幕处理
│   ├── tray.rs               # 系统托盘
│   ├── download/             # 下载引擎（多线程、断点续传、队列管理、持久化）
│   └── upload/               # 上传引擎（SHA1 计算、OSS 分片上传、队列管理）
└── capabilities/             # Tauri 权限配置
```

## 开发说明

### 环境要求

- **Node.js**：最新 LTS 版本
- **包管理器**：pnpm（通过 Corepack 启用）
- **Rust**：最新稳定版
- **IDE**：[VS Code](https://code.visualstudio.com/) + [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

### 115 网盘开放平台配置

在项目根目录创建 `.env` 文件，填入你的 AppID 和 AppKey：

```conf
VITE_APP_ID=你的AppID
VITE_APP_KEY=你的AppKey
```

### 开发

```bash
pnpm install
pnpm tauri:dev
```

### 构建

```bash
# 先生成签名
pnpm tauri signer generate -w ~/.tauri/myapp.key
# 然后将签名私钥放到环境变量中，公钥放到tauri.conf.json中
export TAURI_SIGNING_PRIVATE_KEY="私钥路径或者私钥内容"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="如果有密码"

pnpm tauri:build
```

## 交流

QQ群：[978180785](https://qm.qq.com/q/s2vhxOL8uk)

## 致谢

感谢 [LINUX DO](https://linux.do/) 的佬友的支持。

## 许可证

[MIT](LICENSE)
