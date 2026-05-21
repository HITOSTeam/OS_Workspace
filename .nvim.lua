-- 项目级 rust-analyzer 调优(no_std 内核场景)。
--
-- 需要在 ~/.config/nvim/init.lua 顶部启用一次:
--     vim.o.exrc = true
-- 出于安全考虑,nvim 默认不会 source 项目根的 .nvim.lua —— exrc 打开后,nvim 启动
-- 时会询问你是否信任这个文件(也可以用 `vim.secure.trust` 一次性允许)。
--
-- 配合本目录的 .cargo/config.toml 使用:
--   - .cargo/config.toml 声明 riscv64 target → 让 #[cfg(target_arch="riscv64")] 激活
--   - 这里把 rust-analyzer 的 check 调成 no_std 友好,避免拉 test/bench target 炸掉
--
-- 实现:用 vim.tbl_deep_extend 合并到现有的 vim.g.rustaceanvim 上,不覆盖你
-- ~/.config/nvim/lua/custom/plugins/init.lua 里设的其它默认项(inlay hints 等)。

vim.g.rustaceanvim = vim.tbl_deep_extend('force', vim.g.rustaceanvim or {}, {
  server = {
    default_settings = {
      ['rust-analyzer'] = {
        cargo = {
          target = 'riscv64gc-unknown-none-elf',
          allFeatures = false,
        },
        check = {
          -- no_std 内核没有 test/bench target,allTargets=true 会让 ra 跑
          -- `cargo check --all-targets` 然后在 test/bench 编译阶段炸出无意义错误。
          allTargets = false,
          command = 'check',
          extraArgs = { '--target', 'riscv64gc-unknown-none-elf' },
        },
      },
    },
  },
})
