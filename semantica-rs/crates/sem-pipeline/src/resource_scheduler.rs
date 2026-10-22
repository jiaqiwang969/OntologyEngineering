//! Resource scheduling (`semantica/pipeline/resource_scheduler.py`): CPU, memory, disk and
//! GPU capacities, allocations and their release.
//!
//! The original reads capacities with `psutil`; this port reads the same quantities with
//! the standard library: `/proc/cpuinfo` and `/proc/meminfo` on Linux, `sysctl` and
//! `vm_stat` on macOS, `df -Pk /` for the disk (see `PORT-NOTES.md`).

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, ThreadId};

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{float_repr, py_str, str_repr};
use sem_utils::{ProgressTracker, SemanticaError, ValidationError, get_logger};
use serde_json::{Map, Value};

use crate::kwargs::{Kwargs, init_config, json_map_repr, opt_str_repr};
use crate::pipeline_builder::Pipeline;
use crate::{enabled_progress_tracker, time_now};

py_enum! {
    /// Resource types.
    pub enum ResourceType {
        /// `cpu`.
        Cpu = ("CPU", "cpu"),
        /// `gpu`.
        Gpu = ("GPU", "gpu"),
        /// `memory`.
        Memory = ("MEMORY", "memory"),
        /// `disk`.
        Disk = ("DISK", "disk"),
        /// `network`.
        Network = ("NETWORK", "network"),
    }
}

/// Resource definition (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct Resource {
    /// Resource identifier (`cpu`, `memory`, `disk`, `gpu_<device>`).
    pub resource_id: String,
    /// Resource type.
    pub resource_type: ResourceType,
    /// Total capacity (cores, GB, or devices).
    pub capacity: f64,
    /// Amount currently allocated.
    pub allocated: f64,
    /// Extra information (`logical_cores`, `available_gb`, `total_gb`, `device_id`).
    pub metadata: Map<String, Value>,
}

impl fmt::Debug for Resource {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Resource(resource_id={}, resource_type={:?}, capacity={}, allocated={}, metadata={})",
            str_repr(&self.resource_id),
            self.resource_type,
            float_repr(self.capacity),
            float_repr(self.allocated),
            json_map_repr(&self.metadata)
        )
    }
}

impl Resource {
    /// `Resource(resource_id, resource_type, capacity)` with the dataclass defaults.
    pub fn new(resource_id: impl Into<String>, resource_type: ResourceType, capacity: f64) -> Self {
        Resource {
            resource_id: resource_id.into(),
            resource_type,
            capacity,
            allocated: 0.0,
            metadata: Map::new(),
        }
    }
}

/// Resource allocation record (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct ResourceAllocation {
    /// Allocation identifier: `<kind>_<pipeline>_<step or 'default'>_<time.time()>`.
    pub allocation_id: String,
    /// The allocated resource.
    pub resource_id: String,
    /// Its type.
    pub resource_type: ResourceType,
    /// Amount allocated.
    pub amount: f64,
    /// The pipeline it is allocated to.
    pub pipeline_id: String,
    /// The step it is allocated to.
    pub step_name: Option<String>,
    /// Extra information (`device_id` for GPUs).
    pub metadata: Map<String, Value>,
}

impl fmt::Debug for ResourceAllocation {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ResourceAllocation(allocation_id={}, resource_id={}, resource_type={:?}, amount={}, \
             pipeline_id={}, step_name={}, metadata={})",
            str_repr(&self.allocation_id),
            str_repr(&self.resource_id),
            self.resource_type,
            float_repr(self.amount),
            str_repr(&self.pipeline_id),
            opt_str_repr(self.step_name.as_deref()),
            json_map_repr(&self.metadata)
        )
    }
}

