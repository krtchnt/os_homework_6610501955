use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::time::Instant;

const DEFAULT_SIZES_MB: &[usize] = &[64, 96, 128];
const PIPE_READ: usize = 0;
const PIPE_WRITE: usize = 1;
const _SC_PAGESIZE: i32 = 30;

unsafe extern "C" {
    fn fork() -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn pipe(fds: *mut i32) -> i32;
    fn close(fd: i32) -> i32;
    fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn sysconf(name: i32) -> isize;
    fn _exit(status: i32) -> !;
}

#[derive(Debug)]
struct Config {
    sizes_mb: Vec<usize>,
    output: Option<PathBuf>,
}

#[derive(Debug)]
struct ChildStage {
    stage: String,
    rss_kb: u64,
    private_dirty_kb: u64,
    touch_ms: f64,
}

#[derive(Debug)]
struct ExperimentResult {
    size_mb: usize,
    parent_rss_kb: u64,
    child_post_fork: ChildStage,
    child_post_write: ChildStage,
}

fn parse_args() -> Result<Config, String> {
    let mut sizes: Option<Vec<usize>> = None;
    let mut output: Option<PathBuf> = None;

    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--sizes" => {
                let value = it
                    .next()
                    .ok_or_else(|| "--sizes requires a value".to_string())?;
                let mut parsed = Vec::new();
                for chunk in value.split(',') {
                    if chunk.trim().is_empty() {
                        continue;
                    }
                    let mb: usize = chunk
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid size: {}", chunk))?;
                    if mb < 16 {
                        return Err("each size must be at least 16 MB".into());
                    }
                    parsed.push(mb);
                }
                if parsed.is_empty() {
                    return Err("no valid sizes provided".into());
                }
                sizes = Some(parsed);
            }
            "--output" => {
                let value = it
                    .next()
                    .ok_or_else(|| "--output requires a path".to_string())?;
                output = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {}", other)),
        }
    }

    Ok(Config {
        sizes_mb: sizes.unwrap_or_else(|| DEFAULT_SIZES_MB.to_vec()),
        output,
    })
}

fn print_usage() {
    eprintln!("Usage: cow [--sizes 64,96,128] [--output path]");
    eprintln!("Demonstrates copy-on-write behaviour via RSS measurements.");
}

fn read_rss_kb(pid: u32) -> io::Result<u64> {
    let path = format!("/proc/{pid}/status");
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let value: Vec<&str> = rest.trim().split_whitespace().collect();
            if let Some(number) = value.first() {
                return number
                    .parse::<u64>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "VmRSS not found in /proc status",
    ))
}

fn read_private_dirty_kb(pid: u32) -> io::Result<u64> {
    let path = format!("/proc/{pid}/smaps_rollup");
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if let Some(rest) = line.strip_prefix("Private_Dirty:") {
            let value: Vec<&str> = rest.trim().split_whitespace().collect();
            if let Some(number) = value.first() {
                return number
                    .parse::<u64>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Private_Dirty not found in smaps_rollup",
    ))
}

fn page_size() -> usize {
    unsafe {
        let sz = sysconf(_SC_PAGESIZE);
        if sz > 0 {
            sz as usize
        } else {
            4096
        }
    }
}

fn touch_pages(data: &mut [u8], page: usize) {
    if page == 0 {
        return;
    }
    for chunk in data.chunks_mut(page) {
        if let Some(first) = chunk.first_mut() {
            *first = first.wrapping_add(1);
        }
    }
}

fn write_all(fd: RawFd, payload: &[u8]) -> io::Result<()> {
    let mut total = 0;
    while total < payload.len() {
        let written = unsafe { write(fd, payload[total..].as_ptr(), payload.len() - total) };
        if written < 0 {
            return Err(io::Error::last_os_error());
        }
        total += written as usize;
    }
    Ok(())
}

fn read_to_end(fd: RawFd) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    loop {
        let read_bytes = unsafe { read(fd, temp.as_mut_ptr(), temp.len()) };
        if read_bytes < 0 {
            return Err(io::Error::last_os_error());
        }
        if read_bytes == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read_bytes as usize]);
    }
    Ok(buffer)
}

fn wait_child(pid: i32) -> io::Result<i32> {
    let mut status = 0;
    loop {
        let result = unsafe { waitpid(pid, &mut status, 0) };
        if result < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        break;
    }
    Ok(status)
}

fn parse_child_report(data: &[u8]) -> Result<(ChildStage, ChildStage), String> {
    let text = String::from_utf8_lossy(data);
    let mut stages = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut stage = ChildStage {
            stage: String::new(),
            rss_kb: 0,
            private_dirty_kb: 0,
            touch_ms: 0.0,
        };
        let mut parts = line.split(',');
        stage.stage = parts
            .next()
            .ok_or_else(|| "missing stage label".to_string())?
            .trim()
            .to_string();
        for entry in parts {
            let (key, value) = entry
                .trim()
                .split_once('=')
                .ok_or_else(|| format!("invalid entry: {}", entry))?;
            match key.trim() {
                "rss_kb" => {
                    stage.rss_kb = value
                        .trim()
                        .parse()
                        .map_err(|e| format!("bad rss_kb value: {e}"))?
                }
                "private_dirty_kb" => {
                    stage.private_dirty_kb = value
                        .trim()
                        .parse()
                        .map_err(|e| format!("bad private_dirty_kb value: {e}"))?
                }
                "touch_ms" => {
                    stage.touch_ms = value
                        .trim()
                        .parse()
                        .map_err(|e| format!("bad touch_ms value: {e}"))?
                }
                other => return Err(format!("unknown key {other} in child report")),
            }
        }
        stages.push(stage);
    }
    if stages.len() != 2 {
        return Err("expected exactly two stages from child".into());
    }
    Ok((stages.remove(0), stages.remove(0)))
}

