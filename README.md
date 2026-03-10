# CongCore Workspace

这是项目主工作区仓库。

当前仓库结构采用：

- 根仓库：维护主工作区内容，例如 `user/`、`vendor/`、`ext4-fs/`、`ext4-fs-packer/`、工具脚本和顶层配置
- `os/`：独立仓库，维护内核代码
- `OSGuide/`：独立仓库，维护设计文档和测试进度

## 快速开始

首次获取代码：

```sh
git clone <根仓库地址> CongCore
cd CongCore
git submodule update --init --recursive
```

查看当前工作区状态：

```sh
bash tools/status_all.sh
```

运行一次集成测试：

```sh
ARCH=riscv64 bash os/run.sh
```

## 提交规则

- 修改 `os/`：在 `os/` 仓库提交
- 修改 `OSGuide/`：在 `OSGuide/` 仓库提交
- 修改其他内容：在根仓库提交

如果修改了 `os/` 或 `OSGuide/`，请在对应仓库提交后，再回到根仓库更新 submodule 指针。

## 进一步说明

- 协作流程见 [COLLABORATION.md](./COLLABORATION.md)
- 工作区边界说明见 [WORKSPACE.md](./WORKSPACE.md)
