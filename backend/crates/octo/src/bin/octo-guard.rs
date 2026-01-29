//! octo-guard - FUSE filesystem for runtime file access control.
//!
//! Provides a FUSE mount that proxies access to protected files,
//! enforcing policies and requesting user approval when needed.
//!
//! ## Usage
//!
//! ```bash
//! # Start the guard
//! octo-guard --mount /tmp/octo-guard-1000 \
//!            --octo-server http://localhost:8080
//!
//! # With config
//! octo-guard --config ~/.config/octo/sandbox.toml --profile development
//! ```
//!
//! ## How it works
//!
//! 1. Mounts a FUSE filesystem at the specified path
//! 2. Guarded paths (e.g., ~/.kube) are exposed through this mount
//! 3. When a file is accessed, the guard checks policy
//! 4. For "prompt" policy, sends request to octo server and waits for approval
//! 5. If approved, proxies the read/write to the real file
//! 6. All access is logged for audit
//!
//! ## Security Model
//!
//! - **auto**: Allow access, but log it (audit trail)
//! - **prompt**: Ask user for approval, cache for session if "allow_session"
//! - **deny**: Block access (same as bwrap deny_read, but explicit)

use anyhow::{Context, Result};
use clap::Parser;
use fuser::{
    FUSE_ROOT_ID, FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData,
    ReplyDirectory, ReplyEntry, Request,
};
use glob::Pattern;
use log::{debug, error, info, warn};
use octo::local::{GuardConfig, GuardPolicy, SandboxConfig};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// TTL for FUSE attribute caching
const TTL: Duration = Duration::from_secs(1);

#[derive(Parser, Debug)]
#[command(
    name = "octo-guard",
    about = "FUSE filesystem for runtime file access control",
    after_help = "Examples:\n  \
        octo-guard --mount /tmp/octo-guard\n  \
        octo-guard --config ~/.config/octo/sandbox.toml"
)]
struct Args {
    /// Mount point for the FUSE filesystem
    #[arg(short, long)]
    mount: Option<PathBuf>,

    /// Path to sandbox config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Profile name to use from config
    #[arg(short, long, default_value = "development")]
    profile: String,

    /// Octo server URL for prompts
    #[arg(long, default_value = "http://localhost:8080")]
    octo_server: String,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Allow other users to access the mount (requires user_allow_other in /etc/fuse.conf)
    #[arg(long)]
    allow_other: bool,
}

/// Inode entry for the virtual filesystem
#[derive(Debug, Clone)]
struct InodeEntry {
    /// Real path on the filesystem
    real_path: PathBuf,
    /// Parent inode (for future use)
    #[allow(dead_code)]
    parent: u64,
    /// Name in parent directory (for future use)
    #[allow(dead_code)]
    name: String,
    /// Whether this is a directory (for future use)
    #[allow(dead_code)]
    is_dir: bool,
}

/// Approval cache key
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ApprovalKey {
    path: PathBuf,
    operation: String,
}

/// The guarded filesystem
struct GuardedFs {
    /// Guard configuration
    config: GuardConfig,

    /// Octo server URL for prompts
    octo_server: String,

    /// Inode to entry mapping
    inodes: RwLock<HashMap<u64, InodeEntry>>,

    /// Path to inode mapping
    path_to_inode: RwLock<HashMap<PathBuf, u64>>,

    /// Next inode number
    next_inode: RwLock<u64>,

    /// Policy patterns (compiled globs)
    policy_patterns: Vec<(Pattern, GuardPolicy)>,

    /// Approval cache: key -> expiry time
    approvals: RwLock<HashMap<ApprovalKey, SystemTime>>,

    /// Tokio runtime for async operations
    runtime: tokio::runtime::Handle,
}

