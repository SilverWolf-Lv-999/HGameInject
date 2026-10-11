use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::modules::bus::EventKind;
use crate::modules::context::ModuleContext;
use crate::modules::harness::Harness;
use crate::modules::info::{ModuleCategory, ModuleInfo};
use crate::modules::runtime::Module;
use crate::modules::schema::ConfigSchema;
use crate::platform::playmaker::{self, FsmVariable, PlayMakerHandler};
use crate::utility::math::MathUtility;

const NAME: &str = "DamageBoost";
const FLAGS: u32 = EventKind::Update.flag()
    | EventKind::Open.flag()
    | EventKind::Close.flag()
    | EventKind::Config.flag();
const DEFAULT_NAMES: &str = "HP,Health,Life,Lives,Vital,Heart";
const DEFAULT_MULTIPLIER: f64 = 2.0;
const REFRESH_TICKS: u64 = 120;
const SCHEMA: &[ConfigSchema] = &[
    ConfigSchema::text("variableNames", "目标生命变量", DEFAULT_NAMES),
    ConfigSchema::range("multiplier", "伤害倍率", DEFAULT_MULTIPLIER, 1.0, 100.0, 0.5),
    ConfigSchema::toggle("sceneOnly", "仅场景对象", true),
];

pub fn harness() -> Harness {
    Harness::new(
        ModuleInfo::new(
            NAME,
            "按倍率放大玩家对目标造成的扣血，提高输出伤害",
            ModuleCategory::Combat,
            0,
            FLAGS,
        ),
        |context| Box::new(DamageBoostModule::new(context)),
    )
}

pub struct DamageBoostModule {
    context: RwLock<ModuleContext>,
    handler: RwLock<Option<PlayMakerHandler>>,
    targets: RwLock<Vec<FsmVariable>>,
    patterns: RwLock<Vec<String>>,
    multiplier: RwLock<f64>,
    sceneOnly: AtomicBool,
    hooked: AtomicBool,
    ticks: AtomicU64,
    nextRefresh: AtomicU64,
    signature: RwLock<String>,
}

impl DamageBoostModule {
    fn new(context: &ModuleContext) -> DamageBoostModule {
        DamageBoostModule {
            context: RwLock::new(context.clone()),
            handler: RwLock::new(None),
            targets: RwLock::new(Vec::new()),
            patterns: RwLock::new(default_patterns()),
            multiplier: RwLock::new(DEFAULT_MULTIPLIER),
            sceneOnly: AtomicBool::new(true),
            hooked: AtomicBool::new(false),
            ticks: AtomicU64::new(0),
            nextRefresh: AtomicU64::new(0),
            signature: RwLock::new("init".to_owned()),
        }
    }

    fn apply(&self, context: &ModuleContext) {
        let Some(state) = context.state() else {
            return;
        };
        let names: String = state.read("variableNames", DEFAULT_NAMES.to_owned());
        let multiplier: f64 = state.read("multiplier", DEFAULT_MULTIPLIER);
        let sceneOnly: bool = state.read("sceneOnly", true);
        let patterns: Vec<String> = names
            .split(|symbol: char| symbol == ',' || symbol == ';' || symbol == '|')
            .map(|entry| entry.trim().to_ascii_lowercase())
            .filter(|entry| !entry.is_empty())
            .collect();
        *self.patterns.write() = if patterns.is_empty() {
            default_patterns()
        } else {
            patterns
        };
        *self.multiplier.write() = MathUtility::clamp(multiplier as f32, 1.0, 100.0) as f64;
        self.sceneOnly.store(sceneOnly, Ordering::Release);
        *self.signature.write() = "reset".to_owned();
    }

