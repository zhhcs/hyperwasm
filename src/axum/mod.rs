use serde::{Deserialize, Serialize};
pub mod client;
pub mod server;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct ServerArgs {
    #[arg(short, long)]
    pub port: u16,

    #[arg(short, long, default_value_t = 1)]
    pub workers: u8,

    #[arg(short, long, default_value_t = 0)]
    pub start_cpu: u8,

    #[arg(short, long, default_value_t = 0_000)]
    pub timer_us: u64,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct ClientArgs {
    #[arg(short, long)]
    pub local_ip: String,

    #[arg(short, long)]
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
struct StatusQuery {
    uname: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CallConfigRequest {
    pub wasm_name: String,               //指定的wasm文件名
    pub task_unique_name: String,        //实例名称,必须唯一
    pub export_func: String,             //调用的导出函数名称
    pub param_type: String,              //数据类型
    pub params: Vec<String>,             //数组
    pub results_length: String,          //结果长度
    pub expected_execution_time: String, //预期执行时长(必须小于相对截止时间，单位毫秒)
    pub expected_deadline: String,       //相对截止时间(单位毫秒)
}

#[derive(Serialize, Deserialize)]
pub struct TestRequest {
    pub wasm_name: String,         //指定的wasm文件名
    pub export_func: String,       //调用的导出函数名称
    pub param_type: String,        //数据类型
    pub params: Vec<String>,       //数组
    pub results_length: String,    //结果长度
    pub expected_deadline: String, //预期截止时间(单位ms)
}

#[derive(Serialize, Deserialize)]
struct RegisterResponse {
    status: String,
    url: String,
}

#[derive(Serialize, Deserialize)]
struct CallFuncResponse {
    status: String,
    result: String,
}

#[derive(Serialize, Deserialize)]
struct CallWithName {
    wasm_name: String,
}