impl GuardedFs {
    fn new(config: GuardConfig, octo_server: String, runtime: tokio::runtime::Handle) -> Self {
        // Compile policy patterns
        let policy_patterns: Vec<(Pattern, GuardPolicy)> = config
            .policy
            .iter()
            .filter_map(|(pattern, policy)| Pattern::new(pattern).ok().map(|p| (p, policy.clone())))
            .collect();

        let mut fs = Self {
            config,
            octo_server,
            inodes: RwLock::new(HashMap::new()),
            path_to_inode: RwLock::new(HashMap::new()),
            next_inode: RwLock::new(FUSE_ROOT_ID + 1),
            policy_patterns,
            approvals: RwLock::new(HashMap::new()),
            runtime,
        };

        // Initialize root inode
        {
            let mut inodes = fs.inodes.write().unwrap();
            let mut path_to_inode = fs.path_to_inode.write().unwrap();

            inodes.insert(
                FUSE_ROOT_ID,
                InodeEntry {
                    real_path: PathBuf::from("/"),
                    parent: FUSE_ROOT_ID,
                    name: String::new(),
                    is_dir: true,
                },
            );
            path_to_inode.insert(PathBuf::from("/"), FUSE_ROOT_ID);
        }

        // Initialize guarded paths
        fs.init_guarded_paths();

        fs
    }

    /// Initialize the directory structure for guarded paths
    fn init_guarded_paths(&mut self) {
        for path_str in &self.config.paths {
            let path = expand_home(path_str);
            if path.exists() {
                self.ensure_path_exists(&path);
            } else {
                debug!("Guarded path does not exist: {:?}", path);
            }
        }
    }

    /// Ensure all parent directories exist in the inode table
    fn ensure_path_exists(&self, path: &Path) -> Option<u64> {
        let mut current = PathBuf::from("/");
        let mut parent_ino = FUSE_ROOT_ID;

        for component in path.components().skip(1) {
            // Skip root
            current.push(component);

            // Check if already exists
            {
                let path_to_inode = self.path_to_inode.read().unwrap();
                if let Some(&ino) = path_to_inode.get(&current) {
                    parent_ino = ino;
                    continue;
                }
            }

            // Create new inode
            let ino = {
                let mut next = self.next_inode.write().unwrap();
                let ino = *next;
                *next += 1;
                ino
            };

            let is_dir = current.is_dir();
            let name = component.as_os_str().to_string_lossy().to_string();

            {
                let mut inodes = self.inodes.write().unwrap();
                let mut path_to_inode = self.path_to_inode.write().unwrap();

                inodes.insert(
                    ino,
                    InodeEntry {
                        real_path: current.clone(),
                        parent: parent_ino,
                        name,
                        is_dir,
                    },
                );
                path_to_inode.insert(current.clone(), ino);
            }

            parent_ino = ino;
        }

        Some(parent_ino)
    }

    /// Get inode for a path, creating if necessary
    fn get_or_create_inode(&self, path: &Path) -> Option<u64> {
        // Check existing
        {
            let path_to_inode = self.path_to_inode.read().unwrap();
            if let Some(&ino) = path_to_inode.get(path) {
                return Some(ino);
            }
        }

        // Create new
        self.ensure_path_exists(path)
    }

    /// Get the real path for an inode
    fn get_real_path(&self, ino: u64) -> Option<PathBuf> {
        let inodes = self.inodes.read().unwrap();
        inodes.get(&ino).map(|e| e.real_path.clone())
    }

    /// Get file attributes for a real path
    fn get_attr(&self, path: &Path) -> Option<FileAttr> {
        let metadata = std::fs::metadata(path).ok()?;

        let file_type = if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::Symlink
        } else {
            FileType::RegularFile
        };

