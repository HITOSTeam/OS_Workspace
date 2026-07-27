use anyhow::Context;
use clap::Parser;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::process::Command;

/// Ext4 packer that creates an ext4 image with:
///   /user - user binaries
///   /extra - additional files (optional)
/// Arch-specific extra overlays can be layered after the common extra tree.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory containing user binaries (will be placed in /user)
    #[arg(short = 'u', long = "user")]
    user_dir: PathBuf,

    /// Optional extra directory to pack (will be placed in /extra)
    #[arg(short = 'e', long = "extra")]
    extra_dir: Option<PathBuf>,

    /// Optional architecture-specific extra directory overlaid after --extra
    #[arg(long = "arch-extra")]
    arch_extra_dir: Option<PathBuf>,

    /// Optional base ext4 image to seed filesystem contents
    #[arg(short = 'b', long = "base-image")]
    base_image: Option<PathBuf>,

    /// Build the evaluation bootstrap disk only: copy init_proc.bin and
    /// 0final_init.bin to /user, plus the two root-level evaluation scripts.
    /// This deliberately skips normal rootfs overlays so their libc/loader
    /// files cannot be mixed with the official evaluation image.
    #[arg(long = "minimal-eval-root")]
    minimal_eval_root: bool,

    /// Target directory to write the image into (host path)
    #[arg(short = 't', long = "target")]
    target: PathBuf,

    /// Image size, e.g. 16M, 64M, 1G. Default: 64M
    #[arg(short = 'S', long = "size", default_value = "64M")]
    size: String,

    /// Image file name (default: fs.ext4)
    #[arg(short = 'o', long = "output", default_value = "fs.ext4")]
    output: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Check user_dir exists and is a directory
    if !args.user_dir.exists() || !args.user_dir.is_dir() {
        anyhow::bail!(
            "user dir '{}' does not exist or is not a directory",
            args.user_dir.display()
        );
    }

    let mut extra_dirs: Vec<(&str, &PathBuf)> = Vec::new();
    if let Some(ref extra) = args.extra_dir {
        extra_dirs.push(("extra", extra));
    }
    if let Some(ref arch_extra) = args.arch_extra_dir {
        extra_dirs.push(("arch extra", arch_extra));
    }
    for (label, extra) in &extra_dirs {
        if !extra.exists() || !extra.is_dir() {
            anyhow::bail!(
                "{} dir '{}' does not exist or is not a directory",
                label,
                extra.display()
            );
        }
    }

    // Check base image if provided
    if let Some(ref base_image) = args.base_image {
        if !base_image.is_file() {
            anyhow::bail!(
                "base image '{}' does not exist or is not a file",
                base_image.display()
            );
        }
    }

    // Ensure target exists
    fs::create_dir_all(&args.target)?;

    // Create a temporary staging directory with the desired layout
    let staging_dir = args.target.join("_staging");
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)?;
    }
    fs::create_dir_all(&staging_dir)?;

    // Seed staging root from base image if provided.
    if let Some(ref base_image) = args.base_image {
        seed_from_base_image(&staging_dir, base_image)?;
    }

    // Create /user directory in staging
    let staging_user = staging_dir.join("user");
    fs::create_dir_all(&staging_user)?;

    if args.minimal_eval_root {
        copy_minimal_eval_root(&args.user_dir, args.extra_dir.as_ref(), &staging_dir)?;
    } else {
        // Create standard runtime directories expected by many Unix userland programs.
        // In particular, iperf3 uses `mkstemp("/tmp/iperf3.XXXXXX")` per stream.
        create_standard_dirs(&staging_dir)?;

        // Copy user binaries to staging/user.
        println!(
            "Copying user binaries from '{}'...",
            args.user_dir.display()
        );
        copy_dir_contents(&args.user_dir, &staging_user)?;

        // The common tree is copied first, then the arch-specific tree can replace
        // binaries and rootfs files with target-architecture versions.
        for (label, extra) in &extra_dirs {
            copy_extra_overlay(label, extra, &staging_dir)?;
        }
    }

    // Ensure OSComp shell scripts are executable inside the ext4 image.
    // The source tree may contain `.sh` files with 0644 mode; without +x,
    // scripts invoked as `./foo.sh` will fail in busybox/ash.
    fix_shell_script_modes(&staging_dir)?;

    let image_path = args.target.join(&args.output);
    // Ensure we don't reuse a partially-written/corrupted image from a previous failed run.
    if image_path.exists() {
        fs::remove_file(&image_path)?;
    }

    // Check mke2fs availability
    if Command::new("mke2fs").arg("-V").output().is_err() {
        eprintln!(
            "`mke2fs` not found. Please install e2fsprogs (provides mke2fs).\nOn Debian/Ubuntu: sudo apt install e2fsprogs"
        );
        std::process::exit(1);
    }

    // Build arguments: mke2fs -t ext4 -F -b 4096 -O ... -d <staging> <image> <size>
    // Use 4096 byte block size for compatibility with our ext4-fs implementation.
    // Disable journal/metadata checksums since our ext4-fs driver doesn't update them.
    println!(
        "Creating ext4 image '{}' of size {} (block size: 4096)...",
        image_path.display(),
        args.size,
    );
    let status = Command::new("mke2fs")
        .arg("-t")
        .arg("ext4")
        .arg("-F")
        .arg("-b")
        .arg("4096") // Force 4096 byte block size
        .arg("-O")
        .arg("^has_journal,^metadata_csum,^metadata_csum_seed")
        .arg("-d")
        .arg(staging_dir.as_os_str())
        .arg(image_path.as_os_str())
        .arg(&args.size)
        .status()?;

    if !status.success() {
        anyhow::bail!(
            "mke2fs failed with exit code: {}",
            status.code().unwrap_or(-1)
        );
    }

    // Clean up staging directory
    fs::remove_dir_all(&staging_dir)?;

    println!("Image created: {}", image_path.display());
    println!("Contents:");
    if args.minimal_eval_root {
        println!("  /user/init_proc.bin and /user/0final_init.bin");
        println!("  /buildstorm_testcode.sh and /cagent_testcode.sh");
    } else {
        println!("  /user  - user binaries");
    }
    if !args.minimal_eval_root && !extra_dirs.is_empty() {
        println!("  /extra - additional files");
    }
    if let Some(ref base_image) = args.base_image {
        println!("  base   - {}", base_image.display());
    }

    Ok(())
}

