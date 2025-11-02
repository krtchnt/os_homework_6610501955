use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
enum Mode {
    Avoidance,
    Detection,
    Resolution,
}

#[derive(Clone, Debug)]
struct ProcessPlan {
    id: usize,
    name: &'static str,
    steps: Vec<Vec<u32>>,
}

struct ResourceManager {
    inner: Arc<ResourceInner>,
}

struct ResourceInner {
    state: Mutex<ResourceState>,
    cond: Condvar,
}

struct ResourceState {
    total: Vec<u32>,
    available: Vec<u32>,
    allocations: HashMap<usize, Vec<u32>>,
    waiting: HashMap<usize, Vec<u32>>,
    processes: HashSet<usize>,
    finished: HashSet<usize>,
    terminated: HashSet<usize>,
    stop_all: bool,
}

enum RequestResult {
    Granted,
    Terminated,
    Stopped,
}

impl ResourceManager {
    fn new(total: Vec<u32>) -> Self {
        ResourceManager {
            inner: Arc::new(ResourceInner {
                state: Mutex::new(ResourceState {
                    available: total.clone(),
                    total,
                    allocations: HashMap::new(),
                    waiting: HashMap::new(),
                    processes: HashSet::new(),
                    finished: HashSet::new(),
                    terminated: HashSet::new(),
                    stop_all: false,
                }),
                cond: Condvar::new(),
            }),
        }
    }

    fn register_process(&self, pid: usize) {
        let mut state = self.inner.state.lock().unwrap();
        if !state.allocations.contains_key(&pid) {
            let resource_count = state.total.len();
            state.allocations.insert(pid, vec![0; resource_count]);
            state.processes.insert(pid);
        }
    }

    fn request(&self, pid: usize, request: &[u32]) -> RequestResult {
        let mut state = self.inner.state.lock().unwrap();
        let request_vec = request.to_vec();
        if request_vec.len() != state.total.len() {
            panic!("request vector length does not match resources");
        }
        loop {
            if state.terminated.contains(&pid) {
                state.waiting.remove(&pid);
                return RequestResult::Terminated;
            }
            if state.stop_all {
                state.waiting.remove(&pid);
                return RequestResult::Stopped;
            }
            if self.can_grant(&state, &request_vec) {
                self.allocate(&mut state, pid, &request_vec);
                state.waiting.remove(&pid);
                return RequestResult::Granted;
            }
            state.waiting.insert(pid, request_vec.clone());
            state = self.inner.cond.wait(state).unwrap();
        }
    }

    fn release_all(&self, pid: usize, mark_finished: bool) {
        let mut state = self.inner.state.lock().unwrap();
        if let Some(release) = {
            state.allocations.get_mut(&pid).map(|alloc| {
                let snapshot = alloc.clone();
                alloc.fill(0);
                snapshot
            })
        } {
            for (idx, amount) in release.iter().enumerate() {
                state.available[idx] += *amount;
            }
        }
        state.waiting.remove(&pid);
        if mark_finished {
            state.finished.insert(pid);
        }
        self.inner.cond.notify_all();
    }

    fn terminate(&self, pid: usize) {
        let mut state = self.inner.state.lock().unwrap();
        if let Some(release) = {
            state.allocations.get_mut(&pid).map(|alloc| {
                let snapshot = alloc.clone();
                alloc.fill(0);
                snapshot
            })
        } {
            for (idx, amount) in release.iter().enumerate() {
                state.available[idx] += *amount;
            }
        }
        state.waiting.remove(&pid);
        state.terminated.insert(pid);
        self.inner.cond.notify_all();
    }

    fn stop_all(&self) {
        let mut state = self.inner.state.lock().unwrap();
        state.stop_all = true;
        self.inner.cond.notify_all();
    }

    fn detect_deadlock(&self) -> Option<Vec<usize>> {
        let state = self.inner.state.lock().unwrap();
        if state.waiting.is_empty() {
            return None;
        }
        let graph = self.build_wait_for_graph(&state);
        find_cycle(&graph)
    }

    fn all_done(&self) -> bool {
        let state = self.inner.state.lock().unwrap();
        state.finished.len() + state.terminated.len() == state.processes.len()
    }

    fn can_grant(&self, state: &ResourceState, request: &[u32]) -> bool {
        request
            .iter()
            .enumerate()
            .all(|(idx, amount)| *amount <= state.available[idx])
    }

    fn allocate(&self, state: &mut ResourceState, pid: usize, request: &[u32]) {
        let alloc = state
            .allocations
            .get_mut(&pid)
            .expect("process not registered");
        for (idx, amount) in request.iter().enumerate() {
            state.available[idx] -= *amount;
            alloc[idx] += *amount;
        }
    }