        Some(FileAttr {
            ino: 0, // Will be filled in by caller
            size: metadata.len(),
            blocks: metadata.blocks(),
            atime: metadata.accessed().unwrap_or(UNIX_EPOCH),
            mtime: metadata.modified().unwrap_or(UNIX_EPOCH),
            ctime: UNIX_EPOCH, // Not easily available
            crtime: UNIX_EPOCH,
            kind: file_type,
            perm: metadata.mode() as u16,
            nlink: metadata.nlink() as u32,
            uid: metadata.uid(),
            gid: metadata.gid(),
            rdev: metadata.rdev() as u32,
            blksize: metadata.blksize() as u32,
            flags: 0,
        })
    }

    /// Get policy for a path
    fn get_policy(&self, path: &Path) -> GuardPolicy {
        let path_str = path.to_string_lossy();

        for (pattern, policy) in &self.policy_patterns {
            if pattern.matches(&path_str) {
                return policy.clone();
            }
        }

        // Default policy
        self.config.default_on_timeout.clone()
    }

    /// Check if access is approved (from cache)
    fn is_approved(&self, path: &Path, operation: &str) -> bool {
        let key = ApprovalKey {
            path: path.to_path_buf(),
            operation: operation.to_string(),
        };

        let approvals = self.approvals.read().unwrap();
        if let Some(expiry) = approvals.get(&key) {
            if *expiry > SystemTime::now() {
                return true;
            }
        }
        false
    }

    /// Cache an approval
    fn cache_approval(&self, path: &Path, operation: &str) {
        let key = ApprovalKey {
            path: path.to_path_buf(),
            operation: operation.to_string(),
        };

        // Cache for 8 hours (session duration)
        let expiry = SystemTime::now() + Duration::from_secs(8 * 60 * 60);

        let mut approvals = self.approvals.write().unwrap();
        approvals.insert(key, expiry);
    }

    /// Request approval from octo server (blocking)
    fn request_approval(&self, path: &Path, operation: &str) -> bool {
        self.runtime.block_on(async {
            let client = reqwest::Client::new();

            let body = serde_json::json!({
                "source": "octo_guard",
                "prompt_type": if operation == "read" { "file_read" } else { "file_write" },
                "resource": path.to_string_lossy(),
                "description": format!("{} access to {}", operation, path.display()),
                "timeout_secs": self.config.timeout_secs,
            });

            info!("Requesting approval for {} access to {:?}", operation, path);

            match client
                .post(format!("{}/internal/prompt", self.octo_server))
                .json(&body)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        if let Ok(result) = response.json::<serde_json::Value>().await {
                            if let Some(action) = result.get("action").and_then(|a| a.as_str()) {
                                let approved = action == "allow_once" || action == "allow_session";

                                if approved && action == "allow_session" {
                                    self.cache_approval(path, operation);
                                }

                                return approved;
                            }
                        }
                    }
                    false
                }
                Err(e) => {
                    error!("Failed to request approval: {}", e);
                    false
                }
            }
        })
    }

    /// Check access and request approval if needed
    fn check_access(&self, path: &Path, operation: &str) -> bool {
        let policy = self.get_policy(path);

        match policy {
            GuardPolicy::Auto => {
                info!("Auto-approved {} access to {:?}", operation, path);
                true
            }
            GuardPolicy::Deny => {
                warn!("Denied {} access to {:?} by policy", operation, path);
                false
            }
            GuardPolicy::Prompt => {
                // Check cache first
                if self.is_approved(path, operation) {
                    debug!("Cached approval for {} access to {:?}", operation, path);
                    return true;
                }

                // Request approval
                self.request_approval(path, operation)
            }
        }
    }
}

impl Filesystem for GuardedFs {
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let parent_path = match self.get_real_path(parent) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let path = parent_path.join(name);

        if !path.exists() {
            reply.error(libc::ENOENT);
            return;
        }

