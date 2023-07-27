use wasmtime::{Instance, Module, Store};

use crate::runtime::Runtime;

pub struct Config {
    path: String,
    expected_execution_time: u64,
    relative_deadline: u64,
    name: String,
    // TODO: params & results
}

impl Config {
    pub fn new(
        path: &str,
        expected_execution_time: u64,
        relative_deadline: u64,
        name: &str,
    ) -> Config {
        Config {
            path: path.to_string(),
            expected_execution_time,
            relative_deadline,
            name: name.to_string(),
        }
    }
}

pub fn run_wasm(rt: &Runtime, config: Config) -> wasmtime::Result<()> {
    let mut store = Store::<()>::default();
    let module = Module::from_file(store.engine(), config.path)?;
    let instance = Instance::new(&mut store, &module, &[])?;

    let name = instance.get_typed_func::<i32, i32>(&mut store, &config.name)?;

    let func = move || {
        if let Ok(res) = name.call(&mut store, 10000000) {
            println!("res: {}", res);
        }
    };

    let mut expected_execution_time = None;
    let mut relative_deadline = None;
    if config.expected_execution_time != 0 && config.relative_deadline != 0 {
        expected_execution_time = Some(std::time::Duration::from_millis(
            config.expected_execution_time,
        ));
        relative_deadline = Some(std::time::Duration::from_millis(config.relative_deadline));
    }

    rt.spawn(func, expected_execution_time, relative_deadline);

    Ok(())
}