    fn build_wait_for_graph(&self, state: &ResourceState) -> HashMap<usize, Vec<usize>> {
        let mut graph: HashMap<usize, Vec<usize>> = HashMap::new();
        for (&waiting_pid, req) in &state.waiting {
            let mut dependents = Vec::new();
            for (res_idx, amount) in req.iter().enumerate() {
                if *amount == 0 {
                    continue;
                }
                if state.available[res_idx] >= *amount {
                    continue;
                }
                for (&holder_pid, allocation) in &state.allocations {
                    if holder_pid == waiting_pid {
                        continue;
                    }
                    if allocation[res_idx] > 0 {
                        dependents.push(holder_pid);
                    }
                }
            }
            graph.insert(waiting_pid, dependents);
        }
        graph
    }
}

impl Clone for ResourceManager {
    fn clone(&self) -> Self {
        ResourceManager {
            inner: Arc::clone(&self.inner),
        }
    }
}

fn find_cycle(graph: &HashMap<usize, Vec<usize>>) -> Option<Vec<usize>> {
    #[derive(PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    fn dfs(
        node: usize,
        graph: &HashMap<usize, Vec<usize>>,
        colors: &mut HashMap<usize, Color>,
        stack: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        colors.insert(node, Color::Gray);
        stack.push(node);
        if let Some(neighbours) = graph.get(&node) {
            for &next in neighbours {
                match colors.get(&next) {
                    Some(Color::Gray) => {
                        let mut cycle = Vec::new();
                        for &item in stack.iter().rev() {
                            cycle.push(item);
                            if item == next {
                                break;
                            }
                        }
                        cycle.reverse();
                        return Some(cycle);
                    }
                    Some(Color::Black) => {}
                    _ => {
                        if let Some(found) = dfs(next, graph, colors, stack) {
                            return Some(found);
                        }
                    }
                }
            }
        }
        stack.pop();
        colors.insert(node, Color::Black);
        None
    }

    let mut colors: HashMap<usize, Color> = HashMap::new();
    for &node in graph.keys() {
        colors.entry(node).or_insert(Color::White);
    }

    for &node in graph.keys() {
        if matches!(colors.get(&node), Some(Color::White) | None) {
            let mut stack = Vec::new();
            if let Some(cycle) = dfs(node, graph, &mut colors, &mut stack) {
                return Some(cycle);
            }
        }
    }
    None
}

fn parse_mode() -> Result<Mode, String> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--mode requires a value".to_string())?;
                return match value.to_lowercase().as_str() {
                    "avoidance" => Ok(Mode::Avoidance),
                    "detection" => Ok(Mode::Detection),
                    "resolution" => Ok(Mode::Resolution),
                    other => Err(format!("unknown mode: {}", other)),
                };
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {}", other)),
        }
    }
    Ok(Mode::Detection)
}

fn print_usage() {
    eprintln!("Usage: deadlock [--mode avoidance|detection|resolution]");
    eprintln!("  avoidance   - Banker's algorithm safe-state demo");
    eprintln!("  detection   - Spawn threads that deadlock and detect it");
    eprintln!("  resolution  - Detect deadlock and resolve by terminating a victim");
}

fn run_avoidance_demo() {
    println!("== Deadlock Avoidance via Banker's Algorithm ==");
    let total = vec![10, 5, 7];
    let allocation = vec![
        vec![0, 1, 0],
        vec![2, 0, 0],
        vec![3, 0, 2],
        vec![2, 1, 1],
        vec![0, 0, 2],
    ];
    let maximum = vec![
        vec![7, 5, 3],
        vec![3, 2, 2],
        vec![9, 0, 2],
        vec![2, 2, 2],
        vec![4, 3, 3],
    ];

    let safe_sequence = bankers_safe_sequence(&total, &allocation, &maximum)
        .expect("system should be in a safe state");
    println!("Safe sequence: {:?}", safe_sequence);

    let request = vec![1, 0, 2];
    let process = 1;
    let can_grant = bankers_request_is_safe(&total, &allocation, &maximum, process, &request);
    println!(
        "Request from P{} for {:?} is {} under Banker's algorithm",
        process,
        request,
        if can_grant { "ACCEPTED" } else { "REJECTED" }
    );

    let unsafe_request = vec![3, 3, 0];
    let unsafe_process = 0;
    let can_grant_unsafe = bankers_request_is_safe(
        &total,
        &allocation,
        &maximum,
        unsafe_process,
        &unsafe_request,
    );
    println!(
        "Request from P{} for {:?} is {} (would lead to unsafe state)",
        unsafe_process,
        unsafe_request,
        if can_grant_unsafe {
            "ACCEPTED"
        } else {
            "REJECTED"
        }
    );
}