fn child_routine(data: &mut [u8], pipe_write: RawFd, page: usize) -> ! {
    let pid = std::process::id();
    let rss_post_fork = read_rss_kb(pid).unwrap_or_default();
    let private_dirty_post_fork = read_private_dirty_kb(pid).unwrap_or_default();

    let start = Instant::now();
    touch_pages(data, page);
    let touch_ms = start.elapsed().as_secs_f64() * 1000.0;

    let rss_post_write = read_rss_kb(pid).unwrap_or_default();
    let private_dirty_post_write = read_private_dirty_kb(pid).unwrap_or_default();

    let report = format!(
        "post_fork,rss_kb={rss_post_fork},private_dirty_kb={private_dirty_post_fork},touch_ms=0.0\n\
post_write,rss_kb={rss_post_write},private_dirty_kb={private_dirty_post_write},touch_ms={touch_ms:.4}\n"
    );

    if let Err(err) = write_all(pipe_write, report.as_bytes()) {
        eprintln!("child failed to write report: {err}");
    }

    unsafe {
        close(pipe_write);
        _exit(0);
    }
}

fn run_experiment(size_mb: usize) -> Result<ExperimentResult, String> {
    let size_bytes = size_mb * 1024 * 1024;
    println!("== Running Copy-on-Write demo for {size_mb} MB ==");

    let mut data = vec![0u8; size_bytes];
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = (i & 0xFF) as u8;
    }

    let parent_pid = std::process::id();
    let parent_rss =
        read_rss_kb(parent_pid).map_err(|e| format!("failed to read parent RSS: {e}"))?;
    let parent_private_dirty = read_private_dirty_kb(parent_pid).unwrap_or(0);

    println!(
        "Parent RSS before fork: {} kB (Private_Dirty {} kB)",
        parent_rss, parent_private_dirty
    );

    let page = page_size();
    let mut pipe_fds = [0i32; 2];
    if unsafe { pipe(pipe_fds.as_mut_ptr()) } != 0 {
        return Err(format!("pipe failed: {}", io::Error::last_os_error()));
    }

    let pid = unsafe { fork() };
    if pid < 0 {
        return Err(format!("fork failed: {}", io::Error::last_os_error()));
    }

    if pid == 0 {
        unsafe {
            close(pipe_fds[PIPE_READ]);
        }
        child_routine(&mut data, pipe_fds[PIPE_WRITE], page);
    }

    unsafe {
        close(pipe_fds[PIPE_WRITE]);
    }
    let payload = read_to_end(pipe_fds[PIPE_READ])
        .map_err(|e| format!("failed to read child report: {e}"))?;
    unsafe {
        close(pipe_fds[PIPE_READ]);
    }

    wait_child(pid).map_err(|e| format!("waitpid failed: {e}"))?;

    let (post_fork, post_write) = parse_child_report(&payload)?;
    println!(
        "Child after fork: RSS {} kB, Private_Dirty {} kB",
        post_fork.rss_kb, post_fork.private_dirty_kb
    );
    println!(
        "Child after touching pages: RSS {} kB, Private_Dirty {} kB (touch {:.3} ms)",
        post_write.rss_kb, post_write.private_dirty_kb, post_write.touch_ms
    );

    Ok(ExperimentResult {
        size_mb,
        parent_rss_kb: parent_rss,
        child_post_fork: post_fork,
        child_post_write: post_write,
    })
}

fn write_csv(path: &PathBuf, results: &[ExperimentResult]) -> io::Result<()> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "size_mb,parent_rss_kb,child_post_fork_rss_kb,child_post_fork_private_dirty_kb,\
child_post_write_rss_kb,child_post_write_private_dirty_kb,touch_ms"
    )?;
    for entry in results {
        writeln!(
            file,
            "{},{},{},{},{},{},{}",
            entry.size_mb,
            entry.parent_rss_kb,
            entry.child_post_fork.rss_kb,
            entry.child_post_fork.private_dirty_kb,
            entry.child_post_write.rss_kb,
            entry.child_post_write.private_dirty_kb,
            entry.child_post_write.touch_ms
        )?;
    }
    Ok(())
}

fn main() {
    let config = match parse_args() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Argument error: {err}");
            print_usage();
            std::process::exit(1);
        }
    };

    let mut results = Vec::new();
    for size in &config.sizes_mb {
        match run_experiment(*size) {
            Ok(res) => results.push(res),
            Err(err) => {
                eprintln!("Experiment failed for size {size} MB: {err}");
            }
        }
    }

    if let Some(path) = &config.output {
        if let Err(err) = write_csv(path, &results) {
            eprintln!("Failed to write CSV: {err}");
        } else {
            println!("Saved CSV results to {:?}", path);
        }
    }
}
