use std::sync::Arc;

use crate::modules::context::ModuleContext;
use crate::modules::info::ModuleInfo;
use crate::modules::manager::ModuleManager;
use crate::modules::runtime::Module;

pub struct Harness {
    pub info: ModuleInfo,
    pub create: fn(&ModuleContext) -> Box<dyn Module>,
}

impl Harness {
    pub const fn new(
        info: ModuleInfo,
        create: fn(&ModuleContext) -> Box<dyn Module>,
    ) -> Harness {
        Harness { info, create }
    }
}

pub fn register_harnesses(
    manager: &Arc<ModuleManager>,
    host: &std::sync::Weak<dyn crate::modules::context::ModuleHost>,
    harnesses: &[Harness],
) -> Vec<Arc<crate::modules::runtime::TargetModule>> {
    let mut installed = Vec::with_capacity(harnesses.len());
    for harness in harnesses {
        let context = ModuleContext::new(
            host.clone(),
            harness.info,
            harness.info.name.to_owned(),
        );
        let module = (harness.create)(&context);
        installed.push(manager.register(host, harness.info, module));
    }
    installed
}
