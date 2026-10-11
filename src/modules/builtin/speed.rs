use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::modules::bus::EventKind;
use crate::modules::context::ModuleContext;
use crate::modules::harness::Harness;
use crate::modules::info::{ModuleCategory, ModuleInfo};
use crate::modules::runtime::Module;
use crate::modules::schema::ConfigSchema;

const NAME: &str = "SpeedOverride";
const FLAGS: u32 = EventKind::Update.flag()
    | EventKind::Open.flag()
    | EventKind::Close.flag()
    | EventKind::Config.flag();
const DEFAULT_SPEED: f64 = 1.0;
const SCHEMA: &[ConfigSchema] = &[
    ConfigSchema::range("speed", "速度倍率", DEFAULT_SPEED, 0.1, 10.0, 0.05),
    ConfigSchema::toggle("keepAlive", "后台保持", false),
];

pub fn harness() -> Harness {
    Harness::new(
        ModuleInfo::new(
            NAME,
            "维护可配置的速度倍率，供宿主 Hook 读取",
            ModuleCategory::Movement,
            0,
            FLAGS,
        ),
        |context| Box::new(SpeedOverrideModule::new(context)),
    )
}

pub struct SpeedOverrideModule {
    context: RwLock<ModuleContext>,
    speed: RwLock<f64>,
    ticks: AtomicU64,
}

impl SpeedOverrideModule {
    fn new(context: &ModuleContext) -> SpeedOverrideModule {
        SpeedOverrideModule {
            context: RwLock::new(context.clone()),
            speed: RwLock::new(DEFAULT_SPEED),
            ticks: AtomicU64::new(0),
        }
    }

    pub fn speed(&self) -> f64 {
        *self.speed.read()
    }

    fn apply(&self, context: &ModuleContext) {
        let Some(state) = context.state() else {
            return;
        };
        let speed: f64 = state.read("speed", DEFAULT_SPEED);
        *self.speed.write() =
            crate::utility::math::MathUtility::clamp(speed as f32, 0.1, 10.0) as f64;
    }
}

impl Module for SpeedOverrideModule {
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
        self.apply(context);
        if let Some(logger) = context.logger() {
            logger.info(&format!("speed override enabled at {:.2}", self.speed()));
        }
    }

    fn on_disable(&self, context: &ModuleContext) {
        if let Some(logger) = context.logger() {
            logger.info("speed override disabled");
        }
    }

    fn on_config(&self, context: &ModuleContext, key: &str) {
        if key == "speed" {
            self.apply(context);
        }
    }

    fn on_event(&self, context: &mut ModuleContext, kind: EventKind) {
        if kind != EventKind::Update {
            return;
        }
        let ticks = self.ticks.fetch_add(1, Ordering::AcqRel) + 1;
        if ticks % 600 == 0 {
            if let Some(state) = context.state() {
                state.write("speed", &self.speed());
            }
        }
    }
}