/// Populate the disk used beside an official evaluation rootfs.
///
/// Keep this allow-list intentionally small.  In particular, do not copy
/// `rootfs/lib`, `rootfs/usr`, or architecture overlays: those can contain a
/// different dynamic loader or libc than the official disk.
fn copy_minimal_eval_root(
    user_dir: &PathBuf,
    extra_dir: Option<&PathBuf>,
    staging_dir: &PathBuf,
) -> anyhow::Result<()> {
    println!("Building minimal evaluation bootstrap disk...");
    create_minimal_eval_dirs(staging_dir)?;
    let staging_user = staging_dir.join("user");
    for name in ["init_proc.bin", "0final_init.bin"] {
        copy_required_file(&user_dir.join(name), &staging_user.join(name))?;
    }

    let extra_dir = extra_dir.context("--minimal-eval-root requires --extra for test scripts")?;
    let scripts = extra_dir.join("rootfs");
    for name in ["buildstorm_testcode.sh", "cagent_testcode.sh"] {
        copy_required_file(&scripts.join(name), &staging_dir.join(name))?;
    }
    Ok(())
}

/// Directories required by the two evaluation scripts, without importing any
/// executable or runtime-library file from the local overlay.  `/work` is
/// intentionally omitted: it must continue to resolve to the official image
/// that contains the BuildStorm workload.
fn create_minimal_eval_dirs(staging_root: &PathBuf) -> anyhow::Result<()> {
    let tmp_dir = staging_root.join("tmp");
    fs::create_dir_all(&tmp_dir)?;
    fs::set_permissions(&tmp_dir, fs::Permissions::from_mode(0o1777))?;
    // `/home` is the rootfs marker used by the kernel when selecting disk0 or
    // disk1 as the primary filesystem.  It is deliberately empty here.
    for name in ["proc", "sys", "dev", "home"] {
        fs::create_dir_all(staging_root.join(name))?;
    }
    Ok(())
}

fn copy_required_file(src: &PathBuf, dst: &PathBuf) -> anyhow::Result<()> {
    if !src.is_file() {
        anyhow::bail!("required minimal-eval file '{}' is missing", src.display());
    }
    let parent = dst
        .parent()
        .context("minimal-eval destination must have a parent directory")?;
    fs::create_dir_all(parent)?;
    fs::copy(src, dst).with_context(|| {
        format!("copy required minimal-eval file '{}' to '{}'", src.display(), dst.display())
    })?;
    println!("  -> {}", dst.display());
    Ok(())
}

fn copy_extra_overlay(label: &str, extra: &PathBuf, staging_dir: &PathBuf) -> anyhow::Result<()> {
    let staging_extra = staging_dir.join("extra");
    fs::create_dir_all(&staging_extra)?;
    println!("Copying {} files from '{}'...", label, extra.display());
    copy_dir_contents(extra, &staging_extra)?;

    let rootfs_overlay = extra.join("rootfs");
    if rootfs_overlay.is_dir() {
        println!(
            "Overlaying {} rootfs files from '{}'...",
            label,
            rootfs_overlay.display()
        );
        copy_dir_contents(&rootfs_overlay, staging_dir)?;
    }

    // UnixBench scripts create temp files in their working directory
    // (e.g., `sort.$$` in `tst.sh`). Ensure those dirs are writable.
    make_unixbench_dirs_writable(staging_dir)?;
    Ok(())
}

