use std::io::{ErrorKind, Write};

use crate::loader::load_module;
use anyhow::Result;
use rustix::fs::{chmodat, symlink, unlink, AtFlags, Mode};
use rustix::{
    fd::AsFd,
    fs::{access, makedev, mkdir, mknodat, Access, FileType, CWD},
    mount::{
        fsconfig_create, fsmount, fsopen, move_mount, unmount, FsMountFlags, FsOpenFlags,
        MountAttrFlags, MoveMountFlags, UnmountFlags,
    },
};

use obfstr::obfstr as s;

struct AutoUmount {
    mountpoints: Vec<String>,
}

impl Drop for AutoUmount {
    fn drop(&mut self) {
        for mountpoint in self.mountpoints.iter().rev() {
            if let Err(e) = unmount(mountpoint.as_str(), UnmountFlags::DETACH) {
                log::error!("{} {}: {}", s!("Cannot umount"), mountpoint, e)
            }
        }
    }
}

fn prepare_mount() -> AutoUmount {
    let mut mountpoints = vec![];

    // mount procfs
    let result = mkdir("/proc", Mode::from_raw_mode(0o755))
        .or_else(|err| match err.kind() {
            ErrorKind::AlreadyExists => Ok(()),
            _ => Err(err),
        })
        .and_then(|_| fsopen("proc", FsOpenFlags::FSOPEN_CLOEXEC))
        .and_then(|fd| fsconfig_create(fd.as_fd()).map(|_| fd))
        .and_then(|fd| {
            fsmount(
                fd.as_fd(),
                FsMountFlags::FSMOUNT_CLOEXEC,
                MountAttrFlags::empty(),
            )
        })
        .and_then(|fd| {
            move_mount(
                fd.as_fd(),
                "",
                CWD,
                "/proc",
                MoveMountFlags::MOVE_MOUNT_F_EMPTY_PATH,
            )
        });
    match result {
        Ok(_) => mountpoints.push("/proc".to_string()),
        Err(e) => log::error!("{} {:?}", s!("Cannot mount procfs: "), e),
    }

    // mount sysfs
    let result = mkdir("/sys", Mode::from_raw_mode(0o755))
        .or_else(|err| match err.kind() {
            ErrorKind::AlreadyExists => Ok(()),
            _ => Err(err),
        })
        .and_then(|_| fsopen("sysfs", FsOpenFlags::FSOPEN_CLOEXEC))
        .and_then(|fd| fsconfig_create(fd.as_fd()).map(|_| fd))
        .and_then(|fd| {
            fsmount(
                fd.as_fd(),
                FsMountFlags::FSMOUNT_CLOEXEC,
                MountAttrFlags::empty(),
            )
        })
        .and_then(|fd| {
            move_mount(
                fd.as_fd(),
                "",
                CWD,
                "/sys",
                MoveMountFlags::MOVE_MOUNT_F_EMPTY_PATH,
            )
        });

    match result {
        Ok(_) => mountpoints.push("/sys".to_string()),
        Err(e) => log::error!("{} {:?}", s!("Cannot mount sysfs:"), e),
    }

    AutoUmount { mountpoints }
}

fn setup_kmsg() {
    const KMSG: &str = "/dev/kmsg";
    let device = match access(KMSG, Access::EXISTS) {
        Ok(_) => KMSG,
        Err(_) => {
            // try to create it
            mknodat(
                CWD,
                "/kmsg",
                FileType::CharacterDevice,
                0o666.into(),
                makedev(1, 11),
            )
            .ok();
            "/kmsg"
        }
    };

    let _ = kernlog::init_with_device(device);
}

fn unlimit_kmsg() {
    // Disable kmsg rate limiting
    if let Ok(mut rate) = std::fs::File::options()
        .write(true)
        .open(s!("/proc/sys/kernel/printk_devkmsg"))
    {
        writeln!(rate, "on").ok();
    }
}

pub fn init() -> Result<()> {
    // Setup kernel log first
    setup_kmsg();

    log::info!("{}", s!("Hello, KernelSU!"));

    // mount /proc and /sys to access kernel interface
    let _dontdrop = prepare_mount();

    // This relies on the fact that we have /proc mounted
    unlimit_kmsg();

    if has_kernelsu() {
        log::info!("{}", s!("KernelSU may be already loaded in kernel, skip!"));
    } else {
        log::info!("{}", s!("Loading kernelsu.ko.."));
        if let Err(e) = load_module(s!("/kernelsu.ko")) {
            log::error!("{}: {}", s!("Cannot load kernelsu.ko"), e);
        }
    }

    // And now we should prepare the real init to transfer control to it
    unlink("/init")?;

    let real_init = match access("/init.real", Access::EXISTS) {
        Ok(_) => "init.real",
        Err(_) => "/system/bin/init",
    };

    log::info!("{} {}", s!("init is"), real_init);
    symlink(real_init, "/init")?;

    chmodat(
        CWD,
        "/init",
        Mode::from_raw_mode(0o755),
        AtFlags::SYMLINK_NOFOLLOW,
    )?;

    Ok(())
}

fn has_kernelsu() -> bool {
    use syscalls::{syscall, Sysno};
    let mut version = 0;
    const CMD_GET_VERSION: i32 = 2;
    let ret = unsafe {
        syscall!(
            Sysno::prctl,
            0xDEADBEEF,
            CMD_GET_VERSION,
            std::ptr::addr_of_mut!(version)
        )
    };

    match ret {
        Ok(0) => {
            log::info!("{}: {}", s!("KernelSU version"), version);
            version != 0
        }
        Ok(ret_val) => {
            log::warn!(
                "{}: syscall returned unexpected value: {}",
                s!("KernelSU check"),
                ret_val
            );
            false
        }
        Err(errno) => {
            log::warn!(
                "{}: syscall failed, errno={}",
                s!("KernelSU check"),
                errno
            );
            false
        }
    }
}
