//! The wasmtime Component Model host.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use wasmtime::component::{Component, Instance, Linker};
use wasmtime::{Config, Engine, ResourceLimiter, Store, StoreLimits, StoreLimitsBuilder, Trap};

use crate::error::SandboxError;
use crate::host::{HostBridge, HostError};
use crate::limits::Limits;

/// The imported interface name (`wit/host.wit`).
const HOST_INTERFACE: &str = "fergents:host/host@1.0.0";
/// How often the epoch ticker fires.
const TICK: Duration = Duration::from_millis(50);

/// A compiled component, reusable across calls.
#[derive(Clone)]
pub struct ComponentHandle {
    inner: Arc<Component>,
    byte_len: usize,
}

impl ComponentHandle {
    /// The size of the original component in bytes.
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }
}

/// The sandbox: one engine, one epoch ticker, many component handles.
pub struct Sandbox {
    engine: Engine,
    stop: Arc<AtomicBool>,
    ticker: Option<JoinHandle<()>>,
}

impl Sandbox {
    /// Create the engine and start the epoch ticker.
    pub fn new() -> Result<Self, SandboxError> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|err| SandboxError::Engine(err.to_string()))?;

        let stop = Arc::new(AtomicBool::new(false));
        let tick_engine = engine.clone();
        let tick_stop = stop.clone();
        let ticker = std::thread::Builder::new()
            .name("fergents-wasm-epoch".to_string())
            .spawn(move || {
                while !tick_stop.load(Ordering::Relaxed) {
                    std::thread::sleep(TICK);
                    tick_engine.increment_epoch();
                }
            })
            .map_err(|err| SandboxError::Engine(err.to_string()))?;

        Ok(Self {
            engine,
            stop,
            ticker: Some(ticker),
        })
    }

    /// Compile a component from bytes.
    pub fn load(&self, bytes: &[u8]) -> Result<ComponentHandle, SandboxError> {
        let component = Component::from_binary(&self.engine, bytes)
            .map_err(|err| SandboxError::Invalid(err.to_string()))?;
        Ok(ComponentHandle {
            inner: Arc::new(component),
            byte_len: bytes.len(),
        })
    }

    /// Invoke a `skill` component's `run` export.
    pub fn invoke_skill(
        &self,
        component: &ComponentHandle,
        bridge: Arc<dyn HostBridge>,
        input: &str,
        limits: &Limits,
    ) -> Result<String, SandboxError> {
        check_input(input, limits)?;
        let mut store = self.store(bridge, limits)?;
        let instance = self.instantiate(&mut store, &component.inner)?;
        let func = instance
            .get_typed_func::<(String,), (Result<String, String>,)>(&mut store, "run")
            .map_err(|_| SandboxError::MissingExport("run".to_string()))?;
        let (result,) = func
            .call(&mut store, (input.to_string(),))
            .map_err(map_call_error)?;
        let output = result.map_err(SandboxError::Component)?;
        check_output(&output, limits)?;
        Ok(output)
    }

    /// Call a plugin `init` export.
    pub fn init_plugin(
        &self,
        component: &ComponentHandle,
        bridge: Arc<dyn HostBridge>,
        limits: &Limits,
    ) -> Result<(), SandboxError> {
        let mut store = self.store(bridge, limits)?;
        let instance = self.instantiate(&mut store, &component.inner)?;
        match instance.get_typed_func::<(), (Result<(), String>,)>(&mut store, "init") {
            Ok(func) => {
                let (result,) = func.call(&mut store, ()).map_err(map_call_error)?;
                result.map_err(SandboxError::Component)
            }
            // A plugin without `init` is valid; it simply has no setup.
            Err(_) => Ok(()),
        }
    }

    /// Return the plugin's declared tools as JSON.
    pub fn list_tools(
        &self,
        component: &ComponentHandle,
        bridge: Arc<dyn HostBridge>,
        limits: &Limits,
    ) -> Result<String, SandboxError> {
        let mut store = self.store(bridge, limits)?;
        let instance = self.instantiate(&mut store, &component.inner)?;
        let func = instance
            .get_typed_func::<(), (Result<String, String>,)>(&mut store, "list-tools")
            .map_err(|_| SandboxError::MissingExport("list-tools".to_string()))?;
        let (result,) = func.call(&mut store, ()).map_err(map_call_error)?;
        let output = result.map_err(SandboxError::Component)?;
        check_output(&output, limits)?;
        Ok(output)
    }

    /// Execute one plugin tool.
    pub fn call_tool(
        &self,
        component: &ComponentHandle,
        bridge: Arc<dyn HostBridge>,
        name: &str,
        input: &str,
        limits: &Limits,
    ) -> Result<String, SandboxError> {
        check_input(input, limits)?;
        let mut store = self.store(bridge, limits)?;
        let instance = self.instantiate(&mut store, &component.inner)?;
        let func = instance
            .get_typed_func::<(String, String), (Result<String, String>,)>(&mut store, "call-tool")
            .map_err(|_| SandboxError::MissingExport("call-tool".to_string()))?;
        let (result,) = func
            .call(&mut store, (name.to_string(), input.to_string()))
            .map_err(map_call_error)?;
        let output = result.map_err(SandboxError::Component)?;
        check_output(&output, limits)?;
        Ok(output)
    }

    /// Deliver an event to a plugin.
    pub fn on_event(
        &self,
        component: &ComponentHandle,
        bridge: Arc<dyn HostBridge>,
        topic: &str,
        payload: &str,
        limits: &Limits,
    ) -> Result<(), SandboxError> {
        check_input(payload, limits)?;
        let mut store = self.store(bridge, limits)?;
        let instance = self.instantiate(&mut store, &component.inner)?;
        match instance
            .get_typed_func::<(String, String), (Result<(), String>,)>(&mut store, "on-event")
        {
            Ok(func) => {
                let (result,) = func
                    .call(&mut store, (topic.to_string(), payload.to_string()))
                    .map_err(map_call_error)?;
                result.map_err(SandboxError::Component)
            }
            Err(_) => Ok(()),
        }
    }

    /// Handle a plugin command.
    pub fn on_command(
        &self,
        component: &ComponentHandle,
        bridge: Arc<dyn HostBridge>,
        command: &str,
        args: &str,
        limits: &Limits,
    ) -> Result<String, SandboxError> {
        check_input(args, limits)?;
        let mut store = self.store(bridge, limits)?;
        let instance = self.instantiate(&mut store, &component.inner)?;
        let func = instance
            .get_typed_func::<(String, String), (Result<String, String>,)>(&mut store, "on-command")
            .map_err(|_| SandboxError::MissingExport("on-command".to_string()))?;
        let (result,) = func
            .call(&mut store, (command.to_string(), args.to_string()))
            .map_err(map_call_error)?;
        let output = result.map_err(SandboxError::Component)?;
        check_output(&output, limits)?;
        Ok(output)
    }

    /// Call the plugin's `on-shutdown` export when present.
    pub fn shutdown_plugin(
        &self,
        component: &ComponentHandle,
        bridge: Arc<dyn HostBridge>,
        limits: &Limits,
    ) -> Result<(), SandboxError> {
        let mut store = self.store(bridge, limits)?;
        let instance = self.instantiate(&mut store, &component.inner)?;
        match instance.get_typed_func::<(), ()>(&mut store, "on-shutdown") {
            Ok(func) => func.call(&mut store, ()).map_err(map_call_error),
            Err(_) => Ok(()),
        }
    }

    fn instantiate(
        &self,
        store: &mut Store<HostState>,
        component: &Component,
    ) -> Result<Instance, SandboxError> {
        let mut linker: Linker<HostState> = Linker::new(&self.engine);
        {
            let mut host = linker
                .instance(HOST_INTERFACE)
                .map_err(|err| SandboxError::Engine(err.to_string()))?;
            host.func_wrap(
                "call",
                |store: wasmtime::StoreContextMut<'_, HostState>,
                 (op, payload): (String, String)|
                 -> wasmtime::Result<(Result<String, String>,)> {
                    let bridge = store.data().bridge.clone();
                    Ok((bridge
                        .call(&op, &payload)
                        .map_err(|err: HostError| format!("{}: {}", err.code, err.message)),))
                },
            )
            .map_err(|err| SandboxError::Engine(err.to_string()))?;
        }
        linker
            .instantiate(store, component)
            .map_err(|err| SandboxError::Invalid(err.to_string()))
    }

    fn store(
        &self,
        bridge: Arc<dyn HostBridge>,
        limits: &Limits,
    ) -> Result<Store<HostState>, SandboxError> {
        let store_limits = StoreLimitsBuilder::new()
            .memory_size(limits.memory_bytes)
            .build();
        let mut store = Store::new(
            &self.engine,
            HostState {
                bridge,
                limits: store_limits,
            },
        );
        store
            .set_fuel(limits.fuel)
            .map_err(|err| SandboxError::Engine(err.to_string()))?;
        let ticks = limits.wall_time_ms.div_ceil(TICK.as_millis() as u64).max(1);
        store.set_epoch_deadline(ticks);
        store.epoch_deadline_trap();
        store.limiter(|state| &mut state.limits);
        Ok(store)
    }
}

