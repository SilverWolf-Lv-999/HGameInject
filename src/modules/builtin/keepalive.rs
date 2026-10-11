use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::modules::bus::EventKind;
use crate::modules::context::ModuleContext;
use crate::modules::harness::Harness;
use crate::modules::info::{ModuleCategory, ModuleInfo};
use crate::modules::runtime::Module;
use crate::modules::schema::ConfigSchema;
use crate::platform::playmaker::{self, PlayMakerHandler};
use crate::utility::math::MathUtility;

const NAME: &str = "KeepAlive";
const FLAGS: u32 = EventKind::Update.flag()
    | EventKind::Open.flag()
    | EventKind::Close.flag()
    | EventKind::Config.flag();
const DEFAULT_NAMES: &str = "HP,Health,Life,Lives,Vital,Heart";
const DEFAULT_INTERVAL: f64 = 1.0;
const REFRESH_TICKS: u64 = 120;
const SCHEMA: &[ConfigSchema] = &[
    ConfigSchema::text("variableNames", "生命变量名", DEFAULT_NAMES),
    ConfigSchema::range("lockValue", "锁定值(0=自动)", 0.0, 0.0, 1000000.0, 1.0),
    ConfigSchema::range("intervalTicks", "写入间隔", DEFAULT_INTERVAL, 1.0, 120.0, 1.0),
    ConfigSchema::select("scope", "作用范围", "player", &["player", "all"]),
    ConfigSchema::toggle("listVariables", "记录全部变量", false),
];

pub fn harness() -> Harness {
    Harness::new(
        ModuleInfo::new(
            NAME,
            "把玩家 PlayMaker 生命变量持续写回最大值，保持玩家始终满血",
            ModuleCategory::Combat,
            0,
            FLAGS,
        ),
        |context| Box::new(KeepAliveModule::new(context)),
    )
}

pub struct KeepAliveModule {
    context: RwLock<ModuleContext>,
    handler: RwLock<Option<PlayMakerHandler>>,
    hooked: AtomicBool,
    variables: RwLock<Vec<crate::platform::playmaker::FsmVariable>>,
    patterns: RwLock<Vec<String>>,
    lockValue: RwLock<f64>,
    interval: AtomicU64,
    ticks: AtomicU64,
    nextRefresh: AtomicU64,
    failures: AtomicU64,
    signature: RwLock<String>,
    playerOnly: AtomicBool,
    listVariables: AtomicBool,
}

impl KeepAliveModule {
    fn new(context: &ModuleContext) -> KeepAliveModule {
        KeepAliveModule {
            context: RwLock::new(context.clone()),
            handler: RwLock::new(None),
            hooked: AtomicBool::new(false),
            variables: RwLock::new(Vec::new()),
            patterns: RwLock::new(default_patterns()),
            lockValue: RwLock::new(0.0),
            interval: AtomicU64::new(DEFAULT_INTERVAL as u64),
            ticks: AtomicU64::new(0),
            nextRefresh: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            signature: RwLock::new(String::new()),
            playerOnly: AtomicBool::new(true),
            listVariables: AtomicBool::new(false),
        }
    }

    fn apply(&self, context: &ModuleContext) {
        let Some(state) = context.state() else {
            return;
        };
        let names: String = state.read("variableNames", DEFAULT_NAMES.to_owned());
        let lock: f64 = state.read("lockValue", 0.0);
        let interval: f64 = state.read("intervalTicks", DEFAULT_INTERVAL);
        let list: bool = state.read("listVariables", false);
        let scope: String = state.read("scope", "player".to_owned());
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
        *self.lockValue.write() = MathUtility::clamp(lock as f32, 0.0, 1000000.0) as f64;
        self.interval.store(
            MathUtility::clamp(interval as f32, 1.0, 120.0) as u64,
            Ordering::Release,
        );
        self.listVariables.store(list, Ordering::Release);
        self.playerOnly
            .store(!scope.eq_ignore_ascii_case("all"), Ordering::Release);
        crate::platform::playmaker::setTracking(list);
        self.signature.write().clear();
    }