impl ResourceAllocation {
    /// `ResourceAllocation(allocation_id, resource_id, resource_type, amount, pipeline_id)`
    /// with the dataclass defaults.
    pub fn new(
        allocation_id: impl Into<String>,
        resource_id: impl Into<String>,
        resource_type: ResourceType,
        amount: f64,
        pipeline_id: impl Into<String>,
    ) -> Self {
        ResourceAllocation {
            allocation_id: allocation_id.into(),
            resource_id: resource_id.into(),
            resource_type,
            amount,
            pipeline_id: pipeline_id.into(),
            step_name: None,
            metadata: Map::new(),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
struct RLockState {
    owner: Option<ThreadId>,
    count: usize,
}

/// A re-entrant lock (`threading.RLock`): the thread holding it may acquire it again; other
/// threads wait until every acquisition has been released.
#[derive(Default)]
pub struct RLock {
    state: Mutex<RLockState>,
    released: Condvar,
}

impl RLock {
    /// An unlocked lock.
    pub fn new() -> Self {
        RLock::default()
    }

    /// `with lock:` — block until this thread holds the lock; the guard releases one level.
    pub fn acquire(&self) -> RLockGuard<'_> {
        let me = thread::current().id();
        let mut state = lock(&self.state);
        loop {
            match state.owner {
                None => {
                    state.owner = Some(me);
                    state.count = 1;
                    break;
                }
                Some(owner) if owner == me => {
                    state.count += 1;
                    break;
                }
                Some(_) => {
                    state = self.released.wait(state).unwrap_or_else(|e| e.into_inner());
                }
            }
        }
        RLockGuard {
            lock: self,
            _not_send: PhantomData,
        }
    }

    /// How many times the owning thread currently holds the lock (0 = unlocked).
    pub fn count(&self) -> usize {
        lock(&self.state).count
    }

    fn release(&self) {
        let mut state = lock(&self.state);
        state.count = state.count.saturating_sub(1);
        if state.count == 0 {
            state.owner = None;
            self.released.notify_one();
        }
    }
}

impl fmt::Debug for RLock {
    /// Like `repr(threading.RLock())`: `<unlocked _thread.RLock object count=0>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.count();
        write!(
            f,
            "<{} _thread.RLock object count={count}>",
            if count == 0 { "unlocked" } else { "locked" }
        )
    }
}

/// One acquisition of an [`RLock`], released on drop (on the acquiring thread).
pub struct RLockGuard<'a> {
    lock: &'a RLock,
    _not_send: PhantomData<Cell<*const ()>>,
}

impl Drop for RLockGuard<'_> {
    fn drop(&mut self) {
        self.lock.release();
    }
}

/// CPU and memory as `psutil` reports them.
#[derive(Debug, Clone, Copy)]
struct CpuMemory {
    /// `psutil.cpu_count(logical=False)` (`None` when unknown).
    physical_cores: Option<usize>,
    /// `psutil.cpu_count(logical=True)`.
    logical_cores: Option<usize>,
    /// `virtual_memory().total`, bytes.
    total: f64,
    /// `virtual_memory().available`, bytes.
    available: f64,
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// CPU counts and total memory never change while the process runs.
static STATIC_CPU_MEMORY: Lazy<Option<(Option<usize>, Option<usize>, f64)>> =
    Lazy::new(probe_static_cpu_memory);

fn probe_static_cpu_memory() -> Option<(Option<usize>, Option<usize>, f64)> {
    if cfg!(target_os = "macos") {
        let text = run(
            "/usr/sbin/sysctl",
            &["-n", "hw.physicalcpu", "hw.logicalcpu", "hw.memsize"],
        )?;
        let mut lines = text.lines().map(str::trim);
        let physical = lines.next()?.parse::<usize>().ok();
        let logical = lines.next()?.parse::<usize>().ok();
        let total = lines.next()?.parse::<f64>().ok()?;
        Some((physical, logical, total))
    } else if cfg!(target_os = "linux") {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok();
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        let total = meminfo_kb(&meminfo, "MemTotal:")? * 1024.0;
        let (physical, logical) = match &cpuinfo {
            Some(text) => linux_cpu_counts(text),
            None => (None, None),
        };
        let logical = logical.or_else(|| thread::available_parallelism().ok().map(|n| n.get()));
        Some((physical, logical, total))
    } else {
        None
    }
}

fn meminfo_kb(text: &str, key: &str) -> Option<f64> {
    text.lines()
        .find(|line| line.starts_with(key))
        .and_then(|line| line[key.len()..].split_whitespace().next())
        .and_then(|n| n.parse::<f64>().ok())
}

/// Physical cores (distinct `physical id`/`core id` pairs) and logical processors.
fn linux_cpu_counts(cpuinfo: &str) -> (Option<usize>, Option<usize>) {
    let mut cores = std::collections::HashSet::new();
    let mut processors = 0;
    let mut physical_id: Option<String> = None;
    for line in cpuinfo.lines() {
        let Some((key, value)) = line.split_once(':') else {
            physical_id = None;
            continue;
        };
        match key.trim() {
            "processor" => processors += 1,
            "physical id" => physical_id = Some(value.trim().to_string()),
            "core id" => {
                cores.insert((physical_id.clone(), value.trim().to_string()));
            }
            _ => {}
        }
    }
    (
        (!cores.is_empty()).then_some(cores.len()),
        (processors > 0).then_some(processors),
    )
}

/// `virtual_memory().available`, bytes: free + speculative + inactive pages on macOS (as
/// psutil computes it), `MemAvailable` on Linux.
fn probe_available_memory() -> Option<f64> {
    if cfg!(target_os = "macos") {
        let text = run("/usr/bin/vm_stat", &[])?;
        let page_size = text
            .lines()
            .next()
            .and_then(|line| line.split("page size of ").nth(1))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|n| n.parse::<f64>().ok())?;
        let pages = |key: &str| -> Option<f64> {
            text.lines()
                .find(|line| line.starts_with(key))
                .and_then(|line| line[key.len()..].trim().trim_end_matches('.').parse().ok())
        };
        let free = pages("Pages free:")?;
        let inactive = pages("Pages inactive:")?;
        let speculative = pages("Pages speculative:").unwrap_or(0.0);
        Some((free + speculative + inactive) * page_size)
    } else if cfg!(target_os = "linux") {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        meminfo_kb(&meminfo, "MemAvailable:").map(|kb| kb * 1024.0)
    } else {
        None
    }
}