struct HostState {
    bridge: Arc<dyn HostBridge>,
    limits: StoreLimits,
}

impl ResourceLimiter for HostState {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.limits.memory_growing(current, desired, maximum)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.limits.table_growing(current, desired, maximum)
    }
}

fn check_input(input: &str, limits: &Limits) -> Result<(), SandboxError> {
    if input.len() > limits.max_input_bytes {
        return Err(SandboxError::TooLarge(format!(
            "input is {} bytes (max {})",
            input.len(),
            limits.max_input_bytes
        )));
    }
    Ok(())
}

fn check_output(output: &str, limits: &Limits) -> Result<(), SandboxError> {
    if output.len() > limits.max_output_bytes {
        return Err(SandboxError::TooLarge(format!(
            "output is {} bytes (max {})",
            output.len(),
            limits.max_output_bytes
        )));
    }
    Ok(())
}

fn map_call_error(err: wasmtime::Error) -> SandboxError {
    if let Some(trap) = err.downcast_ref::<Trap>() {
        return match trap {
            Trap::OutOfFuel => SandboxError::FuelExhausted,
            Trap::Interrupt => SandboxError::Timeout,
            other => SandboxError::Trap(other.to_string()),
        };
    }
    SandboxError::Trap(err.to_string())
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
    }
}
