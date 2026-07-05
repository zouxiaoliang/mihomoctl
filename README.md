# Mihomoctl

## 简介 <a name = "about"></a>

Mihomoctl 是一个易用的 TUI 与 CLI 工具，用于和 [Mihomo](https://github.com/MetaCubeX/mihomo) RESTful API 交互。

当前仓库地址：`git@github.com:zouxiaoliang/mihomoctl.git`

本项目 fork 自 [George-Miao/clashctl](https://github.com/George-Miao/clashctl)。当前版本在原项目基础上改名为 `mihomoctl`，并补充了 [MetaCubeX Mihomo API 文档](https://wiki.metacubex.one/api/) 中的接口。

接口补充范围包括日志、流量、内存、版本、缓存、运行配置、更新、策略组、代理与代理集合、规则与规则集合、连接、DNS 查询、存储和 DEBUG 等端点。

## 截图 <a name = "screenshots"></a>

### 状态面板

![状态面板](https://imagedelivery.net/b21oeeg7p6hqWEI-IA5xDw/be2ffc2e-4193-4418-0d0f-b82624f0c800/public)

### 代理面板

![代理面板](https://imagedelivery.net/b21oeeg7p6hqWEI-IA5xDw/0166f654-c5c2-4b0a-e401-8d5b93d3f500/public)

## 安装 <a name = "installing"></a>

### 下载发布二进制

macOS 和 Linux x86 用户可以在当前仓库的 Releases 页面查找已编译二进制。

原始上游 release 页面仍保留为参考：[https://github.com/George-Miao/clashctl/releases](https://github.com/George-Miao/clashctl/releases)。

### 从源码编译

```bash
$ git clone git@github.com:zouxiaoliang/mihomoctl.git
$ cd mihomoctl
$ cargo install --path ./mihomoctl # 这里的路径不是写错了：同名子目录中包含二进制 crate
```

## 快速开始 <a name = "getting_started"></a>

首先添加一个 API 服务器：

```bash
$ mihomoctl server add
# 按提示填写配置
```

不带子命令运行时，默认打开 TUI：

```bash
$ mihomoctl

# 等同于

$ mihomoctl tui
```

也可以使用子命令进入 CLI 流程：

```bash
$ mihomoctl proxy list

---------------------------------------------------------
TYPE                DELAY   NAME
---------------------------------------------------------
selector            -       All

    URLTest         -       Auto-All
    ShadowsocksR    19      SomeProxy-1
    Vmess           177     SomeProxy-2
    Vmess           137     SomeProxy-3
    Shadowsocks     143     SomeProxy-4

---------------------------------------------------------
```

## 功能 <a name = "features"></a>

- 美观的终端界面
- 切换代理
- 展示代理列表，支持过滤与排序，支持普通模式和分组模式
- 保存并使用多个服务器
- 补充支持 MetaCubeX Mihomo API 文档中的接口
- 生成 shell 补全脚本（基于 [clap_generate](https://crates.io/crates/clap_generate)）
- 管理多个服务器

### 已完成与待办 <a name = "todo"></a>

- [ ] CLI
  - [x] 管理服务器
  - [x] 代理排序
  - [ ] 更多功能
- [ ] TUI
  - [x] 状态面板
  - [x] 代理面板
    - [x] 更新代理
    - [x] 测试延迟
    - [x] 按 {原始顺序、延迟升序、延迟降序、名称升序、名称降序} 排序
  - [x] 规则面板
  - [x] 连接面板
    - [ ] 排序
  - [x] 日志面板
  - [x] 调试面板
  - [ ] 配置面板
    - [ ] 更新 mihomo 配置
    - [ ] 更新 mihomoctl 配置
  - [ ] 搜索
  - [ ] 鼠标支持（可能）

## 前置条件 <a name = "prerequisites"></a>

编译和安装需要 nightly Rust 环境（Cargo 与 rustc）。

## 使用方式 <a name = "usage"></a>

### 使用 TUI

- 暂时需要先通过 CLI 配置服务器
- 使用数字键在标签页之间切换
- 按空格键锁定列表，之后可以移动列表
- 在锁定模式下使用方向键移动列表
- 按 `Ctrl-D` 打开调试面板

### 使用 CLI

```text
$ mihomoctl -h
mihomoctl

George Miao <gm@miao.dev>

Cli & Tui used to interact with Mihomo RESTful API

USAGE:
    mihomoctl [OPTIONS] [SUBCOMMAND]

OPTIONS:
    -c, --config-path <CONFIG_PATH>    Path of config file. Default to ~/.config/mihomoctl/config.ron
        --config-dir <CONFIG_DIR>      Path of config directory. Default to ~/.config/mihomoctl
    -h, --help                         Print help information
    -t, --timeout <TIMEOUT>            Timeout of requests, in ms [default: 2000]
        --test-url <TEST_URL>          Url for testing proxy endpointes [default: http://
                                       www.gstatic.com/generate_204]
    -v, --verbose                      Verbosity. Default: INFO, -v DEBUG, -vv TRACE
    -V, --version                      Print version information

SUBCOMMANDS:
    completion    Generate auto-completion scripts
    help          Print this message or the help of the given subcommand(s)
    proxy         Interacting with proxies
    server        Interacting with servers
    tui           Open TUI
```

### 作为 crate 使用

```toml
# Cargo.toml

[dependencies]
mihomoctl-core = "*" # 不要添加 `mihomoctl`；它是二进制 crate，`mihomoctl-core` 才包含 API 能力。
```

在项目中使用：

```rust
use mihomoctl_core::Clash;

fn main() {
  let clash = Clash::builder("http://example.com:9090").unwrap().build();
  println!("Mihomo version is {:?}", clash.get_version().unwrap())
}
```

## 开发 <a name = "development"></a>

`mihomoctl` 提供了 [`justfile`](https://github.com/casey/just) 来加速开发。
其中 `just dev` 使用 [`cargo-watch`](https://github.com/watchexec/cargo-watch) 实现了类似前端开发的热重载流程。

### [`Just`](https://github.com/casey/just) 命令

#### `just dev`（别名：`d`）

热重载开发。启用所有功能，并在 `cargo check` 通过后自动重新运行。

#### `just run {{ Args }}`（别名：`r`）

运行 CLI 与 UI 功能。

#### `just ui`

仅运行 UI。

#### `just cli`

仅运行 CLI。

#### `just build`（别名：`b`）

以 release 模式构建 CLI 与 UI 功能。

#### `just add`

预留命令。

### 项目结构

```bash
$ tree -L 2
├── mihomoctl                # 二进制 crate，包含 CLI 与 TUI
├── mihomoctl-core           # API 交互 crate
└── ...
```
