use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::modules::bus::EventKind;
use crate::modules::context::ModuleContext;
use crate::modules::harness::Harness;
use crate::modules::info::{ModuleCategory, ModuleInfo};
use crate::modules::runtime::Module;
use crate::modules::schema::ConfigSchema;

const NAME: &str = "TargetInspector";
const FLAGS: u32 = EventKind::Update.flag()
    | EventKind::Open.flag()
    | EventKind::Close.flag()
    | EventKind::Config.flag();
const DEFAULT_PATTERN: &str = "48 8B ?? E8";
const SCHEMA: &[ConfigSchema] = &[
    ConfigSchema::text("pattern", "特征码", DEFAULT_PATTERN),
    ConfigSchema::toggle("reportOnce", "只报告一次", true),
    ConfigSchema::range("maxResults", "最大结果数", 4.0, 1.0, 32.0, 1.0),
];

pub fn harness() -> Harness {
    Harness::new(
        ModuleInfo::new(
            NAME,
            "按特征码在宿主主模块内定位地址，用于模块接入前的目标确认",
            ModuleCategory::World,
            0,
            FLAGS,
        ),
        |context| Box::new(TargetInspectorModule::new(context)),
    )
}

pub struct TargetInspectorModule {
    context: RwLock<ModuleContext>,
    ticks: AtomicU64,
    reported: AtomicU64,
}

impl TargetInspectorModule {
    fn new(context: &ModuleContext) -> TargetInspectorModule {
        TargetInspectorModule {
            context: RwLock::new(context.clone()),
            ticks: AtomicU64::new(0),
            reported: AtomicU64::new(0),
        }
    }
}

impl Module for TargetInspectorModule {
    fn names(&self) -> &'static str {
        NAME
    }

    fn context(&self) -> &RwLock<ModuleContext> {
        &self.context
    }

    fn schema(&self) -> &'static [ConfigSchema] {
        SCHEMA
    }

    fn on_enable(&self, context: &ModuleContext) {
        self.reported.store(0, Ordering::Release);
        if let Some(logger) = context.logger() {
            logger.info("target inspector enabled");
        }
    }

    fn on_disable(&self, context: &ModuleContext) {
        if let Some(logger) = context.logger() {
            logger.info("target inspector disabled");
        }
    }

    fn on_config(&self, context: &ModuleContext, key: &str) {
        if key == "pattern" {
            self.reported.store(0, Ordering::Release);
        }
        if let Some(logger) = context.logger() {
            logger.debug(&format!("target inspector config changed: {key}"));
        }
    }

    fn on_event(&self, context: &mut ModuleContext, kind: EventKind) {
        if kind != EventKind::Update {
            return;
        }
        let ticks = self.ticks.fetch_add(1, Ordering::AcqRel) + 1;
        if ticks % 240 != 0 {
            return;
        }
        let Some(state) = context.state() else {
            return;
        };
        let pattern: String = state.read("pattern", DEFAULT_PATTERN.to_owned());
        let report_once: bool = state.read("reportOnce", true);
        let max_results: f64 = state.read("maxResults", 4.0);
        if report_once && self.reported.load(Ordering::Acquire) > 0 {
            return;
        }
        let Some(client) = crate::client::instance_optional() else {
            return;
        };
        let limit = (max_results as usize).max(1);
        let found = client.find_patterns(&pattern, limit);
        if let Some(logger) = context.logger() {
            if found.is_empty() {
                logger.debug(&format!("target inspector: no match for {pattern}"));
            } else {
                let rendered = found
                    .iter()
                    .map(|address| format!("{address:#x}"))
                    .collect::<Vec<String>>()
                    .join(", ");
                logger.info(&format!("target inspector: {rendered}"));
            }
        }
        self.reported.store(found.len() as u64, Ordering::Release);
    }
}