fn bankers_safe_sequence(
    total: &[u32],
    allocation: &[Vec<u32>],
    maximum: &[Vec<u32>],
) -> Option<Vec<usize>> {
    let processes = allocation.len();
    let mut work = total.to_vec();
    for alloc in allocation {
        for (idx, amount) in alloc.iter().enumerate() {
            work[idx] = work[idx].saturating_sub(*amount);
        }
    }

    let mut need = Vec::new();
    for (max_row, alloc_row) in maximum.iter().zip(allocation.iter()) {
        let mut row = Vec::new();
        for (max, alloc) in max_row.iter().zip(alloc_row.iter()) {
            row.push(max.saturating_sub(*alloc));
        }
        need.push(row);
    }

    let mut finish = vec![false; processes];
    let mut sequence = Vec::new();
    loop {
        let mut progressed = false;
        for pid in 0..processes {
            if finish[pid] {
                continue;
            }
            if need[pid]
                .iter()
                .enumerate()
                .all(|(idx, amount)| *amount <= work[idx])
            {
                for (idx, amount) in allocation[pid].iter().enumerate() {
                    work[idx] += *amount;
                }
                finish[pid] = true;
                sequence.push(pid);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    if finish.iter().all(|done| *done) {
        Some(sequence)
    } else {
        None
    }
}

fn bankers_request_is_safe(
    total: &[u32],
    allocation: &[Vec<u32>],
    maximum: &[Vec<u32>],
    pid: usize,
    request: &[u32],
) -> bool {
    if pid >= allocation.len() || request.len() != total.len() {
        return false;
    }
    let mut new_allocation = allocation.to_vec();
    let new_maximum = maximum.to_vec();

    for idx in 0..request.len() {
        new_allocation[pid][idx] += request[idx];
        if new_allocation[pid][idx] > new_maximum[pid][idx] {
            return false;
        }
    }

    bankers_safe_sequence(total, &new_allocation, &new_maximum).is_some()
}

fn run_runtime_demo(mode: Mode) {
    let resolve = matches!(mode, Mode::Resolution);
    println!(
        "== Deadlock {} Demo ==",
        if resolve { "Resolution" } else { "Detection" }
    );
    let manager = ResourceManager::new(vec![1, 1, 1]);
    let plans = vec![
        ProcessPlan {
            id: 0,
            name: "P0",
            steps: vec![vec![1, 0, 0], vec![0, 1, 0]],
        },
        ProcessPlan {
            id: 1,
            name: "P1",
            steps: vec![vec![0, 1, 0], vec![0, 0, 1]],
        },
        ProcessPlan {
            id: 2,
            name: "P2",
            steps: vec![vec![0, 0, 1], vec![1, 0, 0]],
        },
    ];

    for plan in &plans {
        manager.register_process(plan.id);
    }

    let mut handles = Vec::new();
    for plan in plans.clone() {
        let mgr = manager.clone();
        let handle = thread::spawn(move || run_process(plan, mgr));
        handles.push(handle);
    }

    let monitor_manager = manager.clone();
    let monitor = thread::spawn(move || monitor_deadlock(monitor_manager, resolve));

    for handle in handles {
        handle.join().expect("process thread panicked");
    }

    monitor.join().expect("monitor thread panicked");

    println!("Simulation complete.");
}

fn run_process(plan: ProcessPlan, manager: ResourceManager) {
    for (idx, request) in plan.steps.iter().enumerate() {
        println!("{} requesting step {}: {:?}", plan.name, idx + 1, request);
        let start = Instant::now();
        match manager.request(plan.id, request) {
            RequestResult::Granted => {
                println!(
                    "{} granted step {} after {:?}",
                    plan.name,
                    idx + 1,
                    start.elapsed()
                );
            }
            RequestResult::Terminated => {
                println!("{} terminated during wait.", plan.name);
                return;
            }
            RequestResult::Stopped => {
                println!("{} aborted due to system stop.", plan.name);
                manager.terminate(plan.id);
                return;
            }
        }

        if idx + 1 < plan.steps.len() {
            thread::sleep(Duration::from_millis(150));
        }
    }

    println!("{} completed work; releasing resources.", plan.name);
    manager.release_all(plan.id, true);
}

fn monitor_deadlock(manager: ResourceManager, resolve: bool) {
    let mut resolution_triggered = false;
    loop {
        thread::sleep(Duration::from_millis(200));
        if let Some(cycle) = manager.detect_deadlock() {
            println!("Deadlock detected among processes: {:?}", cycle);
            if resolve && !resolution_triggered {
                if let Some(&victim) = cycle.iter().max() {
                    println!("Resolving deadlock by terminating process {}", victim);
                    manager.terminate(victim);
                    resolution_triggered = true;
                }
            } else {
                println!("Halting processes to illustrate deadlock state.");
                manager.stop_all();
                break;
            }
        }

        if manager.all_done() {
            break;
        }
    }
}

fn main() {
    let mode = match parse_mode() {
        Ok(mode) => mode,
        Err(err) => {
            eprintln!("Argument error: {}", err);
            print_usage();
            std::process::exit(1);
        }
    };

    match mode {
        Mode::Avoidance => run_avoidance_demo(),
        Mode::Detection | Mode::Resolution => run_runtime_demo(mode),
    }
}
