# Mihomoctl Core

用于与 Mihomo RESTful API 交互的库。该 crate 不包含二进制程序；CLI 与 TUI 工具位于 `mihomoctl`。

当前仓库地址：`git@github.com:zouxiaoliang/mihomoctl.git`

本项目 fork 自 [George-Miao/clashctl](https://github.com/George-Miao/clashctl)。当前版本在原项目基础上补充了 [MetaCubeX Mihomo API 文档](https://wiki.metacubex.one/api/) 中的接口。

接口补充范围包括日志、流量、内存、版本、缓存、运行配置、更新、策略组、代理与代理集合、规则与规则集合、连接、DNS 查询、存储和 DEBUG 等端点。

## RESTful API 方法

`Clash` 提供的函数：

| 函数名                    | 方法   | 端点                                 |
| ------------------------- | ------ | ------------------------------------ |
| `get_version`             | GET    | /logs                                |
| `get_traffic`             | GET    | /traffic                             |
| `get_version`             | GET    | /version                             |
| `get_configs`             | GET    | /config                              |
| `reload_configs`          | PUT    | /config                              |
| **TODO**                  | PATCH  | /config                              |
| `get_proxies`             | GET    | /proxies                             |
| `get_proxy`               | GET    | /proxies/:name                       |
| `set_proxygroup_selected` | PUT    | /proxies/:name                       |
| `get_proxy_delay`         | GET    | /proxies/:name/delay                 |
| `get_rules`               | GET    | /rules                               |
| `get_connections`         | GET    | /connections                         |
| `close_connections`       | DELETE | /connections                         |
| `close_one_connection`    | DELETE | /connections/:id                     |
| **TODO**                  | GET    | /providers/proxies                   |
| **TODO**                  | GET    | /providers/proxies/:name             |
| **TODO**                  | PUT    | /providers/proxies/:name             |
| **TODO**                  | GET    | /providers/proxies/:name/healthcheck |
