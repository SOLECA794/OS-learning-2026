# 06 · 网络设计

## 1. 目标

提供 Linux socket 语义（TCP/UDP loopback 与 virtio-net），支撑 CAgent 并发 TCP 测例，并把网络做成差异化亮点（自研协议栈）。

## 2. 决策

| 项 | 决策 | 理由 |
|---|---|---|
| socket 抽象 | 自研，对齐 Linux socket API（socket/bind/listen/accept/connect/send/recv/poll） | CAgent 有并发 TCP 测例，语义必须可控 |
| TCP/UDP | 自研协议栈 | 差异化亮点 + 完全自研纪律 |
| 底层 | virtio-net（virtio-mmio / virtio-pci + MSI 按架构） | QEMU 与实板标准设备 |
| 过渡策略 | 允许 M1-M2 暂借 smoltcp 打通功能，**带退出条件**（替换为自研栈后移除，不留双路径） | 降低早期工程量风险，最终交付自研 |

## 3. 组成

```
src/net/
├── socket/      socket 抽象：socket 对象、地址族（AF_INET）、fd 集成
├── tcp/         TCP：状态机、发送/接收队列、阻塞/唤醒、超时重传（起点简化：不追求完整拥塞控制）
├── udp/         UDP：无连接语义
├── pkt/         数据包收发路径（与 virtio-net 驱动对接）
└── driver/      virtio-net 驱动（或引用 src/driver/ 下统一 virtio）
```

## 4. 关键语义（对齐 Linux）

| 操作 | 语义要求 |
|---|---|
| socket | 域/类型/协议校验、fd 关联、错误路径 |
| bind/listen/accept | 地址绑定、监听队列、accept 阻塞唤醒 |
| connect | 非阻塞语义（EINPROGRESS）、失败清理 |
| send/recv | 阻塞/非阻塞、部分收发、MSG_* flag、errno |
| poll/epoll | 与事件框架集成（Event 位掩码唤醒） |
| 并发 | 多连接同时收发无互扰（CAgent 并发 TCP 测例） |

## 5. 性能关注点

- 数据路径零拷贝优先（页缓存/用户缓冲直接映射，按硬件能力）。
- 唤醒路径走事件框架（阻塞 socket 注册事件，避免轮询）。
- 每连接缓冲管理避免无界分配（CAgent 压力下稳定）。

## 6. 验收标准

1. TCP loopback：单连接收发、并发多连接、大包、半关闭（shutdown）测例通过。
2. UDP：收发 + 错误路径。
3. 非阻塞 + poll/epoll 组合测例通过（一等奖：epoll 支持 Tokio 异步为加分项）。
4. 双架构 virtio-net 路径一致（差异收敛在 arch 层）。
5. 过渡 smoltcp（若使用）到期移除，无残留依赖。
6. 基准：loopback 吞吐与延迟纳入性能台账（netperf/iperf 类脚本，参考一等奖做法）。
