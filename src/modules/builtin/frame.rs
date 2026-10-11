use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::inject_log::Level;
use crate::modules::bus::EventKind;
use crate::modules::context::ModuleContext;
use crate::modules::harness::Harness;
use crate::modules::info::{ModuleCategory, ModuleInfo};
use crate::modules::runtime::Module;
use crate::modules::schema::ConfigSchema;

const NAME: &str = "FrameOverlay";
const FLAGS: u32 = EventKind::Frame.flag()
    | EventKind::Update.flag()
    | EventKind::Open.flag()
    | EventKind::Close.flag()
    | EventKind::Config.flag();
const SCHEMA: &[ConfigSchema] = &[ConfigSchema::toggle("showCounter", "显示计数器", true)];

pub fn harness() -> Harness {
    Harness::new(
        ModuleInfo::new(
            NAME,
            "统计帧事件并在日志中输出运行状态",
            ModuleCategory::Render,
            0,
            FLAGS,
        ),
        |context| Box::new(FrameOverlayModule::new(context)),
    )
}

pub struct FrameOverlayModule {
    context: RwLock<ModuleContext>,
    frames: AtomicU64,
}

impl FrameOverlayModule {
    fn new(context: &ModuleContext) -> FrameOverlayModule {
        FrameOverlayModule {
            context: RwLock::new(context.clone()),
            frames: AtomicU64::new(0),
        }
    }

    fn show_counter(&self, context: &ModuleContext) -> bool {
        context
            .state()
            .map(|state| state.read("showCounter", true))
            .unwrap_or(true)
    }
}

impl Module for FrameOverlayModule {
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
        self.frames.store(0, Ordering::Release);
        if let Some(logger) = context.logger() {
            logger.info("frame overlay enabled");
        }
    }

    fn on_disable(&self, context: &ModuleContext) {
        if let Some(logger) = context.logger() {
            logger.info("frame overlay disabled");
        }
    }

    fn on_event(&self, context: &mut ModuleContext, kind: EventKind) {
        match kind {
            EventKind::Frame | EventKind::Update => {
                let frames = self.frames.fetch_add(1, Ordering::AcqRel) + 1;
                if frames % 600 == 0 && self.show_counter(context) {
                    if let Some(logger) = context.logger() {
                        logger.debug(&format!("frame overlay running, frames={frames}"));
                    }
                }
            }
            EventKind::Close => {
                if let Some(logger) = context.logger() {
                    let frames = self.frames.load(Ordering::Acquire);
                    logger.emit(
                        Level::Debug,
                        &format!("frame overlay stopped at {frames} frames"),
                    );
                }
            }
            _ => {}
        }
    }
}