fn seed_from_base_image(staging_root: &PathBuf, base_image: &PathBuf) -> anyhow::Result<()> {
    // `debugfs` is provided by e2fsprogs (same as mke2fs).
    if Command::new("debugfs").arg("-V").output().is_err() {
        eprintln!(
            "`debugfs` not found. Please install e2fsprogs (provides debugfs).\nOn Debian/Ubuntu: sudo apt install e2fsprogs"
        );
        std::process::exit(1);
    }
    let rdump_cmd = format!("rdump / {}", staging_root.display());
    let status = Command::new("debugfs")
        .arg("-R")
        .arg(rdump_cmd)
        .arg(base_image.as_os_str())
        .status()?;
    if !status.success() {
        let has_seeded_files = fs::read_dir(staging_root)?.next().is_some();
        if !has_seeded_files {
            anyhow::bail!(
                "debugfs rdump failed with exit code: {}",
                status.code().unwrap_or(-1)
            );
        }
        eprintln!(
            "warning: debugfs rdump exited with code {}, continuing with copied contents",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

fn create_standard_dirs(staging_root: &PathBuf) -> anyhow::Result<()> {
    let tmp_dir = staging_root.join("tmp");
    fs::create_dir_all(&tmp_dir)?;
    // 01777: world-writable + sticky bit, like Linux /tmp.
    fs::set_permissions(&tmp_dir, fs::Permissions::from_mode(0o1777))?;
    fs::create_dir_all(staging_root.join("bin"))?;
    fs::create_dir_all(staging_root.join("usr/bin"))?;
    // Keep the rootfs selection marker in the full patched-image mode too.
    fs::create_dir_all(staging_root.join("home"))?;
    // iproute2 persists named network namespaces under /var/run/netns.
    fs::create_dir_all(staging_root.join("var/run/netns"))?;
    fs::create_dir_all(staging_root.join("run/netns"))?;
    // LTP controller scripts store intermediate logs under `$LTPROOT/output`.
    // Keep these runtime output directories available in the base image so
    // shell redirections do not fail before the actual test logic runs.
    fs::create_dir_all(staging_root.join("musl/ltp/output"))?;
    fs::create_dir_all(staging_root.join("glibc/ltp/output"))?;
    Ok(())
}

fn make_unixbench_dirs_writable(staging_root: &PathBuf) -> anyhow::Result<()> {
    let candidates = [
        staging_root.join("extra/riscv/musl"),
        staging_root.join("extra/riscv/glibc"),
    ];
    for dir in candidates {
        if dir.is_dir() {
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o777))?;
        }
    }
    Ok(())
}

fn fix_shell_script_modes(root: &PathBuf) -> anyhow::Result<()> {
    fn walk(dir: &std::path::Path) -> anyhow::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                walk(&path)?;
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let data = fs::read(&path)?;
            if !data.starts_with(b"#!") {
                continue;
            }
            let mut perm = fs::metadata(&path)?.permissions();
            let mode = perm.mode();
            // Treat any shebang file as an executable script, even when it
            // has no `.sh` suffix (for example `/extra/bin/mount` wrappers).
            perm.set_mode(mode | 0o111);
            fs::set_permissions(&path, perm)?;
        }
        Ok(())
    }
    walk(root.as_path())
}

/// Copy all files and subdirectories from src to dst
fn copy_dir_contents(src: &PathBuf, dst: &PathBuf) -> anyhow::Result<()> {
    for entry in fs::read_dir(src).with_context(|| format!("read '{}'", src.display()))? {
        let entry = entry.with_context(|| format!("read entry in '{}'", src.display()))?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            if let Ok(metadata) = fs::symlink_metadata(&dst_path) {
                if !metadata.file_type().is_dir() {
                    fs::remove_file(&dst_path)
                        .with_context(|| format!("remove existing '{}'", dst_path.display()))?;
                }
            }
            fs::create_dir_all(&dst_path)
                .with_context(|| format!("create directory '{}'", dst_path.display()))?;
            copy_dir_contents(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            if let Ok(metadata) = fs::symlink_metadata(&dst_path) {
                if metadata.file_type().is_dir() {
                    fs::remove_dir_all(&dst_path)
                        .with_context(|| format!("remove existing '{}'", dst_path.display()))?;
                } else {
                    fs::remove_file(&dst_path)
                        .with_context(|| format!("remove existing '{}'", dst_path.display()))?;
                }
            }
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!("copy '{}' to '{}'", src_path.display(), dst_path.display())
            })?;
            println!("  -> {}", entry.file_name().to_string_lossy());
        } else if file_type.is_symlink() {
            if let Ok(metadata) = fs::symlink_metadata(&dst_path) {
                if metadata.file_type().is_dir() {
                    fs::remove_dir_all(&dst_path)
                        .with_context(|| format!("remove existing '{}'", dst_path.display()))?;
                } else {
                    fs::remove_file(&dst_path)
                        .with_context(|| format!("remove existing '{}'", dst_path.display()))?;
                }
            }
            let target = fs::read_link(&src_path)
                .with_context(|| format!("read link '{}'", src_path.display()))?;
            symlink(&target, &dst_path).with_context(|| {
                format!("symlink '{}' to '{}'", dst_path.display(), target.display())
            })?;
            println!(
                "  -> {} -> {}",
                entry.file_name().to_string_lossy(),
                target.display()
            );
        }
    }
    Ok(())
}
