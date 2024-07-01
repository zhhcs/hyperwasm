use nix::{sys::stat, unistd};
use std::{
    fs::{self, File},
    io::{self, Read},
    path::PathBuf,
};

pub enum ControllerType {
    CPUSET,
    CPU,
    IO,
    MEMORY,
    HUGETLB,
    PIDS,
    RDMA,
}

impl ControllerType {
    fn to_string(&self) -> String {
        let mut controller = String::new();
        match self {
            ControllerType::CPU => controller.push_str("cpu"),
            ControllerType::CPUSET => controller.push_str("cpuset"),
            ControllerType::IO => controller.push_str("io"),
            ControllerType::MEMORY => controller.push_str("memory"),
            ControllerType::HUGETLB => controller.push_str("hugetlb"),
            ControllerType::RDMA => controller.push_str("rdma"),
            ControllerType::PIDS => controller.push_str("pids"),
        }
        controller
    }
}

#[derive(Debug, Clone)]
pub struct Controllerv2 {
    base: PathBuf,
    name: String,
    path: PathBuf,
}

impl Controllerv2 {
    pub fn get_path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn get_base(&self) -> PathBuf {
        self.base.clone()
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    /// base：cgroup目录
    ///  
    /// name：cgroup名
    pub fn new(base: PathBuf, name: String) -> Self {
        let mut path = base.clone();
        path.push(name.clone().as_str());
        if !path.exists() {
            if let Ok(_) = unistd::mkdir(&path, stat::Mode::S_IRWXU) {};
        }
        Self { name, base, path }
    }

    pub fn set_cpu_limit(&self, percent: u32) {
        assert!(percent >= 1 && percent <= 100);
        self.set_cpu_max(percent * 1000);
    }

    /// 删除cgroup
    pub fn delete(&self) {
        if self.get_path().exists() {
            if let Ok(_) = fs::remove_dir(self.get_path()) {}
            //  else {
            //     tracing::info!("delete failed: {:?}", self.get_path());
            // };
        }
    }

    /// 查看控制器
    ///
    /// is_sub: false 查看cgroup.controllers
    ///
    /// is_sub: true 查看cgroup.subtree_control
    pub fn get_controller(&self, is_sub: bool) -> Result<Vec<String>, io::Error> {
        let mut controllers = String::new();
        let mut path = self.path.clone();
        if is_sub {
            path.push("cgroup.subtree_control");
        } else {
            path.push("cgroup.controllers");
        }
        File::open(path)?.read_to_string(&mut controllers)?;
        let mut vc = vec![];
        for c in controllers.trim().split_whitespace() {
            vc.push(c.to_string());
        }
        Ok(vc)
    }

    pub fn contains(&self, ctype: &ControllerType, is_sub: bool) -> bool {
        let mut controller = String::new();
        match ctype {
            ControllerType::CPU => controller.push_str("cpu"),
            ControllerType::CPUSET => controller.push_str("cpuset"),
            ControllerType::IO => controller.push_str("io"),
            ControllerType::MEMORY => controller.push_str("memory"),
            ControllerType::HUGETLB => controller.push_str("hugetlb"),
            ControllerType::RDMA => controller.push_str("rdma"),
            ControllerType::PIDS => controller.push_str("pids"),
        }
        if let Ok(controller_type) = self.get_controller(is_sub) {
            controller_type.contains(&controller)
        } else {
            false
        }
    }

    pub fn set_sub_controller(
        &self,
        ctype: Vec<ControllerType>,
        ctype_remove: Option<Vec<ControllerType>>,
    ) {
        let mut str = String::new();
        for c in ctype {
            if self.contains(&c, false) {
                str.push('+');
                str.push_str(c.to_string().as_str());
                str.push(' ');
            }
        }
        if let Some(ctype_remove) = ctype_remove {
            let mut str_r = String::new();
            for cr in ctype_remove {
                if self.contains(&cr, true) {
                    str_r.push('-');
                    str_r.push_str(cr.to_string().as_str());
                    str_r.push(' ');
                }
            }
            str.push_str(&str_r);
        };
        let mut path = self.path.clone();
        path.push("cgroup.subtree_control");
        if let Ok(_) = fs::write(path, str.to_string().as_bytes()) {};
    }

    /// period 1000~100000
    fn set_cpu_max(&self, period: u32) {
        assert!(period >= 1000 && period <= 100000);
        let mut path = self.path.clone();
        path.push("cpu.max");
        if let Ok(_) = fs::write(path, period.to_string()) {};
    }

    pub fn cpu_max(&self) -> Result<String, io::Error> {
        let mut max = String::new();
        let mut path = self.path.clone();
        path.push("cpu.max");
        File::open(path)?.read_to_string(&mut max)?;
        Ok(max)
    }

    /// weight 1~10000
    pub fn set_cpu_weight(&self, weight: u32) {
        assert!(weight >= 1 && weight <= 10000);
        let mut path = self.path.clone();
        path.push("cpu.weight");
        if let Ok(_) = fs::write(path, weight.to_string()) {};
    }

    pub fn cpu_weight(&self) -> Result<String, io::Error> {
        let mut weight = String::new();
        let mut path = self.path.clone();
        path.push("cpu.weight");
        File::open(path)?.read_to_string(&mut weight)?;
        Ok(weight)
    }

    pub fn set_cgroup_procs(&self, pid: unistd::Pid) {
        let mut path = self.path.clone();
        path.push("cgroup.procs");
        if let Ok(_) = fs::write(path, pid.to_string()) {};
    }

    pub fn cgroup_procs(&self) -> Result<String, io::Error> {
        let mut procs = String::new();
        let mut path = self.path.clone();
        path.push("cgroup.procs");
        File::open(path)?.read_to_string(&mut procs)?;
        Ok(procs)
        // Ok(procs.parse::<u32>().unwrap_or(0))
    }

    pub fn set_cgroup_threads(&self, tid: unistd::Pid) {
        let mut path = self.path.clone();
        path.push("cgroup.threads");
        if let Ok(_) = fs::write(path, tid.to_string()) {};
    }

    pub fn cgroup_threads(&self) -> Result<String, io::Error> {
        let mut threads = String::new();
        let mut path = self.path.clone();
        path.push("cgroup.threads");
        File::open(path)?.read_to_string(&mut threads)?;
        Ok(threads)
        // Ok(threads.parse::<u32>().unwrap_or(0))
    }

    /// cgroup.type
    pub fn set_threaded(&self) {
        let mut path = self.path.clone();
        path.push("cgroup.type");
        if let Ok(_) = fs::write(path, "threaded".to_string()) {};
    }

    pub fn get_cgroup_type(&self) -> Result<String, io::Error> {
        let mut cgroup_type = String::new();
        let mut path = self.path.clone();
        path.push("cgroup.type");
        File::open(path)?.read_to_string(&mut cgroup_type)?;
        Ok(cgroup_type)
    }

    /// FIXME:临时写的，只支持连续的CPU
    pub fn set_cpuset(&self, cpuset1: u8, cpuset2: Option<u8>) {
        let mut path = self.path.clone();
        path.push("cpuset.cpus");
        let mut contents = String::new();
        contents.push_str(cpuset1.to_string().as_str());
        if let Some(cpu) = cpuset2 {
            contents.push('-');
            contents.push_str(cpu.to_string().as_str());
        }
        if let Ok(_) = fs::write(path, contents) {};
    }
}

impl Drop for Controllerv2 {
    fn drop(&mut self) {
        self.delete()
    }
}