    fn refresh(&self, context: &ModuleContext) {
        let mut handler = self.handler.write();
        if handler.is_none() {
            match PlayMakerHandler::create() {
                Ok(created) => *handler = Some(created),
                Err(error) => {
                    let message = format!("damage boost unavailable: {error}");
                    if *self.signature.read() != message {
                        *self.signature.write() = message.clone();
                        if let Some(logger) = context.logger() {
                            logger.warn(&message);
                        }
                    }
                    return;
                }
            }
        }
        let Some(handler) = handler.as_ref() else {
            return;
        };
        handler.attach();
        let patterns = self.patterns.read().clone();
        let multiplier = *self.multiplier.read();
        let sceneOnly = self.sceneOnly.load(Ordering::Acquire);
        let discovered = handler.variables();
        let mut selected = Vec::new();
        for variable in &discovered {
            let name = variable.name.to_ascii_lowercase();
            if name.contains("max") {
                continue;
            }
            if sceneOnly && variable.isGlobal {
                continue;
            }
            if !patterns.iter().any(|pattern| name.contains(pattern.as_str())) {
                continue;
            }
            selected.push(variable.clone());
        }
        *self.targets.write() = selected.clone();
        playmaker::boostValues(&selected, multiplier);
        self.installHooks(context, handler);
        let Some(logger) = context.logger() else {
            return;
        };
        let (intHits, floatHits) = playmaker::boostHits();
        logger.debug(&format!(
            "damage boost hits int={intHits} float={floatHits}"
        ));
        let signature = selected
            .iter()
            .map(|entry| format!("{}@{}", entry.name, entry.fsm))
            .collect::<Vec<String>>()
            .join(",");
        if *self.signature.read() != signature {
            *self.signature.write() = signature;
            if selected.is_empty() {
                logger.warn(&format!(
                    "damage boost: no target matched, {} variables",
                    discovered.len()
                ));
                return;
            }
            let rendered = selected
                .iter()
                .take(12)
                .map(|entry| format!("{}@{}", entry.name, entry.fsm))
                .collect::<Vec<String>>()
                .join(", ");
            logger.info(&format!("damage boost x{multiplier:.2} on {rendered}"));
        }
    }

    fn installHooks(&self, context: &ModuleContext, handler: &PlayMakerHandler) {
        if self.hooked.load(Ordering::Acquire) {
            return;
        }
        let Some(host) = crate::bootstrap::host() else {
            return;
        };
        let mut manager = host.hooks().write();
        match playmaker::requireHooks(handler, &mut manager) {
            Ok(()) => {
                self.hooked.store(true, Ordering::Release);
                if let Some(logger) = context.logger() {
                    logger.info("damage boost hooks installed");
                }
            }
            Err(error) => {
                if let Some(logger) = context.logger() {
                    logger.warn(&format!("damage boost hooks unavailable: {error}"));
                }
            }
        }
    }

    fn removeHooks(&self) {
        playmaker::clearBoosts();
        if !self.hooked.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Some(host) = crate::bootstrap::host() {
            let mut manager = host.hooks().write();
            playmaker::releaseHooks(&mut manager);
        }
    }
}

impl Module for DamageBoostModule {
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
        self.ticks.store(0, Ordering::Release);
        self.nextRefresh.store(0, Ordering::Release);
        *self.handler.write() = None;
        self.targets.write().clear();
        self.refresh(context);
    }

    fn on_disable(&self, context: &ModuleContext) {
        self.removeHooks();
        if let Some(logger) = context.logger() {
            logger.info("damage boost disabled");
        }
    }

    fn on_config(&self, context: &ModuleContext, key: &str) {
        if matches!(key, "variableNames" | "multiplier" | "sceneOnly") {
            self.apply(context);
            self.refresh(context);
            if let Some(logger) = context.logger() {
                logger.debug(&format!("damage boost config applied: {key}"));
            }
        }
    }

    fn on_event(&self, context: &mut ModuleContext, kind: EventKind) {
        if kind != EventKind::Update {
            return;
        }
        let ticks = self.ticks.fetch_add(1, Ordering::AcqRel) + 1;
        if ticks >= self.nextRefresh.load(Ordering::Acquire) {
            self.nextRefresh
                .store(ticks + REFRESH_TICKS, Ordering::Release);
            self.refresh(context);
        }
    }
}

fn default_patterns() -> Vec<String> {
    DEFAULT_NAMES
        .split(',')
        .map(|entry| entry.trim().to_ascii_lowercase())
        .collect()
}
