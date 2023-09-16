use serde::{Deserialize, Serialize};
pub mod client;
pub mod server;

const PORT: u16 = 3001;

fn get_port() -> u16 {
    PORT
}

#[derive(Serialize, Deserialize)]
struct StatusQuery {
    uname: String,
}

#[derive(Serialize, Deserialize)]
pub struct CallConfigRequest {
    pub wasm_name: String,               //指定的wasm文件名
    pub task_unique_name: String,        //实例名称,必须唯一
    pub export_func: String,             //调用的导出函数名称
    pub param_type: String,              //数据类型
    pub params: Vec<String>,             //数组
    pub results_length: String,          //结果长度
    pub expected_execution_time: String, //预期执行时长(必须小于相对截止时间，单位毫秒)
    pub relative_deadline: String,       //相对截止时间(单位毫秒)
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