fn probe_cpu_memory() -> Option<CpuMemory> {
    let (physical_cores, logical_cores, total) = (*STATIC_CPU_MEMORY)?;
    let available = probe_available_memory()?;
    Some(CpuMemory {
        physical_cores,
        logical_cores,
        total,
        available,
    })
}

/// `psutil.disk_usage("/")`: (free, total) bytes from `df -Pk /`.
fn probe_disk() -> Option<(f64, f64)> {
    let text = ["/bin/df", "/usr/bin/df"]
        .iter()
        .find_map(|program| run(program, &["-Pk", "/"]))?;
    let fields: Vec<&str> = text.lines().nth(1)?.split_whitespace().collect();
    let total_kb: f64 = fields.get(1)?.parse().ok()?;
    let available_kb: f64 = fields.get(3)?.parse().ok()?;
    Some((available_kb * 1024.0, total_kb * 1024.0))
}

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// A numeric keyword option (`options.get(key, default)`) compared against a float; anything
/// else fails as Python's comparison would.
fn numeric_option(options: &Kwargs, key: &str, default: f64) -> Result<f64, SemanticaError> {
    match options.get(key) {
        None => Ok(default),
        Some(value) => value.as_f64().ok_or_else(|| {
            SemanticaError::Type(format!(
                "'>=' not supported between instances of 'float' and '{}'",
                value.type_name()
            ))
        }),
    }
}

/// `str()` of an amount the way the original's messages print it.
fn amount_text(x: f64) -> String {
    float_repr(x)
}

/// Resource scheduling and allocation system.
///
/// • Resource allocation and management
/// • Capacity tracking per resource
/// • Thread-safe allocation under a re-entrant lock
pub struct ResourceScheduler {
    /// Configuration (`config` merged with the keyword arguments).
    pub config: Kwargs,
    /// The shared progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    /// Resources by id (`cpu`, `memory`, `disk`, then `gpu_<device>` as allocated).
    pub resources: Mutex<IndexMap<String, Resource>>,
    /// Live allocations by id.
    pub allocations: Mutex<IndexMap<String, ResourceAllocation>>,
    /// The re-entrant scheduler lock (`threading.RLock`), held around allocation and release.
    pub lock: RLock,
    logger: Logger,
}

impl fmt::Debug for ResourceScheduler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let resources = lock(&self.resources).clone();
        let allocations = lock(&self.allocations).clone();
        f.debug_struct("ResourceScheduler")
            .field("config", &self.config)
            .field("resources", &resources)
            .field("allocations", &allocations)
            .field("lock", &self.lock)
            .finish_non_exhaustive()
    }
}

impl Default for ResourceScheduler {
    fn default() -> Self {
        ResourceScheduler::new(None, Kwargs::new())
    }
}

impl ResourceScheduler {
    /// `ResourceScheduler(config=None, **kwargs)`: detect the CPU, memory and disk resources.
    pub fn new(config: Option<Kwargs>, kwargs: Kwargs) -> Self {
        let scheduler = ResourceScheduler {
            config: init_config(config, kwargs),
            progress_tracker: enabled_progress_tracker(),
            resources: Mutex::new(IndexMap::new()),
            allocations: Mutex::new(IndexMap::new()),
            lock: RLock::new(),
            logger: get_logger("resource_scheduler"),
        };
        scheduler.initialize_resources();
        scheduler
    }

