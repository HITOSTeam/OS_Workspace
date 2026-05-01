# 协作指南

本项目当前采用“一个主工作区仓库 + 两个独立子仓库”的协作方式：

- 根目录仓库：承载主工作区内容，例如 `user/`、`vendor/`、`ext4-fs/`、`ext4-fs-packer/`、脚本、配置和说明文档
- `os/`：独立仓库，专门维护内核代码
- `OSGuide/`：独立仓库，专门维护设计文档、测试进度和工作记录

## 一、第一次获取代码

在一台空机器上执行：

```sh
git clone <根仓库地址> CongCore
cd CongCore
git submodule update --init --recursive
```

执行完成后，你会得到：

- 根仓库内容
- `os/` 子仓库
- `OSGuide/` 子仓库

建议随后检查一次状态：

```sh
bash tools/status_all.sh
```

## 二、当前仓库边界

请严格按下面规则提交代码：

- 修改 `os/` 下内容：在 `os/` 仓库提交
- 修改 `OSGuide/` 下内容：在 `OSGuide/` 仓库提交
- 修改其他内容：在根仓库提交

目前通常属于根仓库的目录包括：

- `user/`
- `vendor/`
- `ext4-fs/`
- `ext4-fs-packer/`
- `tools/`
- 顶层 `Makefile`、说明文档、配置文件等

## 三、开始一个新任务

建议三个仓库使用同名分支，方便追踪同一任务。

例如任务名为 `feat/ltp-semop-batch1`：

```sh
git checkout -b feat/ltp-semop-batch1
git -C os checkout -b feat/ltp-semop-batch1
git -C OSGuide checkout -b feat/ltp-semop-batch1
```

如果某个任务不涉及 `os/` 或 `OSGuide/`，对应仓库可以不创建分支。

## 四、日常开发流程

### 1. 修改代码

按仓库边界修改：

- 内核相关改动放到 `os/`
- 文档和进度维护放到 `OSGuide/`
- 其他代码和工作区内容放到根仓库

### 2. 查看状态

推荐统一查看：

```sh
bash tools/status_all.sh
```

也可以分别查看：

```sh
git status
git -C os status
git -C OSGuide status
```

### 3. 提交代码

根仓库提交：

```sh
git add .
git commit -m "..."
```

`os/` 提交：

```sh
git -C os add .
git -C os commit -m "..."
```

`OSGuide/` 提交：

```sh
git -C OSGuide add .
git -C OSGuide commit -m "..."
```

注意：修改了 `os/` 或 `OSGuide/` 后，除了在子仓库里提交，还需要回到根仓库记录 submodule 指针变化：

```sh
git add os OSGuide
git commit -m "chore: update submodule pointers"
```

如果当前任务只需要在最终合并时统一记录指针，也可以在最后再做这一步。

### 4. 更新 workspace 指针前的分支检查

根仓库记录的是 `os/`、`OSGuide/` 子仓库的具体 commit，而不是分支名。因此更新 submodule 指针前，必须先确认子仓库停在团队希望共享的主线提交上。

如果 `os/` 或 `OSGuide/` 的改动已经合并回各自的 `main`，请先把对应子仓库切回 `main` 并更新到远端主线，再回到根仓库记录指针：

```sh
git -C os checkout main
git -C os pull --ff-only

git -C OSGuide checkout main
git -C OSGuide pull --ff-only

git add os OSGuide
git commit -m "chore: update submodule pointers"
```

不要在子仓库仍停留在个人开发分支时直接 `git add os OSGuide`。否则根仓库会把 workspace 指针记录到个人分支上的 commit，其他人更新 workspace 时就会被带到这个分支提交，而不是合并后的主线提交。

## 五、拉取别人最新代码

只执行根仓库 `git pull` 不够，还需要更新子仓库：

```sh
git pull
git submodule update --init --recursive
```

如果你正在 `os/` 或 `OSGuide/` 的独立分支上开发，也可以分别拉取：

```sh
git -C os pull
git -C OSGuide pull
```

然后回到根仓库更新 submodule 指针。

## 六、推送代码

分别推送对应仓库：

```sh
git push
git -C os push
git -C OSGuide push
```

不要只推根仓库而忘记推 `os/` 或 `OSGuide/`，否则别人拿到的 submodule 指针可能指向一个他们无法访问或尚未推送的提交。

## 七、集成验证

推荐在根目录执行集成测试：

```sh
ARCH=riscv64 bash os/run.sh
```

必要时查看输出：

```sh
sed -n '1,200p' output.md
python3 tools/find_ltp_error.py output.md
```

## 八、不要在已合并的分支上继续开发

PR 合并后，**必须为下一个任务创建新分支**，不要在同一个老分支上继续提交。

原因：当你在老分支上继续工作，然后对 `main` 做 `rebase` 时，Git 会改写所有 commit 的 hash（因为 parent 变了）。此时本地分支和远端的老分支就会分叉——内容相同但 hash 不同，普通 `push` 会被拒绝，只能 force push。如果有协作者基于远端老分支工作，force push 会破坏他们的历史。

正确做法：

```sh
# PR 合并后，回到 main 并拉取最新
git checkout main
git pull --ff-only

# 为新任务创建新分支
git checkout -b feat/next-task
```

## 九、常见错误

- 在根仓库修改了 `os/`，但没有进入 `os/` 提交
- 修改了 `OSGuide/`，但没有单独提交文档仓库
- 只执行 `git pull`，没有更新 submodule
- 提交了日志、镜像、调试产物
- 修改了 `submit_repo/` 或 `exampleOs/`，却误以为这是主开发目录

## 九、协作约定建议

建议团队统一遵守以下约定：

- 同一任务跨仓库使用同名分支
- 提交信息写明修改范围
- 合并前至少完成一次根目录集成验证
- `os/` 和 `OSGuide/` 的提交先推送，再更新根仓库 submodule 指针

## 十、遇到问题先检查什么

按顺序检查：

```sh
bash tools/status_all.sh
git submodule status
git status
git -C os status
git -C OSGuide status
```

如果状态不干净，先确认改动属于哪个仓库，再决定提交、暂存或清理。