    fn refresh(&self, context: &ModuleContext) {
        let mut handler = self.handler.write();
        if handler.is_none() {
            match PlayMakerHandler::create() {
                Ok(created) => *handler = Some(created),
                Err(error) => {
                    if self.signature.read().as_str() != error {
                        *self.signature.write() = error.clone();
                        let Some(logger) = context.logger() else {
                            return;
                        };
                        logger.warn(&format!("keep alive unavailable: {error}"));
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
        let lock = *self.lockValue.read();
        let listAll = self.listVariables.load(Ordering::Acquire);
        let discovered = handler.variables();
        if listAll {
            crate::platform::playmaker::trackNames(&discovered);
        }
        let total = discovered.len();
        let fsmCount = discovered
            .iter()
            .map(|entry| entry.fsm.as_str())
            .collect::<BTreeSet<&str>>()
            .len();
        let mut variables = self.variables.write();
        let mut selected = Vec::new();
        for variable in &discovered {
            let name = variable.name.to_ascii_lowercase();
            if self.playerOnly.load(Ordering::Acquire) && !variable.isGlobal {
                continue;
            }
            if name.contains("max") {
                continue;
            }
            if !patterns.iter().any(|pattern| name.contains(pattern.as_str())) {
                continue;
            }
            let previous = variables
                .iter()
                .find(|known| known.object == variable.object)
                .map(|known| known.locked);
            let mut reference = discovered
                .iter()
                .find(|candidate| {
                    if candidate.owner != variable.owner || candidate.object == variable.object {
                        return false;
                    }
                    let candidateName = candidate.name.to_ascii_lowercase();
                    candidateName.contains("max") && candidateName.contains(&name)
                })
                .and_then(|candidate| handler.read(candidate))
                .unwrap_or(0.0);
            if reference <= 0.0 {
                reference = discovered
                    .iter()
                    .filter(|candidate| candidate.object != variable.object)
                    .filter(|candidate| {
                        let candidateName = candidate.name.to_ascii_lowercase();
                        candidateName.contains("max") && candidateName.contains(&name)
                    })
                    .filter_map(|candidate| handler.read(candidate))
                    .fold(0.0, f64::max);
            }
            let current = handler.read(variable).unwrap_or(0.0);
            let mut entry = variable.clone();
            entry.locked = if lock > 0.0 {
                lock
            } else {
                reference.max(current).max(previous.unwrap_or(0.0))
            };
            selected.push(entry);
        }
        *variables = selected;
        crate::platform::playmaker::lockValues(&variables);
        self.installHooks(context, handler);
        let Some(logger) = context.logger() else {
            return;
        };
        let (intHits, floatHits) = crate::platform::playmaker::hits();
        let (intCalls, floatCalls) = crate::platform::playmaker::calls();
        logger.debug(&format!(
            "keep alive getter calls int={intCalls} float={floatCalls} hits int={intHits} float={floatHits}"
        ));
        let top = crate::platform::playmaker::topCounts(8)
            .into_iter()
            .map(|entry| format!("{}={}", entry.0, entry.1))
            .collect::<Vec<String>>()
            .join(", ");
        logger.debug(&format!("keep alive getter top: {top}"));
        if listAll {
            let rendered = discovered
                .iter()
                .map(|entry| {
                    let value = handler.read(entry).unwrap_or(0.0);
                    format!("{}@{}={}", entry.name, entry.fsm, value)
                })
                .collect::<Vec<String>>()
                .join(", ");
            logger.debug(&format!("keep alive discovered: {rendered}"));
        }
        if variables.is_empty() {
            let signature = format!("none:{total}:{fsmCount}");
            if *self.signature.read() != signature {
                *self.signature.write() = signature;
                logger.warn(&format!(
                    "keep alive: no playmaker variable matched, {total} variables on {fsmCount} fsms"
                ));
                if listAll {
                    let rendered = discovered
                        .iter()
                        .take(80)
                        .map(|entry| {
                            let value = handler.read(entry).unwrap_or(0.0);
                            format!("{}={}", entry.name, value)
                        })
                        .collect::<Vec<String>>()
                        .join(", ");
                    logger.info(&format!("keep alive variables: {rendered}"));
                }
            }
            return;
        }
        let signature = variables
            .iter()
            .map(|entry| format!("{}@{}={}", entry.name, entry.fsm, entry.locked))
            .collect::<Vec<String>>()
            .join(",");
        if *self.signature.read() != signature {
            *self.signature.write() = signature;
            let rendered = variables
                .iter()
                .take(12)
                .map(|entry| {
                    format!(
                        "{}@{}({})={}",
                        entry.name,
                        entry.fsm,
                        if entry.isFloat { "float" } else { "int" },
                        entry.locked
                    )
                })
                .collect::<Vec<String>>()
                .join(", ");
            logger.info(&format!("keep alive locked {rendered}"));
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
                    logger.info("keep alive hooks installed");
                }
            }
            Err(error) => {
                if let Some(logger) = context.logger() {
                    logger.warn(&format!("keep alive hooks unavailable: {error}"));
                }
            }
        }
    }

    fn removeHooks(&self) {
        playmaker::clearValues();
        if !self.hooked.swap(false, Ordering::AcqRel) {
            return;
        }
        if let Some(host) = crate::bootstrap::host() {
            let mut manager = host.hooks().write();
            playmaker::releaseHooks(&mut manager);
        }
    }
}

impl Module for KeepAliveModule {
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
        self.failures.store(0, Ordering::Release);
        self.signature.write().clear();
        self.nextRefresh.store(0, Ordering::Release);
        *self.handler.write() = None;
        self.variables.write().clear();
        self.refresh(context);
    }

    fn on_disable(&self, context: &ModuleContext) {
        self.removeHooks();
        if let Some(logger) = context.logger() {
            logger.info("keep alive disabled");
        }
    }

    fn on_config(&self, context: &ModuleContext, key: &str) {
        if matches!(
            key,
            "variableNames" | "lockValue" | "intervalTicks" | "listVariables"
        ) {
            self.apply(context);
            self.refresh(context);
            if let Some(logger) = context.logger() {
                logger.debug(&format!("keep alive config applied: {key}"));
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
        if ticks % self.interval.load(Ordering::Acquire).max(1) != 0 {
            return;
        }
        let handler = self.handler.read();
        let Some(handler) = handler.as_ref() else {
            return;
        };
        handler.attach();
        let auto = *self.lockValue.read() <= 0.0;
        let mut variables = self.variables.write();
        let mut lost = 0usize;
        for variable in variables.iter_mut() {
            let Some(current) = handler.read(variable) else {
                lost += 1;
                continue;
            };
            if auto && current > variable.locked {
                variable.locked = current;
            }
            if current < variable.locked && !handler.write(variable, variable.locked) {
                lost += 1;
            }
        }
        if lost > 0 {
            let failures = self.failures.fetch_add(1, Ordering::AcqRel) + 1;
            if failures == 1 {
                if let Some(logger) = context.logger() {
                    logger.warn(&format!("keep alive lost {lost} variables, rescan queued"));
                }
            }
        }
    }
}

fn default_patterns() -> Vec<String> {
    DEFAULT_NAMES
        .split(',')
        .map(|entry| entry.trim().to_ascii_lowercase())
        .collect()
}
