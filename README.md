# OS-learning

操作系统学习与竞赛项目工作区（公开仓库）。

## 仓库结构

```
OS-learning/
├── kernel-project/     # 竞赛内核项目治理文件（基础 + 设计 + 开发，见其 README）
├── sdd-governance/     # SDD 开发治理体系（AGENTS.md + skills + templates）
└── T2026*…/            # 参考项目（2026 操作系统大赛一等奖作品，git submodule）
```

## 参考项目（submodule）

以下目录为 git submodule，指向竞赛平台（gitlab.eduxiji.net）的公开仓库，用于学习与对标：

| 目录 | 项目 | 定位 |
|---|---|---|
| T2026103369910030-2889 | KernelX | 自研类 UNIX 宏内核 |
| T2026103369910596-2192 | CosmOS | rCore 体系实验内核 |
| T202610617999569-3165 | MossOS | ArceOS/Starry 二次开发 |
| T202612332999758-606 | RyOS | 异步多核（继承 Chronix 重写） |
| T2026181239910972-3450 | Anemone | 完全自研多核宏内核 |
| T2026181239910993-776 | Kairix | polyhal 系多核宏内核 |

拉取子模块：

```bash
git submodule update --init --recursive
```

## 说明

- 参考项目为第三方竞赛作品，版权归原作者/团队；本仓库以 submodule 引用，不包含其代码副本。
- 自研内容（kernel-project / sdd-governance）为本仓库所有，许可见对应目录。
