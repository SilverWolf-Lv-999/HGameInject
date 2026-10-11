use std::collections::BTreeSet;
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

const NAME: &str = "NoCoolDown";
const FLAGS: u32 = EventKind::Update.flag()
    | EventKind::Open.flag()
    | EventKind::Close.flag()
    | EventKind::Config.flag();
const DEFAULT_NAMES: &str = "time";
const DEFAULT_FSMS: &str = "冷却,攻击,空中攻击,冲刺,施法,喝药,使用法术,传送,快速下落,跳";
const DEFAULT_VALUE: f64 = 0.0;
const REFRESH_TICKS: u64 = 120;
const SCHEMA: &[ConfigSchema] = &[
    ConfigSchema::text("variableNames", "冷却变量名", DEFAULT_NAMES),
    ConfigSchema::text("fsmNames", "能力状态机名", DEFAULT_FSMS),
    ConfigSchema::range("value", "强制值", DEFAULT_VALUE, 0.0, 1000.0, 0.5),
];

pub fn harness() -> Harness {
    Harness::new(
        ModuleInfo::new(
            NAME,
            "把玩家能力的冷却计时器强制归零，移除冷却间隔",
            ModuleCategory::Combat,
            0,
            FLAGS,
        ),
        |context| Box::new(NoCoolDownModule::new(context)),
    )
}

pub struct NoCoolDownModule {
    context: RwLock<ModuleContext>,
    handler: RwLock<Option<PlayMakerHandler>>,
    targets: RwLock<Vec<FsmVariable>>,
    patterns: RwLock<Vec<String>>,
    fsmPatterns: RwLock<Vec<String>>,
    value: RwLock<f64>,
    hooked: AtomicBool,
    ticks: AtomicU64,
    nextRefresh: AtomicU64,
    signature: RwLock<String>,
}

impl NoCoolDownModule {
    fn new(context: &ModuleContext) -> NoCoolDownModule {
        NoCoolDownModule {
            context: RwLock::new(context.clone()),
            handler: RwLock::new(None),
            targets: RwLock::new(Vec::new()),
            patterns: RwLock::new(vec![DEFAULT_NAMES.to_owned()]),
            fsmPatterns: RwLock::new(split_patterns(DEFAULT_FSMS)),
            value: RwLock::new(DEFAULT_VALUE),
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
        let fsms: String = state.read("fsmNames", DEFAULT_FSMS.to_owned());
        let value: f64 = state.read("value", DEFAULT_VALUE);
        let patterns = split_patterns(&names);
        let fsmPatterns = split_patterns(&fsms);
        *self.patterns.write() = if patterns.is_empty() {
            vec![DEFAULT_NAMES.to_owned()]
        } else {
            patterns
        };
        *self.fsmPatterns.write() = if fsmPatterns.is_empty() {
            split_patterns(DEFAULT_FSMS)
        } else {
            fsmPatterns
        };
        *self.value.write() = MathUtility::clamp(value as f32, 0.0, 1000.0) as f64;
        *self.signature.write() = "reset".to_owned();
    }

    fn refresh(&self, context: &ModuleContext) {
        let mut handler = self.handler.write();
        if handler.is_none() {
            match PlayMakerHandler::create() {
                Ok(created) => *handler = Some(created),
                Err(error) => {
                    let message = format!("no cooldown unavailable: {error}");
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
        let fsmPatterns = self.fsmPatterns.read().clone();
        let value = *self.value.read();
        let discovered = handler.variables();
        let mut selected = Vec::new();
        for variable in &discovered {
            let name = variable.name.to_ascii_lowercase();
            if !patterns.iter().any(|pattern| name.contains(pattern.as_str())) {
                continue;
            }
            let fsm = variable.fsm.to_ascii_lowercase();
            if !fsmPatterns.iter().any(|pattern| fsm.contains(pattern.as_str())) {
                continue;
            }
            selected.push(variable.clone());
        }
        *self.targets.write() = selected.clone();
        playmaker::forceValues(&selected, value);
        self.installHooks(context, handler);
        let Some(logger) = context.logger() else {
            return;
        };
        let (intHits, floatHits) = playmaker::forceHits();
        logger.debug(&format!(
            "no cooldown force hits int={intHits} float={floatHits}"
        ));
        let signature = selected
            .iter()
            .map(|entry| format!("{}@{}", entry.name, entry.fsm))
            .collect::<Vec<String>>()
            .join(",");
        if *self.signature.read() == signature {
            return;
        }
        *self.signature.write() = signature;
        if selected.is_empty() {
            let fsms = discovered
                .iter()
                .map(|entry| entry.fsm.as_str())
                .collect::<BTreeSet<&str>>()
                .into_iter()
                .take(24)
                .collect::<Vec<&str>>()
                .join(", ");
            logger.warn(&format!("no cooldown: no target matched, fsms: {fsms}"));
            return;
        }
        let rendered = selected
            .iter()
            .take(16)
            .map(|entry| format!("{}@{}", entry.name, entry.fsm))
            .collect::<Vec<String>>()
            .join(", ");
        logger.info(&format!("no cooldown forced {value} on {rendered}"));
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
                    logger.info("no cooldown hooks installed");
                }
            }
            Err(error) => {
                if let Some(logger) = context.logger() {
                    logger.warn(&format!("no cooldown hooks unavailable: {error}"));
                }
            }
        }
    }

    fn removeHooks(&self) {
        playmaker::clearForces();
        if !self.hooked.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Some(host) = crate::bootstrap::host() {
            let mut manager = host.hooks().write();
            playmaker::releaseHooks(&mut manager);
        }
    }
}

impl Module for NoCoolDownModule {
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
            logger.info("no cooldown disabled");
        }
    }

    fn on_config(&self, context: &ModuleContext, key: &str) {
        if matches!(key, "variableNames" | "fsmNames" | "value") {
            self.apply(context);
            self.refresh(context);
            if let Some(logger) = context.logger() {
                logger.debug(&format!("no cooldown config applied: {key}"));
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
        let value = *self.value.read();
        let handler = self.handler.read();
        let Some(handler) = handler.as_ref() else {
            return;
        };
        let mut targets = self.targets.write();
        for target in targets.iter_mut() {
            if handler.read(target).map(|current| current != value).unwrap_or(false) {
                handler.write(target, value);
            }
        }
    }
}

fn split_patterns(text: &str) -> Vec<String> {
    text.split(|symbol: char| symbol == ',' || symbol == ';' || symbol == '|' || symbol == ' ')
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .collect()
}