    /// `_initialize_resources()`.
    fn initialize_resources(&self) {
        let mut resources = lock(&self.resources);
        match probe_cpu_memory() {
            Some(cm) => {
                let mut cpu = Resource::new(
                    "cpu",
                    ResourceType::Cpu,
                    cm.physical_cores.filter(|n| *n > 0).unwrap_or(1) as f64,
                );
                cpu.metadata.insert(
                    "logical_cores".into(),
                    Value::from(cm.logical_cores.filter(|n| *n > 0).unwrap_or(1)),
                );
                resources.insert("cpu".into(), cpu);

                let mut memory = Resource::new("memory", ResourceType::Memory, cm.total / GIB);
                memory
                    .metadata
                    .insert("available_gb".into(), Value::from(cm.available / GIB));
                resources.insert("memory".into(), memory);

                let disk = match probe_disk() {
                    Some((free, total)) => {
                        let mut disk = Resource::new("disk", ResourceType::Disk, free / GIB);
                        disk.metadata
                            .insert("total_gb".into(), Value::from(total / GIB));
                        disk
                    }
                    None => Resource::new("disk", ResourceType::Disk, 100.0),
                };
                resources.insert("disk".into(), disk);
            }
            None => {
                self.logger
                    .warning("psutil not available, using default resource values");
                resources.insert("cpu".into(), Resource::new("cpu", ResourceType::Cpu, 4.0));
                resources.insert(
                    "memory".into(),
                    Resource::new("memory", ResourceType::Memory, 8.0),
                );
                resources.insert(
                    "disk".into(),
                    Resource::new("disk", ResourceType::Disk, 100.0),
                );
            }
        }
    }