        let ino = match self.get_or_create_inode(&path) {
            Some(i) => i,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if let Some(mut attr) = self.get_attr(&path) {
            attr.ino = ino;
            reply.entry(&TTL, &attr, 0);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let path = match self.get_real_path(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if let Some(mut attr) = self.get_attr(&path) {
            attr.ino = ino;
            reply.attr(&TTL, &attr);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let path = match self.get_real_path(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check access
        if !self.check_access(&path, "read") {
            reply.error(libc::EACCES);
            return;
        }

        // Read the file
        match std::fs::read(&path) {
            Ok(data) => {
                let start = offset as usize;
                let end = std::cmp::min(start + size as usize, data.len());

                if start < data.len() {
                    reply.data(&data[start..end]);
                } else {
                    reply.data(&[]);
                }
            }
            Err(e) => {
                error!("Failed to read {:?}: {}", path, e);
                reply.error(libc::EIO);
            }
        }
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let path = match self.get_real_path(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if !path.is_dir() {
            reply.error(libc::ENOTDIR);
            return;
        }

        let entries = match std::fs::read_dir(&path) {
            Ok(e) => e,
            Err(_) => {
                reply.error(libc::EIO);
                return;
            }
        };

        let mut all_entries: Vec<_> = vec![
            (FUSE_ROOT_ID, FileType::Directory, ".".to_string()),
            (ino, FileType::Directory, "..".to_string()),
        ];

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = path.join(&name);

            let file_type = if entry_path.is_dir() {
                FileType::Directory
            } else {
                FileType::RegularFile
            };

            let entry_ino = self.get_or_create_inode(&entry_path).unwrap_or(0);
            all_entries.push((entry_ino, file_type, name));
        }

        for (i, (ino, file_type, name)) in all_entries.iter().enumerate().skip(offset as usize) {
            if reply.add(*ino, (i + 1) as i64, *file_type, name) {
                break;
            }
        }

        reply.ok();
    }

    fn open(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: fuser::ReplyOpen) {
        let path = match self.get_real_path(ino) {
            Some(p) => p,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check if write access is requested
        let operation = if flags & libc::O_WRONLY != 0 || flags & libc::O_RDWR != 0 {
            "write"
        } else {
            "read"
        };

        if !self.check_access(&path, operation) {
            reply.error(libc::EACCES);
            return;
        }

        // We use 0 as file handle since we don't track open files
        reply.opened(0, 0);
    }
}

/// Expand ~ to home directory
fn expand_home(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    // Load config
    let guard_config = if let Some(config_path) = &args.config {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config: {:?}", config_path))?;
        let sandbox: SandboxConfig = toml::from_str(&content)?;

        sandbox
            .profiles
            .get(&args.profile)
            .and_then(|p| p.guard.clone())
            .unwrap_or_default()
    } else {
        GuardConfig::default()
    };

    if !guard_config.enabled {
        info!("Guard is disabled in config, exiting");
        return Ok(());
    }

    // Determine mount point
    let mount_point = args.mount.unwrap_or_else(|| {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/octo-guard-{}", uid))
    });

    // Create mount point directory
    std::fs::create_dir_all(&mount_point)
        .with_context(|| format!("Failed to create mount point: {:?}", mount_point))?;

    info!("octo-guard starting");
    info!("  Mount: {:?}", mount_point);
    info!("  Profile: {}", args.profile);
    info!("  Guarded paths: {:?}", guard_config.paths);
    info!("  Timeout: {}s", guard_config.timeout_secs);

    // Create tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    // Create filesystem
    let fs = GuardedFs::new(guard_config, args.octo_server, runtime.handle().clone());

    // Build mount options
    let mut options = vec![
        MountOption::FSName("octo-guard".to_string()),
        MountOption::RO, // Read-only for now
        MountOption::NoAtime,
    ];

    if args.allow_other {
        options.push(MountOption::AllowOther);
    }

    // Mount and run
    info!("Mounting FUSE filesystem...");

    if args.foreground {
        fuser::mount2(fs, &mount_point, &options)?;
    } else {
        // Daemonize
        match unsafe { libc::fork() } {
            -1 => anyhow::bail!("Fork failed"),
            0 => {
                // Child process
                unsafe { libc::setsid() };
                fuser::mount2(fs, &mount_point, &options)?;
            }
            pid => {
                info!("Daemon started with PID {}", pid);
                return Ok(());
            }
        }
    }

    Ok(())
}