    /// `allocate_resources(pipeline, **options)`: allocate `cpu_cores` (default 1) CPU
    /// cores, `memory_gb` (default 1.0) GB of memory and, when `gpu_device` is given, that
    /// GPU, under the scheduler lock.
    ///
    /// A resource without enough capacity is left out; when nothing at all could be
    /// allocated this raises `ValidationError: No resources were allocated - insufficient
    /// capacity`. On any error the allocations made so far are released.
    pub fn allocate_resources(
        &self,
        pipeline: &Pipeline,
        options: &Kwargs,
    ) -> Result<IndexMap<String, ResourceAllocation>, SemanticaError> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("ResourceScheduler"),
            &format!("Allocating resources for pipeline: {}", pipeline.name),
            None,
        );
        let mut allocations: IndexMap<String, ResourceAllocation> = IndexMap::new();
        let gpu_device = options.get("gpu_device").filter(|v| !v.is_none());

        let outcome = (|| {
            {
                let _guard = self.lock.acquire();
                let cpu_cores = numeric_option(options, "cpu_cores", 1.0)?;
                if let Some(allocation) = self.allocate_cpu(cpu_cores, &pipeline.name, None) {
                    allocations.insert("cpu".into(), allocation);
                }

                let memory_gb = numeric_option(options, "memory_gb", 1.0)?;
                if let Some(allocation) = self.allocate_memory(memory_gb, &pipeline.name, None) {
                    allocations.insert("memory".into(), allocation);
                }

                if let Some(device) = gpu_device {
                    let device = device.to_json_default_str();
                    if let Some(allocation) = self.allocate_gpu(device, &pipeline.name, None) {
                        allocations.insert("gpu".into(), allocation);
                    }
                }
            }

            if allocations.is_empty() {
                return Err(SemanticaError::from(ValidationError::new(
                    "No resources were allocated - insufficient capacity",
                )));
            }
            Ok(())
        })();

        if let Err(e) = outcome {
            if !allocations.is_empty() {
                self.release_resources(&allocations);
            }
            tracker.stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
            return Err(e);
        }

        tracker.update_tracking(&tracking_id, None, "Allocating CPU resources...");
        tracker.update_tracking(&tracking_id, None, "Allocating memory resources...");
        if gpu_device.is_some() {
            tracker.update_tracking(&tracking_id, None, "Allocating GPU resources...");
        }
        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Allocated {} resource(s) for pipeline: {}",
                allocations.len(),
                pipeline.name
            ),
            None,
        );
        Ok(allocations)
    }

    /// `allocation_id` for a new allocation.
    fn allocation_id(kind: &str, pipeline_id: &str, step_name: Option<&str>) -> String {
        format!(
            "{kind}_{pipeline_id}_{}_{}",
            step_name.filter(|s| !s.is_empty()).unwrap_or("default"),
            float_repr(time_now())
        )
    }

    /// `allocate_cpu(cores, pipeline_id, step_name=None)`: allocate `cores` CPU cores if that
    /// much capacity is free.
    pub fn allocate_cpu(
        &self,
        cores: f64,
        pipeline_id: &str,
        step_name: Option<&str>,
    ) -> Option<ResourceAllocation> {
        self.allocate_amount("cpu", ResourceType::Cpu, cores, pipeline_id, step_name)
    }

    /// `allocate_memory(memory_gb, pipeline_id, step_name=None)`: allocate `memory_gb` GB of
    /// memory if that much capacity is free.
    pub fn allocate_memory(
        &self,
        memory_gb: f64,
        pipeline_id: &str,
        step_name: Option<&str>,
    ) -> Option<ResourceAllocation> {
        self.allocate_amount(
            "memory",
            ResourceType::Memory,
            memory_gb,
            pipeline_id,
            step_name,
        )
    }

    fn allocate_amount(
        &self,
        resource_id: &str,
        resource_type: ResourceType,
        amount: f64,
        pipeline_id: &str,
        step_name: Option<&str>,
    ) -> Option<ResourceAllocation> {
        if !lock(&self.resources).contains_key(resource_id) {
            return None;
        }
        let _guard = self.lock.acquire();
        let mut resources = lock(&self.resources);
        let resource = resources.get_mut(resource_id)?;
        let available = resource.capacity - resource.allocated;
        if available >= amount {
            let allocation = ResourceAllocation {
                step_name: step_name.map(str::to_string),
                ..ResourceAllocation::new(
                    Self::allocation_id(resource_id, pipeline_id, step_name),
                    resource_id,
                    resource_type,
                    amount,
                    pipeline_id,
                )
            };
            resource.allocated += amount;
            drop(resources);
            lock(&self.allocations).insert(allocation.allocation_id.clone(), allocation.clone());
            self.logger.debug(match resource_type {
                ResourceType::Cpu => {
                    format!(
                        "Allocated {} CPU cores to {pipeline_id}",
                        amount_text(amount)
                    )
                }
                _ => format!(
                    "Allocated {} GB memory to {pipeline_id}",
                    amount_text(amount)
                ),
            });
            Some(allocation)
        } else {
            drop(resources);
            self.logger.warning(match resource_type {
                ResourceType::Cpu => format!(
                    "Insufficient CPU resources: requested {}, available {}",
                    amount_text(amount),
                    amount_text(available)
                ),
                _ => format!(
                    "Insufficient memory: requested {} GB, available {} GB",
                    amount_text(amount),
                    amount_text(available)
                ),
            });
            None
        }
    }

    /// `allocate_gpu(device_id, pipeline_id, step_name=None)`: allocate GPU `device_id`
    /// (resource `gpu_<device_id>`, capacity 1, created on first use) unless it is taken.
    pub fn allocate_gpu(
        &self,
        device_id: impl Into<Value>,
        pipeline_id: &str,
        step_name: Option<&str>,
    ) -> Option<ResourceAllocation> {
        let device_id: Value = device_id.into();
        let gpu_resource_id = format!("gpu_{}", py_str(&device_id));
        {
            let mut resources = lock(&self.resources);
            if !resources.contains_key(&gpu_resource_id) {
                let mut resource = Resource::new(gpu_resource_id.clone(), ResourceType::Gpu, 1.0);
                resource
                    .metadata
                    .insert("device_id".into(), device_id.clone());
                resources.insert(gpu_resource_id.clone(), resource);
            }
        }

        let _guard = self.lock.acquire();
        let mut resources = lock(&self.resources);
        let resource = resources.get_mut(&gpu_resource_id)?;
        if resource.allocated < resource.capacity {
            let mut allocation = ResourceAllocation::new(
                Self::allocation_id("gpu", pipeline_id, step_name),
                gpu_resource_id.clone(),
                ResourceType::Gpu,
                1.0,
                pipeline_id,
            );
            allocation.step_name = step_name.map(str::to_string);
            allocation
                .metadata
                .insert("device_id".into(), device_id.clone());
            resource.allocated += 1.0;
            drop(resources);
            lock(&self.allocations).insert(allocation.allocation_id.clone(), allocation.clone());
            self.logger.debug(format!(
                "Allocated GPU {} to {pipeline_id}",
                py_str(&device_id)
            ));
            Some(allocation)
        } else {
            drop(resources);
            self.logger
                .warning(format!("GPU {} is already allocated", py_str(&device_id)));
            None
        }
    }

    /// `release_resources(allocations)`: give back every allocation still live (its amount
    /// returns to the resource, never below zero) and forget it.
    pub fn release_resources(&self, allocations: &IndexMap<String, ResourceAllocation>) {
        // Every change to `allocations` happens under `self.lock`, so checking and removing
        // in two steps is safe; the inner mutexes are never held together.
        let _guard = self.lock.acquire();
        for allocation in allocations.values() {
            if !lock(&self.allocations).contains_key(&allocation.allocation_id) {
                continue;
            }
            if let Some(resource) = lock(&self.resources).get_mut(&allocation.resource_id) {
                resource.allocated -= allocation.amount;
                resource.allocated = if resource.allocated > 0.0 {
                    resource.allocated
                } else {
                    0.0
                };
            }
            lock(&self.allocations).shift_remove(&allocation.allocation_id);
            self.logger
                .debug(format!("Released resource: {}", allocation.resource_id));
        }
    }

    /// `get_resource_usage()`: per resource, `capacity`, `allocated`, `available` and
    /// `utilization_percent`.
    pub fn get_resource_usage(&self) -> Map<String, Value> {
        let _guard = self.lock.acquire();
        let resources = lock(&self.resources);
        let mut usage = Map::new();
        for (resource_id, resource) in resources.iter() {
            let mut entry = Map::new();
            entry.insert("capacity".into(), Value::from(resource.capacity));
            entry.insert("allocated".into(), Value::from(resource.allocated));
            entry.insert(
                "available".into(),
                Value::from(resource.capacity - resource.allocated),
            );
            entry.insert(
                "utilization_percent".into(),
                Value::from(if resource.capacity > 0.0 {
                    resource.allocated / resource.capacity * 100.0
                } else {
                    0.0
                }),
            );
            usage.insert(resource_id.clone(), Value::Object(entry));
        }
        usage
    }

    /// `optimize_resource_allocation(pipeline, **options)`: recommended `cpu_cores` (steps,
    /// capped by the CPU capacity), `memory_gb` (0.5 per step) and `parallel_execution`, with
    /// the current usage as `available_resources`.
    pub fn optimize_resource_allocation(
        &self,
        pipeline: &Pipeline,
        _options: &Kwargs,
    ) -> Map<String, Value> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("ResourceScheduler"),
            &format!(
                "Optimizing resource allocation for pipeline: {}",
                pipeline.name
            ),
            None,
        );
        tracker.update_tracking(&tracking_id, None, "Analyzing pipeline requirements...");
        let step_count = pipeline.steps.len();

        tracker.update_tracking(
            &tracking_id,
            None,
            "Calculating resource recommendations...",
        );
        // min(step_count, capacity) keeps the type of whichever is smaller (an int on ties);
        // a missing CPU resource counts as capacity 0 (an int).
        let cpu_capacity = lock(&self.resources).get("cpu").map(|r| r.capacity);
        let cpu_cores = match cpu_capacity {
            Some(capacity) if capacity < step_count as f64 => Value::from(capacity),
            Some(_) => Value::from(step_count),
            None => Value::from(0),
        };
        let memory_gb = step_count as f64 * 0.5;
        let cpu_text = match &cpu_cores {
            Value::Number(n) => sem_utils::pycompat::fmt::number_str(n),
            other => py_str(other),
        };

        let mut recommendations = Map::new();
        recommendations.insert("cpu_cores".into(), cpu_cores);
        recommendations.insert("memory_gb".into(), Value::from(memory_gb));
        recommendations.insert("parallel_execution".into(), Value::from(step_count > 1));

        let mut result = Map::new();
        result.insert("recommendations".into(), Value::Object(recommendations));
        result.insert(
            "available_resources".into(),
            Value::Object(self.get_resource_usage()),
        );
        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Optimization complete: {cpu_text} CPU cores, {} GB memory recommended",
                float_repr(memory_gb)
            ),
            None,
        );
        result
    }
}
